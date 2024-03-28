use std::net::{SocketAddr, UdpSocket};
use std::path::Path;
use std::thread;
use std::time::Duration;
use crate::game_server::GameServer;
use crate::protocol::Channel;

mod protocol;
mod game_server;

fn main() {
    println!("Hello, world!");
    let socket = UdpSocket::bind(SocketAddr::new("127.0.0.1".parse().unwrap(), "20225".parse().unwrap())).expect("couldn't bind to socket");

    let mut channel = Channel::new(200, 1000);
    let mut game_server = GameServer::new(Path::new("config")).unwrap();
    let delta = 5u8;
    loop {
        let mut buf = [0; 512];
        if let Ok((len, src)) = socket.recv_from(&mut buf) {
            println!("Bytes received: {}", len);
            let recv_data = &buf[0..len];
            println!("Bytes: {:x?}", recv_data);
            let receive_result = channel.receive(&recv_data);
            if let Err(ref err) = receive_result {
                println!("Receive error: {:?}", err);
            }

            let received_packets = receive_result.unwrap_or(0);
            println!("Packets received: {}", received_packets);

            println!("Processing at most {} packets", delta);
            let packets_for_game_server = channel.process_next(delta);
            packets_for_game_server.into_iter()
                .flat_map(|packet| game_server.process_packet(packet).unwrap().into_iter())
                .for_each(|packet| channel.send_data(packet));

            let send_result = channel.send_next(delta);
            if let Err(ref err) = send_result {
                println!("Send error: {:?}", err);
            }
            let packets_to_send = send_result.unwrap_or(Vec::new());
            println!("Sending {} packets", packets_to_send.len());
            for buffer in packets_to_send {
                println!("Sending {} bytes: {:x?}", buffer.len(), buffer);
                socket.send_to(&buffer, &src).expect("Unable to send packet to client");
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}
