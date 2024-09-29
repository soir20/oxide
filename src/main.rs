use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::{MutexGuard, RwLock, RwLockReadGuard};
use protocol::BufferSize;
use serde::Deserialize;
use std::fs::File;
use std::io::Error;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
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
    let server_options = load_server_options(config_dir).expect("Unable to read server options");
    spawn(http::start(
        4000,
        config_dir,
        Path::new("config/custom_assets"),
        PathBuf::from(".asset_cache"),
    ));
    println!("Hello, world!");
    let socket = UdpSocket::bind(SocketAddr::new(
        "127.0.0.1".parse().unwrap(),
        "20225".parse().unwrap(),
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
        server_options.clone(),
    );
    threads.append(&mut spawn_process_threads(
        server_options.process_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue,
        client_dequeue,
        server_options.process_packets_per_cycle,
        server_options.send_packets_per_cycle,
        &game_server_arc,
    ));

    for thread in threads {
        thread.join().expect("Thread exited with error");
    }
}

const MAX_BUFFER_SIZE: BufferSize = 512;

#[derive(Clone, Deserialize)]
struct ServerOptions {
    pub receive_threads: u16,
    pub process_threads: u16,
    pub max_sessions: usize,
    pub process_packets_per_cycle: u8,
    pub send_packets_per_cycle: u8,
    pub packet_recency_limit: u16,
    pub default_millis_until_resend: u128,
    pub max_round_trip_entries: usize,
    pub desired_resend_pct: u8,
    pub max_millis_until_resend: u128,
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
    server_options: ServerOptions,
) -> Vec<JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let channel_manager = Arc::clone(channel_manager);
            let socket = Arc::clone(socket);
            let client_enqueue = client_enqueue.clone();

            thread::spawn(move || loop {
                let mut buf = [0; MAX_BUFFER_SIZE as usize];
                if let Ok((len, src)) = socket.recv_from(&mut buf) {
                    let recv_data = &buf[0..len];

                    let mut read_handle = channel_manager.read();

                    let receive_result =
                        read_handle.receive(client_enqueue.clone(), &src, recv_data);
                    if receive_result == ReceiveResult::CreateChannelFirst {
                        println!("Creating channel for {}", src);
                        drop(read_handle);
                        let previous_channel_result = channel_manager.write().insert(
                            &src,
                            Channel::new(
                                src,
                                initial_buffer_size,
                                server_options.packet_recency_limit,
                                server_options.default_millis_until_resend,
                                server_options.max_round_trip_entries,
                                server_options.desired_resend_pct,
                                server_options.max_millis_until_resend,
                            ),
                        );

                        if let Ok(previous_channel) = previous_channel_result {
                            read_handle = channel_manager.read();

                            if previous_channel.is_some() {
                                println!("Client {} reconnected, dropping old channel", src);
                            }

                            read_handle.receive(client_enqueue.clone(), &src, recv_data);
                        } else if let Err(max_channels) = previous_channel_result {
                            println!("Could not create channel because maximum of {} channels was reached", max_channels.0);
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
    process_delta: u8,
    send_delta: u8,
    game_server: &Arc<GameServer>,
) -> Vec<JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let channel_manager = Arc::clone(channel_manager);
            let socket = Arc::clone(socket);
            let game_server = Arc::clone(game_server);
            let client_enqueue = client_enqueue.clone();
            let client_dequeue = client_dequeue.clone();

            thread::spawn(move || loop {
                // Don't lock the channel manager until we have packets to process
                // to avoid deadlock with channel creation
                let src = client_dequeue
                    .recv()
                    .expect("Tried to dequeue client after queue channel disconnected");

                let mut channel_manager_read_handle = channel_manager.read();
                let mut channel_handle = lock_channel(&channel_manager_read_handle, &src);

                let packets_to_send =
                    channel_manager_read_handle.send_next(&mut channel_handle, send_delta);
                for buffer in packets_to_send {
                    socket
                        .send_to(&buffer, src)
                        .expect("Unable to send packet to client");
                }

                let packets_for_game_server =
                    channel_manager_read_handle.process_next(&mut channel_handle, process_delta);

                let mut broadcasts = Vec::new();
                for packet in packets_for_game_server {
                    if let Some(guid) = channel_manager_read_handle.guid(&src) {
                        match game_server.process_packet(guid, packet) {
                            Ok(mut new_broadcasts) => broadcasts.append(&mut new_broadcasts),
                            Err(err) => println!("Unable to process packet: {:?}", err),
                        }
                    } else {
                        match game_server.login(packet) {
                            Ok((guid, mut new_broadcasts)) => {
                                drop(channel_handle);
                                drop(channel_manager_read_handle);
                                channel_manager.write().authenticate(&src, guid);
                                broadcasts.append(&mut new_broadcasts);
                                channel_manager_read_handle = channel_manager.read();
                                channel_handle = lock_channel(&channel_manager_read_handle, &src);
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

fn lock_channel<'a>(
    channel_manager_read_handle: &'a RwLockReadGuard<ChannelManager>,
    addr: &'a SocketAddr,
) -> MutexGuard<'a, Channel> {
    channel_manager_read_handle
        .get_by_addr(addr)
        .expect("Tried to process data on non-existent channel")
        .lock()
}
