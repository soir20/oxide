use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::RwLock;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
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

    let channel_manager = RwLock::new(ChannelManager::new());

    let game_server = GameServer::new(config_dir).unwrap();
    let process_delta = 40u8;
    let send_delta = 20u8;
    let server_options = ServerOptions {
        receive_threads: 1,
        process_threads: 4,
        max_sessions: 100,
    };

    let channel_manager_arc = Arc::new(channel_manager);
    let socket_arc = Arc::new(socket);
    let game_server_arc = Arc::new(game_server);
    let (client_enqueue, client_dequeue) = bounded(server_options.max_sessions);
    spawn_receive_threads(
        server_options.receive_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue.clone(),
        200,
        1000,
        5,
    );
    spawn_process_threads(
        server_options.process_threads,
        &channel_manager_arc,
        &socket_arc,
        client_enqueue,
        client_dequeue,
        process_delta,
        send_delta,
        &game_server_arc,
    );
}

struct ServerOptions {
    pub receive_threads: u16,
    pub process_threads: u16,
    pub max_sessions: usize,
}

fn spawn_receive_threads(
    threads: u16,
    channel_manager: &Arc<RwLock<ChannelManager>>,
    socket: &Arc<UdpSocket>,
    client_enqueue: Sender<SocketAddr>,
    initial_buffer_size: u32,
    recency_limit: u16,
    millis_until_resend: u128,
) {
    for _ in 0..threads {
        let channel_manager = Arc::clone(channel_manager);
        let socket = Arc::clone(socket);
        let client_enqueue = client_enqueue.clone();

        thread::spawn(move || loop {
            let mut buf = [0; 512];
            if let Ok((len, src)) = socket.recv_from(&mut buf) {
                let recv_data = &buf[0..len];

                let mut read_handle = channel_manager.read();

                let receive_result = read_handle.receive(client_enqueue.clone(), &src, recv_data);
                if receive_result == ReceiveResult::CreateChannelFirst {
                    println!("Creating channel for {}", src);
                    drop(read_handle);
                    let previous_channel = channel_manager.write().insert(
                        &src,
                        Channel::new(initial_buffer_size, recency_limit, millis_until_resend),
                    );
                    read_handle = channel_manager.read();

                    if previous_channel.is_some() {
                        println!("Client {} reconnected, dropping old channel", src);
                    }

                    read_handle.receive(client_enqueue.clone(), &src, recv_data);
                }
            }
        });
    }
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
) {
    for _ in 0..threads {
        let channel_manager = Arc::clone(channel_manager);
        let socket = Arc::clone(socket);
        let game_server = Arc::clone(game_server);
        let client_enqueue = client_enqueue.clone();
        let client_dequeue = client_dequeue.clone();

        thread::spawn(move || loop {
            let mut read_handle = channel_manager.read();

            let (src, packets_for_game_server) = read_handle.process_next(
                client_enqueue.clone(),
                client_dequeue.clone(),
                process_delta,
            );

            let mut broadcasts = Vec::new();
            for packet in packets_for_game_server {
                if let Some(guid) = read_handle.guid(&src) {
                    match game_server.process_packet(guid, packet) {
                        Ok(mut new_broadcasts) => broadcasts.append(&mut new_broadcasts),
                        Err(err) => println!("Unable to process packet: {:?}", err),
                    }
                } else {
                    match game_server.login(packet) {
                        Ok((guid, mut new_broadcasts)) => {
                            drop(read_handle);
                            channel_manager.write().authenticate(&src, guid);
                            broadcasts.append(&mut new_broadcasts);
                            read_handle = channel_manager.read();
                        }
                        Err(err) => println!("Unable to process login packet: {:?}", err),
                    }
                }
            }

            read_handle.broadcast(broadcasts);

            let packets_to_send = read_handle.send_next(&src, send_delta);
            for buffer in packets_to_send {
                socket
                    .send_to(&buffer, src)
                    .expect("Unable to send packet to client");
            }
        });
    }
}
