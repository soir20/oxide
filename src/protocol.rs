use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Cursor, Error, Write};
use std::mem::size_of;
use std::net::SocketAddr;
use std::sync::{Mutex, RwLock};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use rand::random;
use crate::deserialize::{deserialize_packet, DeserializeError};
use crate::hash::{CrcSeed, CrcSize};
use crate::login::{make_tunneled_packet, send_item_definitions, send_self_to_client};
use crate::serialize::{max_fragment_data_size, serialize_packets, SerializeError};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

impl ProtocolOpCode {
    pub fn requires_session(&self) -> bool {
        match self {
            ProtocolOpCode::SessionRequest => false,
            ProtocolOpCode::SessionReply => false,
            ProtocolOpCode::MultiPacket => true,
            ProtocolOpCode::Disconnect => true,
            ProtocolOpCode::Heartbeat => true,
            ProtocolOpCode::NetStatusRequest => false,
            ProtocolOpCode::NetStatusReply => false,
            ProtocolOpCode::Data => true,
            ProtocolOpCode::DataFragment => true,
            ProtocolOpCode::Ack => true,
            ProtocolOpCode::AckAll => true,
            ProtocolOpCode::UnknownSender => false,
            ProtocolOpCode::RemapConnection => false
        }
    }
}

pub type SequenceNumber = u16;
pub type SoeProtocolVersion = u32;
pub type SessionId = u32;
pub type BufferSize = u32;
pub type ApplicationProtocol = String;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DisconnectReason {
    Unknown               = 0,
    IcmpError             = 1,
    Timeout               = 2,
    OtherSideTerminated   = 3,
    ManagerDeleted        = 4,
    ConnectFail           = 5,
    Application           = 6,
    UnreachableConnection = 7,
    UnacknowledgedTimeout = 8,
    NewConnectionAttempt  = 9,
    ConnectionRefused     = 10,
    ConnectError          = 11,
    ConnectingToSelf      = 12,
    ReliableOverflow      = 13,
    ApplicationReleased   = 14,
    CorruptPacket         = 15,
    ProtocolMismatch      = 16
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

pub struct Session {
    pub session_id: SessionId,
    pub crc_length: CrcSize,
    pub crc_seed: CrcSeed,
    pub allow_compression: bool,
    pub use_encryption: bool
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

pub struct Channel {
    session: Option<Session>,
    buffer_size: BufferSize,
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

    pub fn new(initial_buffer_size: BufferSize, recency_limit: SequenceNumber) -> Self {
        Channel {
            session: None,
            buffer_size: initial_buffer_size,
            recency_limit,
            fragment_state: FragmentState { buffer: Vec::new(), remaining_bytes: 0 },
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
            reordered_packets: BTreeMap::new(),
            next_client_sequence: 0,
            next_server_sequence: 0,
            last_client_ack: 0,
            last_server_ack: 0
        }
    }

    pub fn receive(&mut self, data: &[u8]) -> Result<u32, DeserializeError> {
        let mut packets = deserialize_packet(data, &self.session)?;

        let packet_count = packets.len() as u32;
        packets.drain(..).for_each(|packet| self.receive_queue.push_back(packet));
        Ok(packet_count)
    }

    pub fn process_next(&mut self, count: u8) {
        let mut needs_new_ack = false;
        let mut packet_to_process = None;

        for _ in 0..count {
            if let Some(packet) = self.receive_queue.pop_front() {

                // Special processing for reliable packets
                if let Some(sequence_number) = packet.sequence_number() {

                    // Add out-of-order packets to a separate queue until the expected
                    // packets arrive.
                    if sequence_number != self.next_client_sequence {
                        if self.save_for_reorder(sequence_number) {
                            self.reordered_packets.insert(sequence_number, packet);
                        }

                        // Ack single packet in case the client didn't receive the ack
                        self.acknowledge_one(sequence_number);

                        continue;
                    }

                    self.last_server_ack = sequence_number;
                    self.next_client_sequence = self.next_client_sequence.wrapping_add(1);
                    needs_new_ack = true;

                    // Add a previously-received data packet if it is next in sequence
                    if let Some(next_packet) = self.reordered_packets.remove(&self.next_client_sequence) {
                        self.receive_queue.push_front(next_packet);
                    }

                }

                match self.fragment_state.add(packet) {
                    Ok(possible_packet) => if let Some(packet) = possible_packet {
                        packet_to_process = Some(packet);
                    } else {
                        packet_to_process = None;
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

        if let Some(packet) = packet_to_process {
            self.process_packet(packet);
        }
    }

    pub fn send_next(&mut self, count: u8) -> Result<Vec<Vec<u8>>, SerializeError> {
        let mut packets_to_send = Vec::new();

        // If the packet was acked, it was already sent, so don't send it again
        // TODO: fix send until acked
        //self.send_queue.retain(|packet| packet.packet.sequence_number().is_none() || packet.needs_ack);

        for _ in 0..count as usize {
            // TODO: can't pop here
            if let Some(packet) = self.send_queue.pop_front() {
                packets_to_send.push(packet.packet);
            } else {
                break;
            }
        }

        serialize_packets(&packets_to_send, self.buffer_size, &self.session)
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
            sequence_number <= max_sequence_number && sequence_number > self.next_client_sequence
        } else {
            sequence_number > self.next_client_sequence || sequence_number < max_sequence_number
        }

    }

    fn should_client_ack(recency_limit: SequenceNumber, next_server_sequence: SequenceNumber,
                         max: SequenceNumber, pending: SequenceNumber) -> bool {
        let min_sequence_number = next_server_sequence.wrapping_sub(recency_limit);

        // If the max is smaller, the sequence numbers wrapped around
        if min_sequence_number < max {
            min_sequence_number <= pending && pending < max
        } else {
            pending < max || pending >= min_sequence_number
        }
    }

    fn process_packet(&mut self, packet: Packet) {
        println!("Received packet op code {:?}", packet.op_code());
        match packet {
            Packet::SessionRequest(protocol_version, session_id,
                                   buffer_size, app_protocol) =>
                self.process_session_request(protocol_version, session_id, buffer_size, app_protocol),
            Packet::Data(_, data) => {
                if data[0] == 1 {
                    self.send_data(make_tunneled_packet(2, &vec![1]).unwrap());

                    let mut live_buf = "live".as_bytes().to_vec();
                    live_buf.push(0);
                    self.send_data(make_tunneled_packet(165, &live_buf).unwrap());

                    let mut zone_buffer = Vec::new();
                    zone_buffer.write_u32::<LittleEndian>(10).unwrap();
                    zone_buffer.extend("JediTemple".as_bytes());
                    zone_buffer.write_u32::<LittleEndian>(2).unwrap();
                    zone_buffer.write_u8(0).unwrap();
                    zone_buffer.write_u8(0).unwrap();
                    zone_buffer.write_u32::<LittleEndian>(0).unwrap();
                    zone_buffer.extend("".as_bytes());
                    zone_buffer.write_u8(0).unwrap();
                    zone_buffer.write_u32::<LittleEndian>(0).unwrap();
                    zone_buffer.write_u32::<LittleEndian>(5).unwrap();
                    self.send_data(make_tunneled_packet(43, &zone_buffer).unwrap());

                    let mut settings_buffer = Vec::new();
                    settings_buffer.write_u32::<LittleEndian>(4).unwrap();
                    settings_buffer.write_u32::<LittleEndian>(7).unwrap();
                    settings_buffer.write_u32::<LittleEndian>(268).unwrap();
                    settings_buffer.write_u8(1).unwrap();
                    settings_buffer.write_f32::<LittleEndian>(1.0f32).unwrap();
                    self.send_data(make_tunneled_packet(143, &settings_buffer).unwrap());

                    //self.send_data(send_item_definitions().unwrap());

                    //println!("DONE SENDING ITEM DEFINITIONS");

                    self.send_data(send_self_to_client().unwrap());
                }
            }
            Packet::Heartbeat => self.process_heartbeat(),
            Packet::Ack(acked_sequence) => self.process_ack(acked_sequence),
            Packet::AckAll(acked_sequence) => self.process_ack_all(acked_sequence),
            _ => {}
        }
    }

    fn process_session_request(&mut self, protocol_version: SoeProtocolVersion, session_id: SessionId,
                               buffer_size: BufferSize, app_protocol: ApplicationProtocol) {

        // TODO: disallow session overwrite
        let session = Session {
            session_id,
            crc_length: 3,
            crc_seed: 12345,
            allow_compression: false,
            use_encryption: false,
        };

        self.buffer_size = buffer_size;
        self.send_queue.push_back(PendingPacket::new(Packet::SessionReply(
            session_id,
            session.crc_seed,
            session.crc_length,
            session.allow_compression,
            session.use_encryption,
            512,
            3
        )));
        self.session = Some(session);
    }

    fn process_heartbeat(&mut self) {
        self.send_queue.push_back(PendingPacket::new(Packet::Heartbeat));
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
                                              acked_sequence.wrapping_add(1), pending_sequence) {
                    pending_packet.needs_ack = false;
                }
            }
        }
    }

    fn send_data(&mut self, data: Vec<u8>) {
        let mut remaining_data = &data[..];
        let mut is_first = true;

        if let Some(session) = &self.session {
            let max_size = max_fragment_data_size(self.buffer_size, session) as usize;

            if remaining_data.len() <= max_size {
                let next_sequence = self.next_server_sequence();
                self.send_queue.push_back(PendingPacket::new(
                    Packet::Data(next_sequence, data)
                ));
                return;
            }

            while remaining_data.len() > 0 {
                let mut end = max_size.min(remaining_data.len());
                let mut buffer = Vec::new();
                if is_first {
                    buffer.write_u32::<BigEndian>(data.len() as u32).expect("Tried to write data length");
                    end -= size_of::<u32>();
                    is_first = false;
                }

                let fragment = &remaining_data[0..end];
                buffer.write_all(fragment).expect("Tried to write fragment data");
                remaining_data = &remaining_data[end..];

                let next_sequence = self.next_server_sequence();
                self.send_queue.push_back(PendingPacket::new(
                    Packet::DataFragment(next_sequence, buffer)
                ));
            }
        } else {
            panic!("Cannot send reliable data without a session");
        }
    }

    fn acknowledge_one(&mut self, sequence_number: SequenceNumber) {
        println!("ACKING {}", sequence_number);
        self.send_queue.push_back(PendingPacket::new(Packet::Ack(sequence_number)));
    }

    fn acknowledge_all(&mut self, sequence_number: SequenceNumber) {
        println!("ACKING ALL {}", sequence_number);
        self.send_queue.push_back(PendingPacket::new(Packet::AckAll(sequence_number)));
    }
}

type ChannelManager = RwLock<HashMap<SocketAddr, Mutex<Channel>>>;
