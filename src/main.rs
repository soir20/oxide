use parking_lot::RwLock;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tokio::spawn;

use crate::channel_manager::{ChannelManager, ReceiveResult};
use crate::game_server::GameServer;
use crate::protocol::Channel;

mod protocol;
mod game_server;
mod channel_manager;
mod http;

#[tokio::main]
async fn main() {
    let config_dir = Path::new("config");
    spawn(
        http::start(4000, config_dir, Path::new("config/custom_assets"), PathBuf::from(".asset_cache"))
    );
    println!("Hello, world!");
    let socket = UdpSocket::bind(SocketAddr::new("127.0.0.1".parse().unwrap(), "20225".parse().unwrap())).expect("couldn't bind to socket");

    let channel_manager = RwLock::new(ChannelManager::new());

    let game_server = GameServer::new(config_dir).unwrap();
    let process_delta = 40u8;
    let send_delta = 20u8;
    loop {
        let mut buf = [0; 512];
        if let Ok((len, src)) = socket.recv_from(&mut buf) {
            //println!("Bytes received: {}", len);
            let recv_data = &buf[0..len];
            //println!("Bytes: {:x?}", recv_data);

            let mut read_handle = channel_manager.read();

            let receive_result = read_handle.receive(&src, recv_data);
            if receive_result == ReceiveResult::CreateChannelFirst {
                println!("Creating channel for {}", src);
                drop(read_handle);
                let previous_channel = channel_manager.write()
                    .insert(&src, Channel::new(200, 1000, 5));
                read_handle = channel_manager.read();

                if previous_channel.is_some() {
                    println!("Client {} reconnected, dropping old channel", src);
                }

                read_handle.receive(&src, recv_data);
            }

            //println!("Processing at most {} packets", process_delta);
            let packets_for_game_server = read_handle.process_next(&src, process_delta);
            let mut broadcasts = Vec::new();
            for packet in packets_for_game_server {
                if let Some(guid) = read_handle.guid(&src) {
                    match game_server.process_packet(guid, packet) {
                        Ok(mut new_broadcasts) => broadcasts.append(&mut new_broadcasts),
                        Err(err) => println!("Unable to process packet: {:?}", err)
                    }
                } else {
                    match game_server.login(packet) {
                        Ok((guid, mut new_broadcasts)) => {
                            drop(read_handle);
                            channel_manager.write().authenticate(&src, guid);
                            broadcasts.append(&mut new_broadcasts);
                            read_handle = channel_manager.read();
                        },
                        Err(err) => println!("Unable to process login packet: {:?}", err)
                    }
                }
            }

            read_handle.broadcast(broadcasts);

            let packets_to_send = read_handle.send_next(&src, send_delta);
            //println!("Sending {} packets", packets_to_send.len());
            for buffer in packets_to_send {
                //println!("Sending {} bytes: {:x?}", buffer.len(), buffer);
                socket.send_to(&buffer, src).expect("Unable to send packet to client");
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}
