use crossbeam_channel::{bounded, tick, Receiver, Sender};
use game_server::Broadcast;
use parking_lot::{Mutex, MutexGuard, RwLock, RwLockReadGuard};
use protocol::{BufferSize, DisconnectReason, MAX_BUFFER_SIZE};
use serde::Deserialize;
use std::fs::File;
use std::io::Error;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::spawn;

use crate::channel_manager::{ChannelManager, ReceiveResult};
use crate::game_server::GameServer;
use crate::protocol::Channel;

mod channel_manager;
mod game_server;
mod http;
mod protocol;

#[tokio::main]
async fn main() {
    let config_dir = Path::new("config");
    let server_options =
        Arc::new(load_server_options(config_dir).expect("Unable to read server options"));
    server_options.validate();

    spawn(http::start(
        server_options.bind_ip,
        server_options.https_port,
        config_dir,
        Path::new("config/custom_assets"),
        PathBuf::from(".asset_cache"),
    ));
    println!("Hello, world!");
    let socket = UdpSocket::bind(SocketAddr::new(
        server_options.bind_ip,
        server_options.udp_port,
    ))
    .expect("couldn't bind to socket");

    let channel_manager = RwLock::new(ChannelManager::new(server_options.max_sessions));
    let game_server = GameServer::new(config_dir).unwrap();

    let channel_manager_arc = Arc::new(channel_manager);
    let socket_arc = Arc::new(socket);
    let game_server_arc = Arc::new(game_server);

    let (client_enqueue, client_dequeue) = bounded(server_options.max_sessions);
    let mut threads = spawn_receive_threads(
        server_options.receive_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue.clone(),
        MAX_BUFFER_SIZE,
        &server_options,
        &game_server_arc,
    );
    threads.append(&mut spawn_process_threads(
        server_options.process_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue.clone(),
        client_dequeue,
        server_options.clone(),
        &game_server_arc,
    ));

    let cleanup_tick_dequeue = tick(Duration::from_millis(
        server_options.channel_cleanup_period_millis,
    ));
    spawn_cleanup_thread(
        &channel_manager_arc,
        &socket_arc,
        cleanup_tick_dequeue,
        client_enqueue,
        &server_options,
        &game_server_arc,
    );

    for thread in threads {
        thread.join().expect("Thread exited with error");
    }
}

#[derive(Clone, Deserialize)]
pub struct ServerOptions {
    pub bind_ip: IpAddr,
    pub udp_port: u16,
    pub https_port: u16,
    pub crc_length: u8,
    pub allow_packet_compression: bool,
    pub receive_threads: u16,
    pub process_threads: u16,
    pub max_sessions: usize,
    pub process_packets_per_cycle: u8,
    pub send_packets_per_cycle: u8,
    pub packet_recency_limit: u16,
    pub default_millis_until_resend: u64,
    pub max_round_trip_entries: usize,
    pub desired_resend_pct: u8,
    pub max_millis_until_resend: u64,
    pub channel_cleanup_period_millis: u64,
    pub channel_inactive_timeout_millis: u64,
}

impl ServerOptions {
    fn validate(&self) {
        if self.crc_length > 4 || self.crc_length < 1 {
            panic!("crc_length must be between 1 and 4 (inclusive)");
        }

        if self.desired_resend_pct >= 100 {
            panic!("desired_resend_pct must be less than 100")
        }
    }
}

fn load_server_options(config_dir: &Path) -> Result<ServerOptions, Error> {
    let mut file = File::open(config_dir.join("server.json"))?;
    Ok(serde_json::from_reader(&mut file)?)
}

fn spawn_receive_threads(
    threads: u16,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: Sender<SocketAddr>,
    initial_buffer_size: BufferSize,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) -> Vec<JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let channel_manager = channel_manager.clone();
            let socket = socket.clone();
            let client_enqueue = client_enqueue.clone();
            let server_options = server_options.clone();
            let game_server = game_server.clone();

            thread::spawn(move || loop {
                let mut buf = [0; MAX_BUFFER_SIZE as usize];
                if let Ok((len, src)) = socket.recv_from(&mut buf) {
                    let recv_data = &buf[0..len];

                    loop {
                        let read_handle = channel_manager.read();
                        let receive_result =
                            read_handle.receive(client_enqueue.clone(), &src, recv_data);
                        if receive_result == ReceiveResult::CreateChannelFirst {
                            println!("Creating channel for {}", src);
                            drop(read_handle);

                            let new_channel = Channel::new(
                                src,
                                initial_buffer_size,
                                server_options.packet_recency_limit,
                                Duration::from_millis(server_options.default_millis_until_resend),
                                server_options.max_round_trip_entries,
                                server_options.desired_resend_pct,
                                Duration::from_millis(server_options.max_millis_until_resend),
                            );
                            let mut write_handle = channel_manager.write();

                            match write_handle.insert(&src, new_channel) {
                                Ok(possible_previous_channel) => {
                                    if let Some(previous_channel) = possible_previous_channel {
                                        println!("Client {} reconnected, dropping old channel", src);
                                        if let Some(guid) = write_handle.guid(&src) {
                                            let log_out_broadcasts = log_out_and_disconnect(
                                                Some(DisconnectReason::NewConnectionAttempt),
                                                guid,
                                                &[],
                                                previous_channel,
                                                &game_server,
                                                &socket,
                                                &server_options
                                            );
                                            write_handle.broadcast(client_enqueue.clone(), log_out_broadcasts);
                                        }
                                    }
                                },
                                Err(err) => {
                                    println!("Could not create channel because maximum of {} channels was reached", err.0);
                                    disconnect(Some(DisconnectReason::ConnectionRefused), &[recv_data], err.1.into(), &socket, &server_options);
                                },
                            }
                        } else {
                            break;
                        }
                    }
                }
            })
        })
        .collect()
}

fn spawn_process_threads(
    threads: u16,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: Sender<SocketAddr>,
    client_dequeue: Receiver<SocketAddr>,
    server_options: Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) -> Vec<JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let channel_manager = channel_manager.clone();
            let socket = socket.clone();
            let server_options = server_options.clone();
            let game_server = game_server.clone();
            let client_enqueue = client_enqueue.clone();
            let client_dequeue = client_dequeue.clone();

            thread::spawn(move || loop {
                // Don't lock the channel manager until we have packets to process
                // to avoid deadlock with channel creation
                let src = client_dequeue
                    .recv()
                    .expect("Tried to dequeue client after queue channel disconnected");

                let mut channel_manager_read_handle = channel_manager.read();
                let mut channel_handle = if let Some(channel_handle) =
                    lock_channel(&channel_manager_read_handle, &src)
                {
                    channel_handle
                } else {
                    return;
                };

                let packets_to_send = channel_manager_read_handle
                    .send_next(&mut channel_handle, server_options.send_packets_per_cycle);
                send_packets(&packets_to_send, &src, &socket);

                let packets_for_game_server = channel_manager_read_handle.process_next(
                    &mut channel_handle,
                    server_options.process_packets_per_cycle,
                    &server_options,
                );

                let mut broadcasts = Vec::new();
                for packet in packets_for_game_server {
                    if let Some(guid) = channel_manager_read_handle.guid(&src) {
                        match game_server.process_packet(guid, packet) {
                            Ok(mut new_broadcasts) => broadcasts.append(&mut new_broadcasts),
                            Err(err) => println!("Unable to process packet: {:?}", err),
                        }
                    } else {
                        match game_server.authenticate(packet) {
                            Ok(guid) => {
                                drop(channel_handle);
                                drop(channel_manager_read_handle);

                                let mut channel_manager_write_handle = channel_manager.write();
                                if let Some(existing_channel) = channel_manager_write_handle.authenticate(&src, guid) {
                                    println!("Client {} logged in as an already logged-in player {}, disconnecting existing client", src, guid);
                                    broadcasts.append(&mut log_out_and_disconnect(
                                        Some(DisconnectReason::NewConnectionAttempt),
                                        guid,
                                        &[],
                                        existing_channel,
                                        &game_server,
                                        &socket,
                                        &server_options,
                                    ));
                                }

                                match game_server.log_in(guid) {
                                    Ok(mut log_in_broadcasts) => broadcasts.append(&mut log_in_broadcasts),
                                    Err(err) => println!("Unable to log in player {} on client {}: {:?}", guid, src, err),
                                };
                                drop(channel_manager_write_handle);

                                channel_manager_read_handle = channel_manager.read();
                                channel_handle = if let Some(channel_handle) =
                                    lock_channel(&channel_manager_read_handle, &src)
                                {
                                    channel_handle
                                } else {
                                    return;
                                };
                            }
                            Err(err) => println!("Unable to process login packet: {:?}", err),
                        }
                    }
                }

                // Re-enqueue this address for another thread to pick up if there is still more processing to be done
                if channel_handle.needs_processing() {
                    client_enqueue
                        .send(src)
                        .expect("Tried to enqueue client after queue channel disconnected");
                }

                drop(channel_handle);
                channel_manager_read_handle.broadcast(client_enqueue.clone(), broadcasts);
            })
        })
        .collect()
}

fn spawn_cleanup_thread(
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    cleanup_tick_dequeue: Receiver<Instant>,
    client_enqueue: Sender<SocketAddr>,
    server_options: &Arc<ServerOptions>,
    game_server: &Arc<GameServer>,
) {
    let channel_inactive_timeout =
        Duration::from_millis(server_options.channel_inactive_timeout_millis);
    let channel_manager = channel_manager.clone();
    let socket = socket.clone();
    let client_enqueue = client_enqueue.clone();
    let server_options = server_options.clone();
    let game_server = game_server.clone();
    thread::spawn(move || loop {
        cleanup_tick_dequeue
            .recv()
            .expect("Cleanup tick channel disconnected");
        let mut channel_manager_handle = channel_manager.write();
        let channels_to_disconnect = channel_manager_handle.drain_filter(|channel| {
            if channel.elapsed_since_last_receive() > channel_inactive_timeout {
                let _ = channel.disconnect(DisconnectReason::Timeout);
            }
            !channel.connected()
        });

        let mut broadcasts = Vec::new();
        for (possible_guid, channel) in channels_to_disconnect {
            if let Some(guid) = possible_guid {
                broadcasts.append(&mut log_out_and_disconnect(
                    None,
                    guid,
                    &[],
                    channel,
                    &game_server,
                    &socket,
                    &server_options,
                ));
            }
        }

        channel_manager_handle.broadcast(client_enqueue.clone(), broadcasts);
    });
}

fn lock_channel<'a>(
    channel_manager_read_handle: &'a RwLockReadGuard<ChannelManager>,
    addr: &'a SocketAddr,
) -> Option<MutexGuard<'a, Channel>> {
    channel_manager_read_handle
        .get_by_addr(addr)
        .map(|channel| channel.lock())
}

fn send_packet(buffer: &[u8], addr: &SocketAddr, socket: &Arc<UdpSocket>) {
    if let Err(err) = socket.send_to(buffer, addr) {
        println!("Unable to send packet to client {}: {}", addr, err);
    }
}

fn send_packets(packets: &[Vec<u8>], addr: &SocketAddr, socket: &Arc<UdpSocket>) {
    packets
        .iter()
        .for_each(|packet| send_packet(packet, addr, socket))
}

fn disconnect(
    reason_override: Option<DisconnectReason>,
    packets_to_process_first: &[&[u8]],
    channel: Mutex<Channel>,
    socket: &Arc<UdpSocket>,
    server_options: &Arc<ServerOptions>,
) {
    // Allow processing some packets first so we can add the session ID to the disconnect packet
    let mut channel_handle = channel.lock();
    packets_to_process_first.iter().for_each(|packet| {
        if let Err(err) = channel_handle.receive(packet) {
            println!(
                "Couldn't deserialize packet while processing disconnect for client {}: {:?}",
                channel_handle.addr, err
            );
        }
    });
    channel_handle.process_all(server_options);

    let disconnect_reason = reason_override
        .or(channel_handle.disconnect_reason)
        .unwrap_or(DisconnectReason::Unknown);
    match channel_handle.disconnect(disconnect_reason) {
        Ok(disconnect_packets) => send_packets(&disconnect_packets, &channel_handle.addr, socket),
        Err(err) => println!(
            "Unable to serialize disconnect packet for client {}: {:?}",
            channel_handle.addr, err
        ),
    }
}

fn log_out_and_disconnect(
    reason_override: Option<DisconnectReason>,
    guid: u32,
    packets_to_process_first: &[&[u8]],
    channel: Mutex<Channel>,
    game_server: &Arc<GameServer>,
    socket: &Arc<UdpSocket>,
    server_options: &Arc<ServerOptions>,
) -> Vec<Broadcast> {
    disconnect(
        reason_override,
        packets_to_process_first,
        channel,
        socket,
        server_options,
    );
    match game_server.log_out(guid) {
        Ok(log_out_broadcasts) => log_out_broadcasts,
        Err(err) => {
            println!("Unable to log out existing player {}: {:?}", guid, err);
            Vec::new()
        }
    }
}
