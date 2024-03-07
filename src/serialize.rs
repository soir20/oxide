use std::collections::VecDeque;
use std::io::{Error, Write};
use std::mem::size_of;
use byteorder::{BigEndian, WriteBytesExt};
use miniz_oxide::deflate::compress_to_vec_zlib;
use crate::hash::{compute_crc, CrcSeed, CrcSize};
use crate::protocol::{ApplicationProtocol, BufferSize, ClientTick, DisconnectReason, Packet, PacketCount, ProtocolOpCode, SequenceNumber, ServerTick, Session, SessionId, SoeProtocolVersion, Timestamp};

// Use 100 as an arbitrary threshold to avoid compressing packets that benefit
// little to nothing from compression
const ZLIB_COMPRESSION_LENGTH_THRESHOLD: usize = 100;

const ZLIB_COMPRESSION_LEVEL: u8 = 2;

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

fn variable_length_int_size(length: usize) -> usize {
    if length < 0xFF {
        size_of::<u8>()
    } else if length < 0xFFFF {
        size_of::<u16>() + 1
    } else {
        size_of::<u32>() + 3
    }
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

fn group_session_packets(session_packets: Vec<&Packet>, buffer_size: BufferSize, session: &Session) -> Result<Vec<Vec<(ProtocolOpCode, Vec<u8>)>>, SerializeError> {
    let mut groups = Vec::new();
    let wrapper_size = header_size(session) + footer_size(session);
    let data_max_size = buffer_size.checked_sub(wrapper_size).unwrap_or(0);

    let mut serialized_packets = VecDeque::new();
    for packet in session_packets.into_iter() {
        serialized_packets.push_back(
            (packet.op_code(), serialize_packet_data(packet)?)
        );
    }

    let mut space_left = data_max_size;
    let mut group: Vec<(ProtocolOpCode, Vec<u8>)> = Vec::new();

    while !serialized_packets.is_empty() {
        let (op_code, serialized_packet) = serialized_packets.pop_front().unwrap();

        // Add two bytes for the op code
        let mut total_len = serialized_packet.len();

        // Leave space for this packet's op code and data length if it is not the first packet.
        // If it is the first packet, then the op code is included in the header size.
        if group.len() > 0 {
            total_len += size_of::<u16>();
            total_len += variable_length_int_size(total_len);
        }

        // Leave space for the op code and data length of the first packet if not accounted for
        if group.len() == 1 {
            total_len += size_of::<u16>();
            total_len += variable_length_int_size(group[0].1.len() + size_of::<u16>());
        }

        if total_len <= space_left as usize {
            space_left -= total_len as BufferSize;
            group.push((op_code, serialized_packet));
        } else if serialized_packet.len() > data_max_size as usize {

            // Prevent infinite loop if the packet cannot fit into the buffer by itself
            return Err(SerializeError::BufferTooSmall(serialized_packet.len()));

        } else {
            groups.push(group.clone());
            group.clear();
            space_left = data_max_size;
            serialized_packets.push_front((op_code, serialized_packet));
        }
    }

    groups.push(group);

    Ok(groups)
}

fn write_header(buffer: &mut Vec<u8>, op_code: ProtocolOpCode, session: &Session, compressed: bool) -> Result<(), SerializeError> {
    buffer.write_u16::<BigEndian>(op_code as u16)?;

    if session.allow_compression {
        buffer.write_u8(compressed as u8)?;
    }

    Ok(())
}

fn try_compress(data: &mut Vec<u8>, session: &Session) -> bool {
    if session.allow_compression && data.len() > ZLIB_COMPRESSION_LENGTH_THRESHOLD {
        let compressed_data = compress_to_vec_zlib(&data, ZLIB_COMPRESSION_LEVEL);
        if data.len() > compressed_data.len() {
            *data = compressed_data;
            return true;
        }
    }

    false
}

fn add_session_packets(buffers: &mut Vec<Vec<u8>>, session_packets: Vec<&Packet>, buffer_size: BufferSize,
                       session: &Session) -> Result<(), SerializeError> {
    let groups = group_session_packets(session_packets, buffer_size, session)?;

    for mut group in groups.into_iter() {
        if group.len() == 0 {
            continue;
        }

        let mut buffer = Vec::new();
        if group.len() == 1 {
            let (op_code, mut data) = group.pop().unwrap();
            let compressed = try_compress(&mut data, session);
            write_header(&mut buffer, op_code, session, compressed)?;
            buffer.write_all(&data)?;
        } else {
            let mut all_data = Vec::new();
            for (op_code, data) in group {
                write_variable_length_int(&mut all_data, data.len() as BufferSize + 2)?;
                all_data.write_u16::<BigEndian>(op_code as u16)?;
                all_data.write_all(&data)?;
            }

            let compressed = try_compress(&mut all_data, session);
            write_header(&mut buffer, ProtocolOpCode::MultiPacket, session, compressed)?;
            buffer.write_all(&all_data)?;
        }

        buffer.write_uint::<BigEndian>(
            compute_crc(&buffer, session.crc_seed, session.crc_length) as u64,
            session.crc_length as usize
        )?;
        buffers.push(buffer);
    }

    Ok(())
}

pub fn max_fragment_data_size(buffer_size: BufferSize, session: &Session) -> u32 {

    // Fragment needs space for header, sequence number, and footer
    buffer_size - header_size(session) - size_of::<u16>() as u32 - footer_size(session) - 100

}

pub fn serialize_packets(packets: &[&Packet], buffer_size: BufferSize,
                         possible_session: &Option<Session>) -> Result<Vec<Vec<u8>>, SerializeError> {
    let (require_session, no_require_session): (Vec<&Packet>, Vec<&Packet>) = packets.iter()
        .partition(|packet| packet.op_code().requires_session());
    let mut buffers = Vec::new();

    add_non_session_packets(&mut buffers, no_require_session, buffer_size)?;

    if let Some(session) = possible_session {
        add_session_packets(&mut buffers, require_session, buffer_size, session)?;
    } else if require_session.len() > 0 {
        return Err(SerializeError::MissingSession);
    }

    Ok(buffers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_session_packets(buffer_size: BufferSize, session: Session) -> Vec<Vec<u8>> {
        let compression_byte = if session.allow_compression { 1 } else { 0 };
        let packets = vec![
            Packet::Disconnect(session.session_id, DisconnectReason::Application),
            Packet::Heartbeat,

            // Data packet should fit exactly
            // 5 bytes for the wrapper
            // 9 bytes for disconnect packet and its 1-byte length
            // 3 bytes for the heartbeat packet and its 1-byte length
            // 3 bytes for this data packet's length
            // 2 bytes for this data packet's op code
            // 2 bytes for this data packet's sequence number
            Packet::Data(3, vec![4; buffer_size as usize - 5 - 9 - 3 - 3 - 2 - 2 - compression_byte]),

            Packet::Disconnect(session.session_id, DisconnectReason::CorruptPacket),
            Packet::Heartbeat,

            // Data packet should overflow by 1 byte
            // 5 bytes for the wrapper
            // 9 bytes for disconnect packet and its 1-byte length
            // 3 bytes for the heartbeat packet and its 1-byte length
            // 3 bytes for this data packet's length
            // 2 bytes for this data packet's op code
            // 2 bytes for this data packet's sequence number
            Packet::Data(7, vec![8; buffer_size as usize - 5 - 9 - 3 - 3 - 2 - 2 - compression_byte + 1]),

            // Data packet should fit by itself exactly
            // 5 bytes for the wrapper
            // 2 bytes for this data packet's op code
            // 2 bytes for this data packet's sequence number
            Packet::Data(9, vec![10; buffer_size as usize - 5 - 2 - compression_byte]),

            Packet::Ack(11),
            Packet::AckAll(12),
        ];

        serialize_packets(
            &packets.iter().map(|packet| packet).collect::<Vec<&Packet>>(),
            buffer_size,
            &Some(session)
        ).unwrap()
    }

    #[test]
    fn test_good_session_packets_without_compression() {
        let buffer_size = 512;
        let session = Session {
            session_id: 12345,
            crc_length: 3,
            crc_seed: 67890,
            allow_compression: false,
            use_encryption: false,
        };

        let actual = make_test_session_packets(buffer_size, session);
        let expected: Vec<Vec<u8>> = vec![
            vec![
                0, 3,
                8, 0, 5, 0, 0, 48, 57, 0, 6,
                2, 0, 6,
                255, 1, 236, 0, 9, 0, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
                216, 255, 39
            ],
            vec![
                0, 3,
                8, 0, 5, 0, 0, 48, 57, 0, 15,
                2, 0, 6,
                137, 22, 228
            ],
            vec![
                0, 9, 0, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
                8, 8, 8, 8, 8, 8, 8,
                147, 247, 242
            ],
            vec![
                0, 9, 0, 9, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
                10, 10, 10, 10, 10, 10, 10, 10,
                22, 185, 46
            ],
            vec![
                0, 3,
                4, 0, 17, 0, 11,
                4, 0, 21, 0, 12,
                122, 81, 177
            ]
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_good_session_packets_with_compression() {
        let buffer_size = 512;
        let session = Session {
            session_id: 12345,
            crc_length: 3,
            crc_seed: 67890,
            allow_compression: true,
            use_encryption: false,
        };

        let actual = make_test_session_packets(buffer_size, session);
        let expected: Vec<Vec<u8>> = vec![
            vec![
                0, 3, 1,
                120, 94, 237, 192, 75, 21, 0, 64, 4, 64, 209, 55, 63, 103, 180, 177, 213, 63,
                142, 16, 4, 225, 126, 30, 152, 35, 27, 201, 21, 40, 231, 142, 14, 10, 14, 7,
                10, 24,
                188, 38, 243
            ],
            vec![
                0, 3, 0,
                8, 0, 5, 0, 0, 48, 57, 0, 15,
                2, 0, 6,
                9, 91, 117
            ],
            vec![
                0, 9, 1,
                120, 94, 237, 192, 49, 1, 0, 0, 0, 130, 48, 63, 237, 159, 216, 32, 176, 116, 66,
                56, 160, 187, 15, 72,
                245, 91, 70
            ],
            vec![
                0, 9, 1,
                120, 94, 237, 192, 49, 1, 0, 0, 0, 130, 48, 95, 237, 31, 216, 32, 176, 116, 66,
                58, 127, 240, 19, 186,
                27, 226, 100
            ],
            vec![
                0, 3, 0,
                4, 0, 17, 0, 11, 4, 0, 21, 0, 12,
                217, 39, 71
            ]
        ];

        assert_eq!(actual, expected);
    }

}
