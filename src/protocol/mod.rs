use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::random;

use crate::protocol::deserialize::{deserialize_packet, DeserializeError};
use crate::protocol::hash::{CrcSeed, CrcSize};
use crate::protocol::reliable_data_ops::{
    fragment_data, unbundle_reliable_data, DataPacket, FragmentState,
};
use crate::protocol::serialize::{serialize_packets, SerializeError};

mod deserialize;
mod hash;
mod reliable_data_ops;
mod serialize;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ProtocolOpCode {
    SessionRequest = 0x01,
    SessionReply = 0x02,
    MultiPacket = 0x03,
    Disconnect = 0x05,
    Heartbeat = 0x06,
    NetStatusRequest = 0x07,
    NetStatusReply = 0x08,
    Data = 0x09,
    DataFragment = 0x0D,
    Ack = 0x11,
    AckAll = 0x15,
    UnknownSender = 0x1D,
    RemapConnection = 0x1E,
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
            ProtocolOpCode::RemapConnection => false,
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
    ProtocolMismatch = 16,
}

pub type ClientTick = u16;
pub type ServerTick = u32;
pub type Timestamp = u32;
pub type PacketCount = u64;

pub enum Packet {
    SessionRequest(
        SoeProtocolVersion,
        SessionId,
        BufferSize,
        ApplicationProtocol,
    ),
    SessionReply(
        SessionId,
        CrcSeed,
        CrcSize,
        bool,
        bool,
        BufferSize,
        SoeProtocolVersion,
    ),
    Disconnect(SessionId, DisconnectReason),
    Heartbeat,
    NetStatusRequest(
        ClientTick,
        Timestamp,
        Timestamp,
        Timestamp,
        Timestamp,
        Timestamp,
        PacketCount,
        PacketCount,
        u16,
    ),
    NetStatusReply(
        ClientTick,
        ServerTick,
        PacketCount,
        PacketCount,
        PacketCount,
        PacketCount,
        u16,
    ),
    Data(SequenceNumber, Vec<u8>),
    DataFragment(SequenceNumber, Vec<u8>),
    Ack(SequenceNumber),
    AckAll(SequenceNumber),
    UnknownSender,
    RemapConnection(SessionId, CrcSeed),
}

impl Packet {
    pub fn sequence_number(&self) -> Option<SequenceNumber> {
        match self {
            Packet::Data(n, _) => Some(*n),
            Packet::DataFragment(n, _) => Some(*n),
            _ => None,
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
            Packet::RemapConnection(..) => ProtocolOpCode::RemapConnection,
        }
    }
}

struct PendingPacket {
    needs_send: bool,
    packet: Packet,
    last_prepare_to_send: u128,
}

impl PendingPacket {
    fn new(packet: Packet) -> Self {
        PendingPacket {
            needs_send: true,
            packet,
            last_prepare_to_send: 0,
        }
    }

    pub fn update_last_prepare_to_send_time(&mut self) {
        self.last_prepare_to_send = PendingPacket::now();
    }

    pub fn time_since_last_prepare_to_send(&self) -> u128 {
        let now = PendingPacket::now();
        now.checked_sub(self.last_prepare_to_send).unwrap_or(now)
    }

    fn now() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time before Unix epoch")
            .as_millis()
    }
}

pub struct Session {
    pub session_id: SessionId,
    pub crc_length: CrcSize,
    pub crc_seed: CrcSeed,
    pub allow_compression: bool,
    pub use_encryption: bool,
}

pub struct Channel {
    session: Option<Session>,
    buffer_size: BufferSize,
    recency_limit: SequenceNumber,
    millis_until_resend: u128,
    fragment_state: FragmentState,
    send_queue: VecDeque<PendingPacket>,
    receive_queue: VecDeque<Packet>,
    reordered_packets: BTreeMap<SequenceNumber, Packet>,
    next_client_sequence: SequenceNumber,
    next_server_sequence: SequenceNumber,
    last_server_ack: SequenceNumber,
}

impl Channel {
    pub fn new(
        initial_buffer_size: BufferSize,
        recency_limit: SequenceNumber,
        millis_until_resend: u128,
    ) -> Self {
        Channel {
            session: None,
            buffer_size: initial_buffer_size,
            recency_limit,
            millis_until_resend,
            fragment_state: FragmentState::new(),
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
            reordered_packets: BTreeMap::new(),
            next_client_sequence: 0,
            next_server_sequence: 0,
            last_server_ack: 0,
        }
    }

    pub fn receive(&mut self, data: &[u8]) -> Result<u32, DeserializeError> {
        let mut packets = deserialize_packet(data, &self.session)?;

        let packet_count = packets.len() as u32;
        packets
            .drain(..)
            .for_each(|packet| self.receive_queue.push_back(packet));
        Ok(packet_count)
    }

    pub fn process_next(&mut self, count: u8) -> Vec<Vec<u8>> {
        let mut needs_new_ack = false;
        let mut packets_to_process = Vec::new();

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
                    if let Some(next_packet) =
                        self.reordered_packets.remove(&self.next_client_sequence)
                    {
                        self.receive_queue.push_front(next_packet);
                    }
                }

                match self.fragment_state.add(packet) {
                    Ok(possible_packet) => {
                        if let Some(packet) = possible_packet {
                            packets_to_process.push(packet);
                        }
                    }
                    Err(err) => println!("Unable to process packet: {:?}", err),
                }
            } else {
                break;
            }
        }

        if needs_new_ack {
            self.acknowledge_all(self.last_server_ack);
        }

        let mut packets = Vec::new();
        for packet in packets_to_process {
            // Process the packet inside the protocol
            self.process_packet(&packet);

            // Only data packets need to be handled outside the protocol. We already
            // de-fragmented the data packet, so we don't need to check for fragments here.
            if let Packet::Data(_, data) = packet {
                if let Ok(mut unbundled_packets) = unbundle_reliable_data(&data) {
                    packets.append(&mut unbundled_packets);
                } else {
                    println!("Bad bundled packet");
                }
            }
        }

        packets
    }

    pub fn prepare_to_send_data(&mut self, data: Vec<u8>) {
        let packets =
            fragment_data(self.buffer_size, &self.session, data).expect("Unable to fragment data");

        for packet in packets {
            let sequence = self.next_server_sequence();
            let sequenced_packet = match packet {
                DataPacket::Fragment(data) => Packet::DataFragment(sequence, data),
                DataPacket::Single(data) => Packet::Data(sequence, data),
            };

            self.send_queue
                .push_back(PendingPacket::new(sequenced_packet));
        }
    }

    pub fn send_next(&mut self, count: u8) -> Result<Vec<Vec<u8>>, SerializeError> {
        let mut indices_to_send = Vec::new();

        // If the packet was acked, it was already sent, so don't send it again
        self.send_queue.retain(|packet| packet.needs_send);

        let mut index = 0;
        while indices_to_send.len() < count as usize && index < self.send_queue.len() {
            let packet = &mut self.send_queue[index];

            // All later packets are newer than this packet, so they should also be skipped
            if packet.time_since_last_prepare_to_send() < self.millis_until_resend {
                index += 1;
                continue;
            }

            // Packets without sequence numbers do not need to be acked, so they
            // are always sent exactly once.
            if packet.packet.sequence_number().is_none() {
                packet.needs_send = false;
            }

            indices_to_send.push(index);
            packet.update_last_prepare_to_send_time();
            index += 1;
        }

        let packets_to_send: Vec<&Packet> = indices_to_send
            .into_iter()
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

    fn should_client_ack(
        recency_limit: SequenceNumber,
        next_server_sequence: SequenceNumber,
        max: SequenceNumber,
        pending: SequenceNumber,
    ) -> bool {
        let min_sequence_number = next_server_sequence.wrapping_sub(recency_limit);

        // If the max is smaller, the sequence numbers wrapped around
        if min_sequence_number < max {
            min_sequence_number <= pending && pending <= max
        } else {
            min_sequence_number <= pending || pending <= max
        }
    }

    fn process_packet(&mut self, packet: &Packet) {
        println!("Received packet op code {:?}", packet.op_code());
        match packet {
            Packet::SessionRequest(protocol_version, session_id, buffer_size, app_protocol) => self
                .process_session_request(
                    *protocol_version,
                    *session_id,
                    *buffer_size,
                    app_protocol,
                ),
            Packet::Heartbeat => self.process_heartbeat(),
            Packet::Ack(acked_sequence) => self.process_ack(*acked_sequence),
            Packet::AckAll(acked_sequence) => self.process_ack_all(*acked_sequence),
            _ => {}
        }
    }

    fn process_session_request(
        &mut self,
        protocol_version: SoeProtocolVersion,
        session_id: SessionId,
        buffer_size: BufferSize,
        app_protocol: &ApplicationProtocol,
    ) {
        let session = self.session.get_or_insert_with(|| Session {
            session_id,
            crc_length: 3,
            crc_seed: random::<CrcSeed>(),
            allow_compression: true,
            use_encryption: false,
        });

        self.buffer_size = buffer_size;
        self.send_queue
            .push_back(PendingPacket::new(Packet::SessionReply(
                session_id,
                session.crc_seed,
                session.crc_length,
                session.allow_compression,
                session.use_encryption,
                512,
                3,
            )));
    }

    fn process_heartbeat(&mut self) {
        self.send_queue
            .push_back(PendingPacket::new(Packet::Heartbeat));
    }

    fn process_ack(&mut self, acked_sequence: SequenceNumber) {
        if Channel::should_client_ack(
            self.recency_limit,
            self.next_server_sequence,
            self.next_server_sequence.wrapping_sub(1),
            acked_sequence,
        ) {
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
                if Channel::should_client_ack(
                    self.recency_limit,
                    self.next_server_sequence,
                    acked_sequence,
                    pending_sequence,
                ) {
                    pending_packet.needs_send = false;
                }
            }
        }
    }

    fn acknowledge_one(&mut self, sequence_number: SequenceNumber) {
        self.send_queue
            .push_back(PendingPacket::new(Packet::Ack(sequence_number)));
    }

    fn acknowledge_all(&mut self, sequence_number: SequenceNumber) {
        self.send_queue
            .push_back(PendingPacket::new(Packet::AckAll(sequence_number)));
    }
}
