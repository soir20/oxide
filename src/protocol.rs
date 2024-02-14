use std::collections::{HashMap, VecDeque};
use std::io::{Cursor, Error};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, RwLock};
use byteorder::{BigEndian, ReadBytesExt};

type SequenceNumber = u16;
type Crc32 = u32;

enum ProtocolOpCode {
    SessionRequest   = 0x01,
    SessionReply     = 0x02,
    MultiPacket      = 0x03,
    Disconnect       = 0x05,
    Heartbeat        = 0x06,
    NetStatusRequest = 0x07,
    NetStatusReply   = 0x08,
    Data             = 0x09,
    DataFragment     = 0x0D,
    OutOfOrder       = 0x11,
    Ack              = 0x15,
    UnknownSender    = 0x1D,
    RemapConnection  = 0x1E
}

type SoeProtocolVersion = u32;
type SessionId = u32;
type BufferSize = u32;
type ApplicationProtocol = String;
type CrcSeed = u32;
type CrcSize = u8;

enum DisconnectReason {

}

type ClientTick = u16;
type ServerTick = u32;
type Timestamp = u32;
type PacketCount = u64;

enum Packet {
    SessionRequest(SoeProtocolVersion, SessionId, BufferSize, ApplicationProtocol),
    SessionReply(SessionId, CrcSeed, CrcSize, bool, bool, BufferSize, SoeProtocolVersion),
    Disconnect(SessionId, DisconnectReason),
    Heartbeat,
    NetStatusRequest(ClientTick, Timestamp, Timestamp, Timestamp, Timestamp,
                     Timestamp, PacketCount, PacketCount, u16),
    NetStatusReply(ClientTick, ServerTick, PacketCount, PacketCount,
                   PacketCount, PacketCount, u16),
    Data(SequenceNumber, Vec<u8>),
    DataFragment(SequenceNumber, Vec<u8>),
    OutOfOrder(SequenceNumber),
    Ack(SequenceNumber),
    UnknownSender,
    RemapConnection(SessionId, CrcSeed)
}

impl Packet {
    pub fn sequence_number(&self) -> Option<SequenceNumber> {
        match self {
            Packet::Data(n, _) => Some(*n),
            Packet::DataFragment(n, _) => Some(*n),
            _ => None
        }
    }

    pub fn op_code(&self) -> ProtocolOpCode {
        match self {
            Packet::SessionRequest(..) => ProtocolOpCode::SessionRequest,
            Packet::SessionReply(..) => ProtocolOpCode::SessionReply,
            Packet::Disconnect(..) => ProtocolOpCode::Disconnect,
            Packet::Heartbeat => ProtocolOpCode::Heartbeat,
            Packet::NetStatusRequest(..) => ProtocolOpCode::NetStatusRequest,
            Packet::NetStatusReply(..) => ProtocolOpCode::NetStatusReply,
            Packet::Data(..) => ProtocolOpCode::Data,
            Packet::DataFragment(..) => ProtocolOpCode::DataFragment,
            Packet::OutOfOrder(..) => ProtocolOpCode::OutOfOrder,
            Packet::Ack(..) => ProtocolOpCode::Ack,
            Packet::UnknownSender => ProtocolOpCode::UnknownSender,
            Packet::RemapConnection(..) => ProtocolOpCode::RemapConnection
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

#[non_exhaustive]
enum FragmentErr {
    ExpectedFragment(ProtocolOpCode),
    IoError(Error)
}

impl From<Error> for FragmentErr {
    fn from(value: Error) -> Self {
        FragmentErr::IoError(value)
    }
}

struct FragmentState {
    buffer: Vec<u8>,
    remaining_bytes: u32
}

impl FragmentState {
    fn add(&mut self, packet: Packet) -> Result<Option<Packet>, FragmentErr> {
        if let Packet::DataFragment(sequence_number, mut data) = packet {
            let packet_data;
            if self.remaining_bytes == 0 {
                packet_data = &data[4..];
                self.remaining_bytes = Cursor::new(&data).read_u32::<BigEndian>()?;
            } else {
                packet_data = &data;
            }

            self.remaining_bytes = self.remaining_bytes.checked_sub(packet_data.len() as u32)
                .unwrap_or(0);
            self.buffer.extend(packet_data);

            if self.remaining_bytes > 0 {
                return Ok(None);
            }

            let old_buffer = self.buffer.clone();
            self.buffer.clear();
            return Ok(Some(Packet::Data(sequence_number, old_buffer)))
        }

        if self.remaining_bytes > 0 {
            return Err(FragmentErr::ExpectedFragment(packet.op_code()));
        }

        Ok(Some(packet))
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

                    // Request client resend out-of-order packets.
                    // There is an edge case where sequence numbers near
                    // u16::MAX may have extraneous out-of-order packets
                    // sent. For example, if these sequence numbers are received:
                    // 65534, 65535, 0, 65534, 65535, 0
                    // This case could happen if the first three packets are not
                    // acked in time and resent by the client. In this case,
                    // out-of-order packets will be sent for packets 65534 and
                    // 65535. However, the server will still ack the packets. The
                    // client should be able to handle this case because there is
                    // no guarantee that correct out-of-order packets will arrive
                    // before an ack.
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
