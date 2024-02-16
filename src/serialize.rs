use std::collections::VecDeque;
use std::io::{Error, Write};
use std::mem::size_of;
use byteorder::{BigEndian, WriteBytesExt};
use crate::hash::{compute_crc, CrcSeed, CrcSize};
use crate::protocol::{ApplicationProtocol, BufferSize, ClientTick, DisconnectReason, Packet, PacketCount, ProtocolOpCode, SequenceNumber, ServerTick, Session, SessionId, SoeProtocolVersion, Timestamp};

#[non_exhaustive]
#[derive(Debug)]
pub enum SerializeError {
    MissingSession,
    NonSessionPacketTooLarge(usize),
    BufferTooSmall(usize),
    IoError(Error)
}

impl From<Error> for SerializeError {
    fn from(value: Error) -> Self {
        SerializeError::IoError(value)
    }
}

fn write_variable_length_int(buffer: &mut Vec<u8>, value: BufferSize) -> Result<(), SerializeError> {
    if value <= 0xFF {
        buffer.write_u8(value as u8)?;
    } else if value < 0xFFFF {
        buffer.write_u8(0xFF)?;
        buffer.write_u16::<BigEndian>(value as u16)?;
    } else {
        buffer.write_u8(0xFF)?;
        buffer.write_u8(0xFF)?;
        buffer.write_u8(0xFF)?;
        buffer.write_u32::<BigEndian>(value)?;
    }

    Ok(())
}

fn serialize_session_request(protocol_version: SoeProtocolVersion, session_id: SessionId,
                             buffer_size: BufferSize, app_protocol: &ApplicationProtocol) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u32::<BigEndian>(protocol_version)?;
    buffer.write_u32::<BigEndian>(session_id)?;
    buffer.write_u32::<BigEndian>(buffer_size)?;
    buffer.write_all(app_protocol.as_bytes())?;
    Ok(buffer)
}

fn serialize_session_reply(session_id: SessionId, crc_seed: CrcSeed, crc_size: CrcSize,
                           allow_compression: bool, encrypt: bool, buffer_size: BufferSize,
                           protocol_version: SoeProtocolVersion) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u32::<BigEndian>(session_id)?;
    buffer.write_u32::<BigEndian>(crc_seed)?;
    buffer.write_u8(crc_size)?;
    buffer.write_u8(allow_compression as u8)?;
    buffer.write_u8(encrypt as u8)?;
    buffer.write_u32::<BigEndian>(buffer_size)?;
    buffer.write_u32::<BigEndian>(protocol_version)?;
    Ok(buffer)
}

fn serialize_disconnect(session_id: SessionId, disconnect_reason: DisconnectReason) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u32::<BigEndian>(session_id)?;
    buffer.write_u16::<BigEndian>(disconnect_reason as u16)?;
    Ok(buffer)
}

fn serialize_net_status_request(client_ticks: ClientTick, last_client_update: Timestamp,
                                average_update: Timestamp, shortest_update: Timestamp,
                                longest_update: Timestamp, last_server_update: Timestamp,
                                packets_sent: PacketCount, packets_received: PacketCount,
                                unknown: u16) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u16::<BigEndian>(client_ticks)?;
    buffer.write_u32::<BigEndian>(last_client_update)?;
    buffer.write_u32::<BigEndian>(average_update)?;
    buffer.write_u32::<BigEndian>(shortest_update)?;
    buffer.write_u32::<BigEndian>(longest_update)?;
    buffer.write_u32::<BigEndian>(last_server_update)?;
    buffer.write_u64::<BigEndian>(packets_sent)?;
    buffer.write_u64::<BigEndian>(packets_received)?;
    buffer.write_u16::<BigEndian>(unknown)?;
    Ok(buffer)
}

fn serialize_net_status_response(client_ticks: ClientTick, server_ticks: ServerTick,
                                 client_packets_sent: PacketCount, client_packets_received: PacketCount,
                                 server_packets_sent: PacketCount, server_packets_received: PacketCount,
                                 unknown: u16) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u16::<BigEndian>(client_ticks)?;
    buffer.write_u32::<BigEndian>(server_ticks)?;
    buffer.write_u64::<BigEndian>(client_packets_sent)?;
    buffer.write_u64::<BigEndian>(client_packets_received)?;
    buffer.write_u64::<BigEndian>(server_packets_sent)?;
    buffer.write_u64::<BigEndian>(server_packets_received)?;
    buffer.write_u16::<BigEndian>(unknown)?;
    Ok(buffer)
}

fn serialize_reliable_data(sequence_number: SequenceNumber, data: &[u8]) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u16::<BigEndian>(sequence_number)?;
    buffer.write_all(data)?;
    Ok(buffer)
}

fn serialize_ack(sequence_number: SequenceNumber) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u16::<BigEndian>(sequence_number)?;
    Ok(buffer)
}

fn serialize_remap_connection(session_id: SessionId, crc_seed: CrcSeed) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    buffer.write_u32::<BigEndian>(session_id)?;
    buffer.write_u32::<BigEndian>(crc_seed)?;
    Ok(buffer)
}

fn serialize_packet_data(packet: &Packet) -> Result<Vec<u8>, SerializeError> {
    match packet {
        Packet::SessionRequest(protocol_version, session_id,
                               buffer_size, app_protocol) =>
            serialize_session_request(*protocol_version, *session_id, *buffer_size, app_protocol),
        Packet::SessionReply(session_id, crc_seed, crc_size,
                             allow_compression, encrypt, buffer_size,
                             protocol_version) =>
            serialize_session_reply(*session_id, *crc_seed, *crc_size, *allow_compression, *encrypt,
                                    *buffer_size, *protocol_version),
        Packet::Disconnect(session_id, disconnect_reason) =>
            serialize_disconnect(*session_id, *disconnect_reason),
        Packet::Heartbeat => Ok(Vec::new()),
        Packet::NetStatusRequest(client_ticks, last_client_update,
                                 average_update, shortest_update,
                                 longest_update, last_server_update,
                                 packets_sent, packets_received,
                                 unknown) =>
            serialize_net_status_request(*client_ticks, *last_client_update, *average_update,
                                         *shortest_update, *longest_update, *last_server_update,
                                         *packets_sent, *packets_received, *unknown),
        Packet::NetStatusReply(client_ticks, server_ticks,
                               client_packets_sent, client_packets_received,
                               server_packets_sent, server_packets_received,
                               unknown) =>
            serialize_net_status_response(*client_ticks, *server_ticks, *client_packets_sent,
                                          *client_packets_received, *server_packets_sent,
                                          *server_packets_received, *unknown),
        Packet::Data(sequence_number, data) =>
            serialize_reliable_data(*sequence_number, data),
        Packet::DataFragment(sequence_number, data) =>
            serialize_reliable_data(*sequence_number, data),
        Packet::Ack(sequence_number) =>
            serialize_ack(*sequence_number),
        Packet::AckAll(sequence_number) =>
            serialize_ack(*sequence_number),
        Packet::UnknownSender => Ok(Vec::new()),
        Packet::RemapConnection(session_id, crc_seed) =>
            serialize_remap_connection(*session_id, *crc_seed)
    }
}

fn add_non_session_packets(buffers: &mut Vec<Vec<u8>>, non_session_packets: Vec<&Packet>, buffer_size: BufferSize) -> Result<(), SerializeError> {

    // Send non-session packets individually since the multi packet requires a session
    let mut serialized_packets = Vec::new();
    for packet in non_session_packets.into_iter() {
        let mut buffer = Vec::new();
        buffer.write_u16::<BigEndian>(packet.op_code() as u16)?;
        let mut packet_data = serialize_packet_data(packet)?;
        buffer.append(&mut packet_data);
        serialized_packets.push(buffer);
    }

    let max_no_session_len = serialized_packets.iter()
        .map(|buffer| buffer.len()).max()
        .unwrap_or(0);

    // Fragmented packets require a session, so reject non-session packets that are too large
    if max_no_session_len > buffer_size as usize {
        return Err(SerializeError::NonSessionPacketTooLarge(max_no_session_len));
    }

    buffers.append(&mut serialized_packets);

    Ok(())
}

fn header_size(session: &Session) -> u32 {
    if session.allow_compression {
        3
    } else {
        2
    }
}

fn footer_size(session: &Session) -> u32 {
    session.crc_length as u32
}

fn fits_data_fragment(space_left: BufferSize, needs_data_length: bool) -> bool {

    // Must always leave space for the op code
    let min_size = size_of::<u16>() as BufferSize;

    // Use strict comparisons because there should be space for one additional byte
    if needs_data_length {
        space_left > min_size + size_of::<u32>() as BufferSize
    } else {
        space_left > min_size
    }

}

fn split(space_left: BufferSize, data: Vec<u8>, needs_data_length: bool) -> Result<(Vec<u8>, Vec<u8>), SerializeError> {

    let mut fragment1 = if needs_data_length {
        let mut data_length_buffer = Vec::new();
        data_length_buffer.write_u32::<BigEndian>(data.len() as u32)?;
        data_length_buffer
    } else {
        Vec::new()
    };

    // The caller already knows the data length is larger than space_left
    fragment1.extend(&data[0..space_left as usize]);

    // Create a new buffer with space for the op code allocated
    let mut fragment2 = Vec::new();
    fragment2.write_u16::<BigEndian>(ProtocolOpCode::DataFragment as u16)?;
    fragment2.append(&mut data[space_left as usize..].to_vec());

    Ok((fragment1, fragment2))
}

fn group_session_packets(session_packets: Vec<&Packet>, buffer_size: BufferSize, session: &Session) -> Result<Vec<Vec<(ProtocolOpCode, Vec<u8>)>>, SerializeError> {
    let mut groups = Vec::new();
    let wrapper_size = header_size(session) + footer_size(session);
    let data_max_size = buffer_size.checked_sub(wrapper_size).unwrap_or(0);

    let mut serialized_packets = VecDeque::new();
    for packet in session_packets.into_iter() {
        serialized_packets.push_back(
            (true, packet.op_code(), serialize_packet_data(packet)?)
        );
    }

    let mut space_left = 0u32;
    let mut group = Vec::new();

    while !serialized_packets.is_empty() {
        let (need_data_length, op_code, serialized_packet) = serialized_packets.pop_front().unwrap();

        if serialized_packet.len() < space_left as usize {
            space_left -= serialized_packet.len() as BufferSize;
            group.push((op_code, serialized_packet));
        } else if (op_code == ProtocolOpCode::Data) && fits_data_fragment(space_left, need_data_length) {

            // Assume data fragment packets are already sufficiently fragmented
            let (fragment1, fragment2) = split(space_left, serialized_packet, need_data_length)?;

            space_left -= fragment1.len() as BufferSize;
            group.push((ProtocolOpCode::DataFragment, fragment1));

            serialized_packets.push_front((false, ProtocolOpCode::DataFragment, fragment2));

        } else if serialized_packet.len() > data_max_size as usize {
            return Err(SerializeError::BufferTooSmall(serialized_packet.len()));
        } else {
            groups.push(group.clone());
            group.clear();
            space_left = data_max_size;
            serialized_packets.push_front((need_data_length, op_code, serialized_packet));
        }
    }

    groups.push(group);

    Ok(groups)
}

fn write_header(buffer: &mut Vec<u8>, op_code: ProtocolOpCode, session: &Session, use_compression_if_allowed: bool) -> Result<(), SerializeError> {
    buffer.write_u16::<BigEndian>(op_code as u16)?;

    if session.allow_compression {
        buffer.write_u8(use_compression_if_allowed as u8)?;
    }

    Ok(())
}

fn add_session_packets(buffers: &mut Vec<Vec<u8>>, session_packets: Vec<&Packet>, buffer_size: BufferSize,
                       session: &Session, use_compression_if_allowed: bool) -> Result<(), SerializeError> {
    let groups = group_session_packets(session_packets, buffer_size, session)?;

    for mut group in groups.into_iter() {
        if group.len() == 0 {
            continue;
        }

        let mut buffer = Vec::new();
        if group.len() == 1 {
            let (op_code, data) = group.pop().unwrap();
            write_header(&mut buffer, op_code, session, use_compression_if_allowed)?;
            buffer.write_all(&data)?;
            buffer.write_uint::<BigEndian>(
                compute_crc(&data, session.crc_seed, session.crc_length) as u64,
                session.crc_length as usize
            )?;
        } else {
            write_header(&mut buffer, ProtocolOpCode::MultiPacket, session, use_compression_if_allowed)?;

            for (op_code, data) in group {
                write_variable_length_int(&mut buffer, data.len() as BufferSize + 2)?;
                buffer.write_u16::<BigEndian>(op_code as u16)?;
                buffer.write_all(&data)?;
            }

            let data_start = header_size(session) as usize;
            buffer.write_uint::<BigEndian>(
                compute_crc(&buffer[data_start..], session.crc_seed, session.crc_length) as u64,
                session.crc_length as usize
            )?;
        }

        buffers.push(buffer);
    }

    Ok(())
}

pub fn serialize_packets(packets: &[Packet], buffer_size: BufferSize,
                         possible_session: &Option<Session>) -> Result<Vec<Vec<u8>>, SerializeError> {
    let (require_session, no_require_session): (Vec<&Packet>, Vec<&Packet>) = packets.iter()
        .partition(|packet| packet.op_code().requires_session());
    let mut buffers = Vec::new();

    add_non_session_packets(&mut buffers, no_require_session, buffer_size)?;

    if let Some(session) = possible_session {
        add_session_packets(&mut buffers, require_session, buffer_size, session, true)?;
    } else if require_session.len() > 0 {
        return Err(SerializeError::MissingSession);
    }

    Ok(buffers)
}
