use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::Duration;
use crate::protocol::Channel;

mod protocol;
mod hash;
mod deserialize;
mod serialize;

fn main() {
    println!("Hello, world!");
    let socket = UdpSocket::bind(SocketAddr::new("127.0.0.1".parse().unwrap(), "20225".parse().unwrap())).expect("couldn't bind to socket");

    let mut channel = Channel::new(200, 1000);
    let delta = 5u8;
    loop {
        let mut buf = [0; 512];
        if let Ok((len, src)) = socket.recv_from(&mut buf) {
            println!("Bytes received: {}", len);
            println!("Bytes: {:x?}", buf);
            let receive_result = channel.receive(&buf);
            if let Err(ref err) = receive_result {
                println!("Receive error: {:?}", err);
            }

            let received_packets = receive_result.unwrap_or(0);
            println!("Packets received: {}", received_packets);

            println!("Processing at most {} packets", delta);
            channel.process_next(delta);

            let send_result = channel.send_next(delta);
            if let Err(ref err) = send_result {
                println!("Send error: {:?}", err);
            }
            let packets_to_send = send_result.unwrap_or(Vec::new());
            println!("Sending {} packets", packets_to_send.len());
            for buffer in packets_to_send {
                println!("Sending: {:x?}", buffer);
                socket.send_to(&buffer, &src).expect("Unable to send packet to client");
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}
