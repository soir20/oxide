use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use byteorder::{LittleEndian, WriteBytesExt};
use rand::random;
use crate::deserialize::{deserialize_packet, DeserializeError};
use crate::hash::{CrcSeed, CrcSize};
use crate::login::{extract_tunneled_packet_data, make_tunneled_packet, send_self_to_client};
use crate::reliable_data_ops::{DataPacket, fragment_data, FragmentState};
use crate::serialize::{serialize_packets, SerializeError};

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
    needs_send: bool,
    packet: Packet
}

impl PendingPacket {
    fn new(packet: Packet) -> Self {
        PendingPacket {
            needs_send: true,
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
            fragment_state: FragmentState::new(),
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
        let mut indices_to_send = Vec::new();

        // If the packet was acked, it was already sent, so don't send it again
        self.send_queue.retain(|packet| packet.needs_send);

        let mut index = 0;
        while indices_to_send.len() < count as usize && index < self.send_queue.len() {
            let mut packet = &mut self.send_queue[index];

            // Packets without sequence numbers do not need to be acked, so they
            // are always sent exactly once.
            if packet.packet.sequence_number().is_none() {
                packet.needs_send = false;
            }

            indices_to_send.push(index);
            index += 1;
        }

        let packets_to_send: Vec<&Packet> = indices_to_send.into_iter()
            .map(|index| &self.send_queue[index].packet)
            .collect();

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
            min_sequence_number <= pending && pending <= max
        } else {
            min_sequence_number <= pending || pending <= max
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
                    //self.send_data(make_tunneled_packet(0x8f, &settings_buffer).unwrap());

                    //self.send_data(send_item_definitions().unwrap());

                    //println!("DONE SENDING ITEM DEFINITIONS");

                    self.send_data(send_self_to_client().unwrap());
                } else if data[3] == 5 {
                    let (op_code, payload) = extract_tunneled_packet_data(&data[3..]).unwrap();
                    if op_code == 13 {
                        println!("received client ready packet");

                        let mut point_of_interest_buffer = Vec::new();
                        point_of_interest_buffer.write_u8(1).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(3961).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(281).unwrap();
                        point_of_interest_buffer.write_f32::<LittleEndian>(887.30).unwrap();
                        point_of_interest_buffer.write_f32::<LittleEndian>(173.0).unwrap();
                        point_of_interest_buffer.write_f32::<LittleEndian>(1546.956).unwrap();
                        point_of_interest_buffer.write_f32::<LittleEndian>(1.0).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(0).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(7).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(382845).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(651).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(0).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(210020).unwrap();
                        point_of_interest_buffer.write_u32::<LittleEndian>(60).unwrap();
                        point_of_interest_buffer.write_u8(0).unwrap();
                        let mut poi_buffer2 = Vec::new();
                        poi_buffer2.write_u32::<LittleEndian>(point_of_interest_buffer.len() as u32).unwrap();
                        poi_buffer2.write_all(&point_of_interest_buffer).unwrap();
                        //self.send_data(make_tunneled_packet(0x39, &poi_buffer2).unwrap());

                        let mut hp_buffer = Vec::new();
                        hp_buffer.write_u16::<LittleEndian>(1).unwrap();
                        hp_buffer.write_u32::<LittleEndian>(25000).unwrap();
                        hp_buffer.write_u32::<LittleEndian>(25000).unwrap();
                        self.send_data(make_tunneled_packet(0x26, &hp_buffer).unwrap());

                        let mut mana_buffer = Vec::new();
                        mana_buffer.write_u16::<LittleEndian>(0xd).unwrap();
                        mana_buffer.write_u32::<LittleEndian>(300).unwrap();
                        mana_buffer.write_u32::<LittleEndian>(300).unwrap();
                        self.send_data(make_tunneled_packet(0x26, &mana_buffer).unwrap());

                        let mut stat_buffer = Vec::new();
                        stat_buffer.write_u16::<LittleEndian>(7).unwrap();
                        stat_buffer.write_u32::<LittleEndian>(5).unwrap();

                        // Movement speed
                        stat_buffer.write_u32::<LittleEndian>(2).unwrap();
                        stat_buffer.write_u32::<LittleEndian>(1).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(8.0).unwrap();

                        // Health refill
                        stat_buffer.write_u32::<LittleEndian>(4).unwrap();
                        stat_buffer.write_u32::<LittleEndian>(0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(1.0).unwrap();

                        // Energy refill
                        stat_buffer.write_u32::<LittleEndian>(6).unwrap();
                        stat_buffer.write_u32::<LittleEndian>(0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(1.0).unwrap();

                        // Extra gravity
                        stat_buffer.write_u32::<LittleEndian>(58).unwrap();
                        stat_buffer.write_u32::<LittleEndian>(0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();

                        // Extra jump height
                        stat_buffer.write_u32::<LittleEndian>(59).unwrap();
                        stat_buffer.write_u32::<LittleEndian>(0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();
                        stat_buffer.write_f32::<LittleEndian>(0.0).unwrap();

                        self.send_data(make_tunneled_packet(0x26, &stat_buffer).unwrap());

                        // Welcome screen
                        self.send_data(make_tunneled_packet(0x5d, &vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap());

                        // Zone done sending init data
                        self.send_data(make_tunneled_packet(0xe, &Vec::new()).unwrap());

                        // Preload characters
                        self.send_data(make_tunneled_packet(0x26, &vec![0x1a, 0, 0]).unwrap());

                    } else {
                        println!("Received unknown op code: {}", op_code);
                    }
                } else if data[0] == 5 && data[7] == 0x34 {
                    let mut buffer = Vec::new();
                    let time = SystemTime::now().duration_since(UNIX_EPOCH)
                        .expect("Time went backwards").as_secs();
                    println!("Sending time: {}", time);
                    buffer.write_u64::<LittleEndian>(time).unwrap();
                    buffer.write_u32::<LittleEndian>(0).unwrap();
                    buffer.write_u8(1).unwrap();
                    self.send_data(make_tunneled_packet(0x34, &buffer).unwrap());
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
            crc_seed: random::<CrcSeed>(),
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
                                      self.next_server_sequence.wrapping_sub(1), acked_sequence) {
            for pending_packet in self.send_queue.iter_mut() {
                if let Some(pending_sequence) = pending_packet.packet.sequence_number() {
                    if acked_sequence == pending_sequence {
                        pending_packet.needs_send = false;
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
                    pending_packet.needs_send = false;
                }
            }
        }
    }

    fn send_data(&mut self, data: Vec<u8>) {
        let packets = fragment_data(self.buffer_size, &self.session, data)
            .expect("Unable to fragment data");

        for packet in packets {
            let sequence = self.next_server_sequence();
            let sequenced_packet = match packet {
                DataPacket::Fragment(data) => Packet::DataFragment(sequence, data),
                DataPacket::Single(data) => Packet::Data(sequence, data)
            };

            self.send_queue.push_back(PendingPacket::new(sequenced_packet));
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
