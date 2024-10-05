use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rand::random;

use crate::protocol::deserialize::{deserialize_packet, DeserializeError};
use crate::protocol::hash::{CrcSeed, CrcSize};
use crate::protocol::reliable_data_ops::{
    fragment_data, unbundle_reliable_data, DataPacket, FragmentState,
};
use crate::protocol::serialize::{serialize_packets, SerializeError};
use crate::ServerOptions;

mod deserialize;
mod hash;
mod reliable_data_ops;
mod serialize;

pub const MAX_BUFFER_SIZE: BufferSize = 512;

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

#[derive(Eq, PartialEq)]
enum SendTime {
    Instant(Instant),
    NeverSent,
}

struct PendingPacket {
    needs_send: bool,
    packet: Packet,
    last_send: Instant,
    first_send: SendTime,
}

impl PendingPacket {
    fn new(packet: Packet) -> Self {
        PendingPacket {
            needs_send: true,
            packet,
            last_send: Instant::now(),
            first_send: SendTime::NeverSent,
        }
    }

    pub fn is_reliable(&self) -> bool {
        self.packet.sequence_number().is_some()
    }

    pub fn update_last_send_time(&mut self) {
        self.last_send = Instant::now();
        if self.first_send == SendTime::NeverSent {
            self.first_send = SendTime::Instant(self.last_send);
        }
    }

    pub fn time_since_last_send(&self) -> Duration {
        let now = Instant::now();
        now.saturating_duration_since(self.last_send)
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
    connected: bool,
    pub addr: SocketAddr,
    session: Option<Session>,
    buffer_size: BufferSize,
    recency_limit: SequenceNumber,
    time_until_resend: Duration,
    last_round_trip_times: Vec<Duration>,
    next_round_trip_index: usize,
    selected_round_trip_index: usize,
    max_time_until_resend: Duration,
    fragment_state: FragmentState,
    send_queue: VecDeque<PendingPacket>,
    receive_queue: VecDeque<Packet>,
    reordered_packets: BTreeMap<SequenceNumber, Packet>,
    next_client_sequence: SequenceNumber,
    next_server_sequence: SequenceNumber,
    last_server_ack: SequenceNumber,
    last_receive_time: Instant,
}

impl Channel {
    pub fn new(
        addr: SocketAddr,
        initial_buffer_size: BufferSize,
        recency_limit: SequenceNumber,
        time_until_resend: Duration,
        max_round_trip_entries: usize,
        desired_resend_pct: u8,
        max_time_until_resend: Duration,
    ) -> Self {
        if desired_resend_pct >= 100 {
            panic!("desired_resend_pct must be less than 100")
        }

        Channel {
            connected: true,
            addr,
            session: None,
            buffer_size: initial_buffer_size,
            recency_limit,
            time_until_resend,
            last_round_trip_times: vec![Duration::default(); max_round_trip_entries],
            next_round_trip_index: 0,
            selected_round_trip_index: (100 - desired_resend_pct) as usize * max_round_trip_entries
                / 100,
            max_time_until_resend,
            fragment_state: FragmentState::new(),
            send_queue: VecDeque::new(),
            receive_queue: VecDeque::new(),
            reordered_packets: BTreeMap::new(),
            next_client_sequence: 0,
            next_server_sequence: 0,
            last_server_ack: 0,
            last_receive_time: Instant::now(),
        }
    }

    pub fn receive(&mut self, data: &[u8]) -> Result<u32, DeserializeError> {
        if !self.connected() {
            return Ok(0);
        }

        let mut packets = deserialize_packet(data, &self.session)?;

        let packet_count = packets.len() as u32;
        packets
            .drain(..)
            .for_each(|packet| self.receive_queue.push_back(packet));

        self.last_receive_time = Instant::now();
        Ok(packet_count)
    }

    pub fn needs_processing(&self) -> bool {
        (!self.receive_queue.is_empty() || !self.send_queue.is_empty()) && self.connected()
    }

    pub fn process_next(&mut self, count: u8, server_options: &ServerOptions) -> Vec<Vec<u8>> {
        if !self.connected() {
            return Vec::new();
        }

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
            self.process_packet(&packet, server_options);

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

    pub fn process_all(&mut self, server_options: &ServerOptions) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();

        while !self.receive_queue.is_empty() {
            packets.append(&mut self.process_next(u8::MAX, server_options));
        }

        packets
    }

    pub fn prepare_to_send_data(&mut self, data: Vec<u8>) {
        if !self.connected() {
            return;
        }

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
        if !self.connected() {
            return Ok(Vec::new());
        }

        let mut indices_to_send = Vec::new();

        self.update_time_until_resend();

        // If the packet was acked, it was already sent, so don't send it again
        self.send_queue.retain(|packet| packet.needs_send);

        let mut index = 0;
        while indices_to_send.len() < count as usize && index < self.send_queue.len() {
            let packet = &mut self.send_queue[index];

            // All later packets are newer than this packet, so they should also be skipped
            if packet.time_since_last_send() < self.time_until_resend {
                index += 1;
                continue;
            }

            // Unreliable packets do not need to be acked, so they are always sent exactly once.
            if !packet.is_reliable() {
                packet.needs_send = false;
            }

            indices_to_send.push(index);
            packet.update_last_send_time();
            index += 1;
        }

        let packets_to_send: Vec<&Packet> = indices_to_send
            .into_iter()
            .map(|index| &self.send_queue[index].packet)
            .collect();

        serialize_packets(&packets_to_send, self.buffer_size, &self.session)
    }

    pub fn disconnect(
        &mut self,
        disconnect_reason: DisconnectReason,
    ) -> Result<Vec<Vec<u8>>, SerializeError> {
        self.receive_queue.clear();
        self.send_queue.clear();
        self.connected = false;

        if let Some(session) = &self.session {
            serialize_packets(
                &[&Packet::Disconnect(session.session_id, disconnect_reason)],
                self.buffer_size,
                &self.session,
            )
        } else {
            Ok(Vec::new())
        }
    }

    pub fn disconnect_if_same_session(
        &mut self,
        session_id: SessionId,
        disconnect_reason: DisconnectReason,
    ) -> Result<Vec<Vec<u8>>, SerializeError> {
        if let Some(session) = &self.session {
            if session.session_id == session_id {
                self.disconnect(disconnect_reason)
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn elapsed_since_last_receive(&self) -> Duration {
        Instant::now().saturating_duration_since(self.last_receive_time)
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

    fn process_packet(&mut self, packet: &Packet, server_options: &ServerOptions) {
        println!("Received packet op code {:?}", packet.op_code());
        match packet {
            Packet::SessionRequest(protocol_version, session_id, buffer_size, app_protocol) => self
                .process_session_request(
                    *protocol_version,
                    *session_id,
                    *buffer_size,
                    app_protocol,
                    server_options,
                ),
            Packet::Heartbeat => self.process_heartbeat(),
            Packet::Ack(acked_sequence) => self.process_ack(*acked_sequence),
            Packet::AckAll(acked_sequence) => self.process_ack_all(*acked_sequence),
            Packet::Disconnect(session_id, disconnect_reason) => {
                let _ = self.process_disconnect(*session_id, *disconnect_reason);
            }
            _ => {}
        }
    }

    fn process_session_request(
        &mut self,
        protocol_version: SoeProtocolVersion,
        session_id: SessionId,
        buffer_size: BufferSize,
        app_protocol: &ApplicationProtocol,
        server_options: &ServerOptions,
    ) {
        let session: &mut Session = self.session.get_or_insert_with(|| Session {
            session_id,
            crc_length: server_options.crc_length,
            crc_seed: random::<CrcSeed>(),
            allow_compression: server_options.allow_packet_compression,
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
                MAX_BUFFER_SIZE,
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

    fn process_disconnect(
        &mut self,
        session_id: SessionId,
        disconnect_reason: DisconnectReason,
    ) -> Result<Vec<Vec<u8>>, SerializeError> {
        println!(
            "Client {} disconnected with reason {:?}",
            self.addr, disconnect_reason
        );
        self.disconnect_if_same_session(session_id, DisconnectReason::OtherSideTerminated)
    }

    fn update_time_until_resend(&mut self) {
        let mut ready_to_update = false;
        for packet in self.send_queue.iter() {
            if !packet.needs_send && packet.is_reliable() {
                if let SendTime::Instant(first_send) = packet.first_send {
                    self.last_round_trip_times[self.next_round_trip_index] =
                        Instant::now().saturating_duration_since(first_send);
                    self.next_round_trip_index += 1;

                    if self.next_round_trip_index == self.last_round_trip_times.len() {
                        self.next_round_trip_index = 0;
                        ready_to_update = true;
                    }
                } else {
                    panic!("Packet was marked as sent but has no timing statistics");
                }
            }
        }

        if ready_to_update {
            self.last_round_trip_times.sort();
            self.time_until_resend = self.last_round_trip_times[self.selected_round_trip_index]
                .min(self.max_time_until_resend);
        }
    }
}
