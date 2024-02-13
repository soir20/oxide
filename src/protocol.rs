use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, RwLock};

type SequenceNumber = u16;

enum ProtocolOpCode {
    SessionRequest   = 0x01,
    SessionReply     = 0x02,
    MultiPacket      = 0x03,
    Disconnect       = 0x05,
    Ping             = 0x06,
    NetStatusRequest = 0x07,
    NetStatusReply   = 0x08,
    Data             = 0x09,
    DataFragment     = 0x0D,
    OutOfOrder       = 0x11,
    Ack              = 0x15,
    FatalError       = 0x1D,
    FatalErrorReply  = 0x1E
}

enum Packet {
    SessionRequest,
    SessionReply,
    MultiPacket,
    Disconnect,
    Ping,
    NetStatusRequest,
    NetStatusReply,
    Data(SequenceNumber),
    DataFragment(SequenceNumber),
    OutOfOrder(SequenceNumber),
    Ack(SequenceNumber),
    FatalError,
    FatalErrorReply
}

impl Packet {
    pub fn sequence_number(&self) -> Option<SequenceNumber> {
        match self {
            Packet::Data(n) => Some(*n),
            Packet::DataFragment(n) => Some(*n),
            _ => None
        }
    }

    pub fn op_code(&self) -> ProtocolOpCode {
        match self {
            Packet::SessionRequest => ProtocolOpCode::SessionRequest,
            Packet::SessionReply => ProtocolOpCode::SessionReply,
            Packet::MultiPacket => ProtocolOpCode::MultiPacket,
            Packet::Disconnect => ProtocolOpCode::Disconnect,
            Packet::Ping => ProtocolOpCode::Ping,
            Packet::NetStatusRequest => ProtocolOpCode::NetStatusRequest,
            Packet::NetStatusReply => ProtocolOpCode::NetStatusReply,
            Packet::Data(_) => ProtocolOpCode::Data,
            Packet::DataFragment(_) => ProtocolOpCode::DataFragment,
            Packet::OutOfOrder(_) => ProtocolOpCode::OutOfOrder,
            Packet::Ack(_) => ProtocolOpCode::Ack,
            Packet::FatalError => ProtocolOpCode::FatalError,
            Packet::FatalErrorReply => ProtocolOpCode::FatalErrorReply
        }
    }
}

struct PendingPacket {
    acked: bool,
    packet: Packet
}

impl PendingPacket {
    fn new(packet: Packet) -> Self {
        PendingPacket {
            acked: false,
            packet
        }
    }
}

struct Channel {
    socket: UdpSocket,
    buffer_size: usize,
    send_queue: VecDeque<PendingPacket>,
    receive_queue: VecDeque<Packet>,
    next_client_sequence: SequenceNumber,
    next_server_sequence: SequenceNumber,
    last_client_ack: SequenceNumber,
    last_server_ack: SequenceNumber
}

impl Channel {
    pub fn receive_next(&mut self, count: u8) {
        let mut needs_new_ack = false;

        for _ in 0..count {
            if let Some(packet) = self.receive_queue.pop_front() {

                // Special processing for reliable packets
                if let Some(sequence_number) = packet.sequence_number() {

                    // Request client resend out-of-order packets
                    if sequence_number > self.next_client_sequence {
                        self.reply_out_of_order(sequence_number);
                        continue;
                    }

                    // Ignore already-processed packets
                    if sequence_number < self.next_client_sequence {
                        continue;
                    }

                    self.last_server_ack = sequence_number;
                    self.next_client_sequence = self.next_client_sequence.wrapping_add(1);
                    needs_new_ack = true;
                }

                if let Packet::Ack(acked_sequence_number) = packet {

                    // Since the server always adds packets to the sending queue in order,
                    // we can assume all packets up until the matching sequence number
                    // are acked. This avoids edge cases where the sequence number wraps around.
                    for pending_packet in self.send_queue.iter_mut() {
                        pending_packet.acked = true;

                        if let Some(sequence_number) = pending_packet.packet.sequence_number() {
                            if sequence_number == acked_sequence_number {
                                break;
                            }
                        }
                    }

                }

                // TODO: let server handle packet data
            } else {
                break;
            }
        }

        if needs_new_ack {
            self.acknowledge(self.last_server_ack);
        }
    }

    pub fn send_next(&mut self, count: u8) {

    }

    fn next_server_sequence(&mut self) -> SequenceNumber {
        let next_sequence = self.next_server_sequence;
        self.next_server_sequence = self.next_server_sequence.wrapping_add(1);
        next_sequence
    }

    fn reply_out_of_order(&mut self, sequence_number: SequenceNumber) {
        self.send_queue.push_back(PendingPacket::new(Packet::OutOfOrder(sequence_number)));
    }

    fn acknowledge(&mut self, sequence_number: SequenceNumber) {
        self.send_queue.push_back(PendingPacket::new(Packet::Ack(sequence_number)));
    }
}

type ChannelManager = RwLock<HashMap<SocketAddr, Mutex<Channel>>>;
