use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Cursor, Error};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, RwLock};
use byteorder::{BigEndian, ReadBytesExt};
use crate::deserialize::{deserialize_packet, DeserializeError};
use crate::hash::{CrcSeed, CrcSize};

#[derive(Debug)]
pub enum ProtocolOpCode {
    SessionRequest   = 0x01,
    SessionReply     = 0x02,
    MultiPacket      = 0x03,
    Disconnect       = 0x05,
    Heartbeat        = 0x06,
    NetStatusRequest = 0x07,
    NetStatusReply   = 0x08,
    Data             = 0x09,
    DataFragment     = 0x0D,
    Ack              = 0x11,
    AckAll           = 0x15,
    UnknownSender    = 0x1D,
    RemapConnection  = 0x1E
}

pub type SequenceNumber = u16;
pub type SoeProtocolVersion = u32;
pub type SessionId = u32;
pub type BufferSize = u32;
pub type ApplicationProtocol = String;

pub enum DisconnectReason {
    Unknown = 0,
    IcmpError = 1,
    Timeout = 2,
    OtherSideTerminated = 3,
    ManagerDeleted = 4,
    ConnectFail = 5,
    Application = 6,
    UnreachableConnection = 7,
    UnacknowledgedTimeout = 8,
    NewConnectionAttempt = 9,
    ConnectionRefused = 10,
    ConnectError = 11,
    ConnectingToSelf = 12,
    ReliableOverflow = 13,
    ApplicationReleased = 14,
    CorruptPacket = 15,
    ProtocolMismatch = 16
}

pub type ClientTick = u16;
pub type ServerTick = u32;
pub type Timestamp = u32;
pub type PacketCount = u64;

pub enum Packet {
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
    Ack(SequenceNumber),
    AckAll(SequenceNumber),
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
            Packet::Ack(..) => ProtocolOpCode::Ack,
            Packet::AckAll(..) => ProtocolOpCode::AckAll,
            Packet::UnknownSender => ProtocolOpCode::UnknownSender,
            Packet::RemapConnection(..) => ProtocolOpCode::RemapConnection
        }
    }
}

struct PendingPacket {
    needs_ack: bool,
    packet: Packet
}

impl PendingPacket {
    fn new(packet: Packet) -> Self {
        PendingPacket {
            needs_ack: packet.sequence_number().is_some(),
            packet
        }
    }
}

#[non_exhaustive]
#[derive(Debug)]
enum FragmentError {
    ExpectedFragment(ProtocolOpCode),
    MissingDataLength,
    IoError(Error)
}

impl From<Error> for FragmentError {
    fn from(value: Error) -> Self {
        FragmentError::IoError(value)
    }
}

struct FragmentState {
    buffer: Vec<u8>,
    remaining_bytes: u32
}

impl FragmentState {
    fn add(&mut self, packet: Packet) -> Result<Option<Packet>, FragmentError> {
        if let Packet::DataFragment(sequence_number, data) = packet {
            let packet_data;
            if self.remaining_bytes == 0 {
                if data.len() < 8 {
                    return Err(FragmentError::MissingDataLength);
                }

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
            return Err(FragmentError::ExpectedFragment(packet.op_code()));
        }

        Ok(Some(packet))
    }
}

struct Channel {
    socket: UdpSocket,
    buffer_size: usize,
    crc_length: CrcSize,
    crc_seed: CrcSeed,
    allow_compression: bool,
    allow_encryption: bool,
    recency_limit: SequenceNumber,
    fragment_state: FragmentState,
    send_queue: VecDeque<PendingPacket>,
    receive_queue: VecDeque<Packet>,
    reordered_packets: BTreeMap<SequenceNumber, Packet>,
    next_client_sequence: SequenceNumber,
    next_server_sequence: SequenceNumber,
    last_client_ack: SequenceNumber,
    last_server_ack: SequenceNumber
}

impl Channel {

    pub fn receive(&mut self, data: &[u8]) -> Result<u32, DeserializeError> {
        let mut packets = deserialize_packet(
            data,
            self.allow_compression,
            self.crc_length,
            self.crc_seed
        )?;

        let packet_count = packets.len() as u32;
        packets.drain(..).for_each(|packet| self.receive_queue.push_back(packet));
        Ok(packet_count)
    }

    pub fn process_next(&mut self, count: u8) {
        let mut needs_new_ack = false;

        for _ in 0..count {
            if let Some(packet) = self.receive_queue.pop_front() {

                // Special processing for reliable packets
                if let Some(sequence_number) = packet.sequence_number() {

                    // Add out-of-order packets to a separate queue until the expected
                    // packets arrive.
                    if sequence_number != self.next_client_sequence {
                        if self.save_for_reorder(sequence_number) {
                            self.reordered_packets.insert(sequence_number, packet);
                            self.acknowledge_one(sequence_number);
                        }

                        // Assume the packet was already acked. In the worst case, the
                        // client just has to resend the packet again.
                        continue;

                    }

                    self.last_server_ack = sequence_number;
                    self.next_client_sequence = self.next_client_sequence.wrapping_add(1);
                    needs_new_ack = true;

                    if let Some((&next_reorder_sequence, _)) = self.reordered_packets.first_key_value() {
                        if next_reorder_sequence == self.next_client_sequence {
                            let (_, next_packet) = self.reordered_packets.pop_first().unwrap();
                            self.receive_queue.push_front(next_packet);
                        }
                    }
                }

                match self.fragment_state.add(packet) {
                    Ok(possible_packet) => if let Some(packet) = possible_packet {
                        self.process_packet(packet)
                    },
                    Err(err) => println!("Unable to process packet: {:?}", err)
                }
            } else {
                break;
            }
        }

        if needs_new_ack {
            self.acknowledge_all(self.last_server_ack);
        }
    }

    pub fn send_next(&mut self, count: u8) {

    }

    fn next_server_sequence(&mut self) -> SequenceNumber {
        let next_sequence = self.next_server_sequence;
        self.next_server_sequence = self.next_server_sequence.wrapping_add(1);
        next_sequence
    }

    fn save_for_reorder(&self, sequence_number: SequenceNumber) -> bool {
        let max_sequence_number = self.next_client_sequence.wrapping_add(self.recency_limit);

        // If the max is smaller, the sequence numbers wrapped around
        if max_sequence_number > self.next_client_sequence {
            sequence_number <= max_sequence_number
        } else {
            sequence_number > self.next_client_sequence || sequence_number < max_sequence_number
        }

    }

    fn should_client_ack(recency_limit: SequenceNumber, next_server_sequence: SequenceNumber,
                         max: SequenceNumber, pending: SequenceNumber) -> bool {
        let min_sequence_number = next_server_sequence.wrapping_sub(recency_limit);

        // If the max is smaller, the sequence numbers wrapped around
        if min_sequence_number < max {
            min_sequence_number <= pending
        } else {
            pending < max || pending > min_sequence_number
        }
    }

    fn process_packet(&mut self, packet: Packet) {
        match packet {
            Packet::Ack(acked_sequence) => self.process_ack(acked_sequence),
            Packet::AckAll(acked_sequence) => self.process_ack_all(acked_sequence),
            _ => println!("Unimplemented: {:?}", packet.op_code())
        }
    }

    fn process_ack(&mut self, acked_sequence: SequenceNumber) {
        if Channel::should_client_ack(self.recency_limit, self.next_server_sequence,
                                      self.next_server_sequence, acked_sequence) {
            for pending_packet in self.send_queue.iter_mut() {
                if let Some(pending_sequence) = pending_packet.packet.sequence_number() {
                    if acked_sequence == pending_sequence {
                        pending_packet.needs_ack = false;
                    }
                }
            }
        }
    }

    fn process_ack_all(&mut self, acked_sequence: SequenceNumber) {
        for pending_packet in self.send_queue.iter_mut() {
            if let Some(pending_sequence) = pending_packet.packet.sequence_number() {
                if Channel::should_client_ack(self.recency_limit, self.next_server_sequence,
                                              acked_sequence, pending_sequence) {
                    pending_packet.needs_ack = false;
                }
            }
        }
    }

    fn acknowledge_one(&mut self, sequence_number: SequenceNumber) {
        self.send_queue.push_back(PendingPacket::new(Packet::Ack(sequence_number)));
    }

    fn acknowledge_all(&mut self, sequence_number: SequenceNumber) {
        self.send_queue.push_back(PendingPacket::new(Packet::AckAll(sequence_number)));
    }
}

type ChannelManager = RwLock<HashMap<SocketAddr, Mutex<Channel>>>;
