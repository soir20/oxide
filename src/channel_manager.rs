use crate::game_server::Broadcast;
use crate::protocol::Channel;
use crate::ServerOptions;
use crossbeam_channel::Sender;
use parking_lot::{Mutex, MutexGuard};
use std::collections::BTreeMap;
use std::net::SocketAddr;

#[derive(Eq, PartialEq)]
pub enum ReceiveResult {
    Success(u32),
    CreateChannelFirst,
}

pub struct TooManyChannels(pub usize, pub Channel);

pub struct ChannelManager {
    unauthenticated: BTreeMap<SocketAddr, Mutex<Channel>>,
    authenticated: AuthenticatedChannelManager,
    max_sessions: usize,
}

impl ChannelManager {
    pub fn new(max_sessions: usize) -> Self {
        ChannelManager {
            unauthenticated: Default::default(),
            authenticated: Default::default(),
            max_sessions,
        }
    }

    pub fn get_by_addr(&self, addr: &SocketAddr) -> Option<&Mutex<Channel>> {
        self.unauthenticated
            .get(addr)
            .or(self.authenticated.get_by_addr(addr))
    }

    pub fn get_by_guid(&self, guid: u32) -> Option<&Mutex<Channel>> {
        self.authenticated.get_by_guid(guid)
    }

    pub fn guid(&self, addr: &SocketAddr) -> Option<u32> {
        self.authenticated.guid(addr)
    }

    pub fn insert(
        &mut self,
        addr: &SocketAddr,
        channel: Channel,
    ) -> Result<Option<Mutex<Channel>>, TooManyChannels> {
        let previous = self
            .unauthenticated
            .remove(addr)
            .or(self.authenticated.remove(addr).map(|(_, channel)| channel));

        if self.len() < self.max_sessions {
            self.unauthenticated.insert(*addr, Mutex::new(channel));
            Ok(previous)
        } else {
            Err(TooManyChannels(self.max_sessions, channel))
        }
    }

    pub fn authenticate(&mut self, addr: &SocketAddr, guid: u32) -> Option<Mutex<Channel>> {
        let channel = self
            .unauthenticated
            .remove(addr)
            .expect("Tried to authenticate non-existent or already-authenticated channel");
        self.authenticated.insert(addr, guid, channel)
    }

    pub fn receive(
        &self,
        client_enqueue: Sender<SocketAddr>,
        addr: &SocketAddr,
        data: &[u8],
    ) -> ReceiveResult {
        if let Some(channel) = self.get_by_addr(addr) {
            let mut channel_handle = channel.lock();
            let client_not_queued = !channel_handle.needs_processing();

            match channel_handle.receive(data) {
                Ok(packets_received) => {
                    // If the last processing thread did not process all packets, the client is already queued
                    if client_not_queued && packets_received > 0 {
                        client_enqueue
                            .send(*addr)
                            .expect("Tried to enqueue client after queue channel disconnected");
                    }

                    ReceiveResult::Success(packets_received)
                }
                Err(err) => {
                    println!(
                        "Deserialize error on channel {}: {:?}, data={:x?}",
                        addr, err, data
                    );
                    ReceiveResult::Success(0)
                }
            }
        } else {
            ReceiveResult::CreateChannelFirst
        }
    }

    pub fn process_next(
        &self,
        channel_handle: &mut MutexGuard<Channel>,
        count: u8,
        server_options: &ServerOptions,
    ) -> Vec<Vec<u8>> {
        channel_handle.process_next(count, server_options)
    }

    pub fn broadcast(
        &self,
        client_enqueue: Sender<SocketAddr>,
        broadcasts: Vec<Broadcast>,
    ) -> Vec<u32> {
        let mut missing_guids = Vec::new();

        for broadcast in broadcasts {
            let (guids, packets) = match broadcast {
                Broadcast::Single(guid, packets) => (vec![guid], packets),
                Broadcast::Multi(guids, packets) => (guids, packets),
            };

            for guid in guids {
                if let Some(channel) = self.get_by_guid(guid) {
                    let mut channel_handle = channel.lock();
                    let client_not_queued = !channel_handle.needs_processing();

                    packets.iter().for_each(|packet| {
                        channel_handle.prepare_to_send_data(packet.clone());
                    });

                    if client_not_queued {
                        client_enqueue
                            .send(channel_handle.addr)
                            .expect("Tried to enqueue client after queue channel disconnected");
                    }
                } else {
                    missing_guids.push(guid);
                }
            }
        }

        missing_guids
    }

    pub fn send_next(&self, channel_handle: &mut MutexGuard<Channel>, count: u8) -> Vec<Vec<u8>> {
        let send_result = channel_handle.send_next(count);

        send_result.unwrap_or_else(|err| {
            println!("Send error: {:?}", err);
            Vec::new()
        })
    }

    pub fn len(&self) -> usize {
        self.unauthenticated.len() + self.authenticated.len()
    }

    pub fn drain_filter(
        &mut self,
        mut predicate: impl FnMut(&MutexGuard<Channel>) -> bool,
    ) -> Vec<(Option<u32>, Mutex<Channel>)> {
        let mut addrs_to_remove = Vec::new();
        for (addr, channel) in self.unauthenticated.iter() {
            let channel_handle = channel.lock();
            if predicate(&channel_handle) {
                addrs_to_remove.push(*addr);
            }
        }

        let mut removed = Vec::new();
        for addr in addrs_to_remove {
            if let Some(result) = self.unauthenticated.remove(&addr) {
                removed.push((None, result));
            }
        }

        removed.append(
            &mut self
                .authenticated
                .drain_filter(predicate)
                .into_iter()
                .map(|(guid, channel)| (Some(guid), channel))
                .collect(),
        );

        removed
    }
}

#[derive(Default)]
struct AuthenticatedChannelManager {
    socket_to_guid: BTreeMap<SocketAddr, u32>,
    channels: BTreeMap<u32, Mutex<Channel>>,
}

impl AuthenticatedChannelManager {
    pub fn get_by_addr(&self, addr: &SocketAddr) -> Option<&Mutex<Channel>> {
        self.socket_to_guid.get(addr).map(|guid| {
            self.channels
                .get(guid)
                .expect("Entry in socket to GUID mapping has no corresponding channel")
        })
    }
    pub fn get_by_guid(&self, guid: u32) -> Option<&Mutex<Channel>> {
        self.channels.get(&guid)
    }

    pub fn guid(&self, addr: &SocketAddr) -> Option<u32> {
        self.socket_to_guid.get(addr).copied()
    }

    pub fn insert(
        &mut self,
        addr: &SocketAddr,
        guid: u32,
        channel: Mutex<Channel>,
    ) -> Option<Mutex<Channel>> {
        self.socket_to_guid.insert(*addr, guid);
        self.channels.insert(guid, channel)
    }

    pub fn remove(&mut self, addr: &SocketAddr) -> Option<(u32, Mutex<Channel>)> {
        self.socket_to_guid.remove(addr).map(|guid| {
            (
                guid,
                self.channels
                    .remove(&guid)
                    .expect("Entry in socket to GUID mapping has no corresponding channel"),
            )
        })
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn drain_filter(
        &mut self,
        mut predicate: impl FnMut(&MutexGuard<Channel>) -> bool,
    ) -> Vec<(u32, Mutex<Channel>)> {
        let addrs_to_remove: Vec<SocketAddr> = self
            .channels
            .iter()
            .filter_map(|(_, channel)| {
                let channel_handle = channel.lock();
                if predicate(&channel_handle) {
                    Some(channel_handle.addr)
                } else {
                    None
                }
            })
            .collect();

        let mut removed = Vec::new();
        for addr in addrs_to_remove {
            if let Some(result) = self.remove(&addr) {
                removed.push(result);
            }
        }

        removed
    }
}
