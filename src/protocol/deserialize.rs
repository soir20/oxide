use crate::protocol::hash::{compute_crc, CrcHash};
use crate::protocol::{DisconnectReason, Packet, ProtocolOpCode, Session};
use byteorder::{BigEndian, ReadBytesExt};
use miniz_oxide::inflate::{decompress_to_vec_zlib, DecompressError};
use std::io::{Cursor, Error, Read};
use std::mem::size_of;

use super::hash::{CrcSeed, CrcSize};

#[non_exhaustive]
#[derive(Debug)]
pub enum DeserializeError {
    IoError(Error),
    DecompressError(DecompressError),
    UnknownOpCode(u16),
    MismatchedHash(CrcHash, CrcHash, CrcSeed, CrcSize),
    UnknownDisconnectReason(u16),
    MissingSession(ProtocolOpCode),
    BadSubPacketLength,
}

impl From<Error> for DeserializeError {
    fn from(value: Error) -> Self {
        DeserializeError::IoError(value)
    }
}

impl From<DecompressError> for DeserializeError {
    fn from(value: DecompressError) -> Self {
        DeserializeError::DecompressError(value)
    }
}

fn check_op_code(op_code: u16) -> Result<ProtocolOpCode, DeserializeError> {
    match op_code {
        0x01 => Ok(ProtocolOpCode::SessionRequest),
        0x02 => Ok(ProtocolOpCode::SessionReply),
        0x03 => Ok(ProtocolOpCode::MultiPacket),
        0x05 => Ok(ProtocolOpCode::Disconnect),
        0x06 => Ok(ProtocolOpCode::Heartbeat),
        0x07 => Ok(ProtocolOpCode::NetStatusRequest),
        0x08 => Ok(ProtocolOpCode::NetStatusReply),
        0x09 => Ok(ProtocolOpCode::Data),
        0x0D => Ok(ProtocolOpCode::DataFragment),
        0x11 => Ok(ProtocolOpCode::Ack),
        0x15 => Ok(ProtocolOpCode::AckAll),
        0x1D => Ok(ProtocolOpCode::UnknownSender),
        0x1E => Ok(ProtocolOpCode::RemapConnection),
        _ => Err(DeserializeError::UnknownOpCode(op_code)),
    }
}

//noinspection DuplicatedCode
fn read_multi_packet_variable_length_int(data: &[u8]) -> Result<(u32, usize), DeserializeError> {
    let mut cursor = Cursor::new(data);

    if data.len() >= 2 && data[1] == 0 {
        Ok((data[0] as u32, size_of::<u8>()))
    } else if data.len() >= 3 && data[1] == 0xFF && data[2] == 0xFF {
        cursor.set_position(3);
        Ok((cursor.read_u32::<BigEndian>()?, 3 + size_of::<u32>()))
    } else {
        cursor.set_position(1);
        Ok((cursor.read_u16::<BigEndian>()? as u32, 1 + size_of::<u16>()))
    }
}

fn deserialize_session_request(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let protocol_version = cursor.read_u32::<BigEndian>()?;
    let session_id = cursor.read_u32::<BigEndian>()?;
    let buffer_size = cursor.read_u32::<BigEndian>()?;
    let mut application_protocol = String::new();
    cursor.read_to_string(&mut application_protocol)?;

    Ok(vec![Packet::SessionRequest(
        protocol_version,
        session_id,
        buffer_size,
        application_protocol,
    )])
}

fn deserialize_session_reply(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let session_id = cursor.read_u32::<BigEndian>()?;
    let crc_seed = cursor.read_u32::<BigEndian>()?;
    let crc_size = cursor.read_u8()?;
    let allow_compression = cursor.read_u8()? > 0;
    let encrypt = cursor.read_u8()? > 0;
    let buffer_size = cursor.read_u32::<BigEndian>()?;
    let protocol_version = cursor.read_u32::<BigEndian>()?;

    Ok(vec![Packet::SessionReply(
        session_id,
        crc_seed,
        crc_size,
        allow_compression,
        encrypt,
        buffer_size,
        protocol_version,
    )])
}

fn deserialize_multi_packet(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut offset = 0;
    let mut cursor = Cursor::new(data);
    let mut packets = Vec::new();

    while offset < data.len() {
        let (packet_length, new_offset) = read_multi_packet_variable_length_int(&data[offset..])?;
        offset += new_offset;
        cursor.set_position(offset as u64);

        if packet_length as usize > data[offset..].len() {
            return Err(DeserializeError::BadSubPacketLength);
        }

        let op_code = check_op_code(cursor.read_u16::<BigEndian>()?)?;
        offset += size_of::<u16>();
        let remaining_length = packet_length as usize - size_of::<u16>();

        let mut new_packets =
            deserialize_packet_data(&data[offset..(offset + remaining_length)], op_code)?;
        packets.append(&mut new_packets);
        offset += remaining_length;
    }

    Ok(packets)
}

fn check_disconnect_reason(reason: u16) -> Result<DisconnectReason, DeserializeError> {
    match reason {
        0 => Ok(DisconnectReason::Unknown),
        1 => Ok(DisconnectReason::IcmpError),
        2 => Ok(DisconnectReason::Timeout),
        3 => Ok(DisconnectReason::OtherSideTerminated),
        4 => Ok(DisconnectReason::ManagerDeleted),
        5 => Ok(DisconnectReason::ConnectFail),
        6 => Ok(DisconnectReason::Application),
        7 => Ok(DisconnectReason::UnreachableConnection),
        8 => Ok(DisconnectReason::UnacknowledgedTimeout),
        9 => Ok(DisconnectReason::NewConnectionAttempt),
        10 => Ok(DisconnectReason::ConnectionRefused),
        11 => Ok(DisconnectReason::ConnectError),
        12 => Ok(DisconnectReason::ConnectingToSelf),
        13 => Ok(DisconnectReason::ReliableOverflow),
        14 => Ok(DisconnectReason::ApplicationReleased),
        15 => Ok(DisconnectReason::CorruptPacket),
        16 => Ok(DisconnectReason::ProtocolMismatch),
        _ => Err(DeserializeError::UnknownDisconnectReason(reason)),
    }
}

fn deserialize_disconnect_reason(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let session_id = cursor.read_u32::<BigEndian>()?;
    let disconnect_reason = check_disconnect_reason(cursor.read_u16::<BigEndian>()?)?;
    Ok(vec![Packet::Disconnect(session_id, disconnect_reason)])
}

fn deserialize_net_status_request(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let client_tick_count = cursor.read_u16::<BigEndian>()?;
    let client_last_update = cursor.read_u32::<BigEndian>()?;
    let average_update = cursor.read_u32::<BigEndian>()?;
    let shortest_update = cursor.read_u32::<BigEndian>()?;
    let longest_update = cursor.read_u32::<BigEndian>()?;
    let last_server_update = cursor.read_u32::<BigEndian>()?;
    let packets_sent = cursor.read_u64::<BigEndian>()?;
    let packets_received = cursor.read_u64::<BigEndian>()?;
    let unknown = cursor.read_u16::<BigEndian>()?;
    Ok(vec![Packet::NetStatusRequest(
        client_tick_count,
        client_last_update,
        average_update,
        shortest_update,
        longest_update,
        last_server_update,
        packets_sent,
        packets_received,
        unknown,
    )])
}

fn deserialize_net_status_reply(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let client_tick_count = cursor.read_u16::<BigEndian>()?;
    let server_tick_count = cursor.read_u32::<BigEndian>()?;
    let client_packets_sent = cursor.read_u64::<BigEndian>()?;
    let client_packets_received = cursor.read_u64::<BigEndian>()?;
    let server_packets_sent = cursor.read_u64::<BigEndian>()?;
    let server_packets_received = cursor.read_u64::<BigEndian>()?;
    let unknown = cursor.read_u16::<BigEndian>()?;
    Ok(vec![Packet::NetStatusReply(
        client_tick_count,
        server_tick_count,
        client_packets_sent,
        client_packets_received,
        server_packets_sent,
        server_packets_received,
        unknown,
    )])
}

fn deserialize_reliable_data(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let sequence_number = cursor.read_u16::<BigEndian>()?;
    let remaining_data = data[size_of::<u16>()..].to_vec();
    Ok(vec![Packet::Data(sequence_number, remaining_data)])
}

fn deserialize_reliable_data_fragment(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);

    // We don't know if this packet is the first fragment, so deserialize only the same
    // fields as the complete reliable data packet.
    let sequence_number = cursor.read_u16::<BigEndian>()?;
    let remaining_data = data[size_of::<u16>()..].to_vec();

    Ok(vec![Packet::DataFragment(sequence_number, remaining_data)])
}

fn deserialize_ack(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let sequence_number = cursor.read_u16::<BigEndian>()?;
    Ok(vec![Packet::Ack(sequence_number)])
}

fn deserialize_ack_all(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let sequence_number = cursor.read_u16::<BigEndian>()?;
    Ok(vec![Packet::AckAll(sequence_number)])
}

fn deserialize_remap_connection(data: &[u8]) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let session_id = cursor.read_u32::<BigEndian>()?;
    let crc_seed = cursor.read_u32::<BigEndian>()?;
    Ok(vec![Packet::RemapConnection(session_id, crc_seed)])
}

fn deserialize_packet_data(
    data: &[u8],
    op_code: ProtocolOpCode,
) -> Result<Vec<Packet>, DeserializeError> {
    match op_code {
        ProtocolOpCode::SessionRequest => deserialize_session_request(data),
        ProtocolOpCode::SessionReply => deserialize_session_reply(data),
        ProtocolOpCode::MultiPacket => deserialize_multi_packet(data),
        ProtocolOpCode::Disconnect => deserialize_disconnect_reason(data),
        ProtocolOpCode::Heartbeat => Ok(vec![Packet::Heartbeat]),
        ProtocolOpCode::NetStatusRequest => deserialize_net_status_request(data),
        ProtocolOpCode::NetStatusReply => deserialize_net_status_reply(data),
        ProtocolOpCode::Data => deserialize_reliable_data(data),
        ProtocolOpCode::DataFragment => deserialize_reliable_data_fragment(data),
        ProtocolOpCode::Ack => deserialize_ack(data),
        ProtocolOpCode::AckAll => deserialize_ack_all(data),
        ProtocolOpCode::UnknownSender => Ok(vec![Packet::UnknownSender]),
        ProtocolOpCode::RemapConnection => deserialize_remap_connection(data),
    }
}

pub fn deserialize_packet(
    data: &[u8],
    possible_session: &Option<Session>,
) -> Result<Vec<Packet>, DeserializeError> {
    let mut cursor = Cursor::new(data);
    let op_code = check_op_code(cursor.read_u16::<BigEndian>()?)?;

    let mut packet_data;
    if op_code.requires_session() {
        if let Some(session) = possible_session {
            let compressed = session.allow_compression && cursor.read_u8()? != 0;

            // Two bytes for the op code and, optionally, one byte for the compression flag
            let data_offset = if session.allow_compression {
                size_of::<u8>()
            } else {
                0
            } + size_of::<u16>();

            let crc_offset = data
                .len()
                .checked_sub(session.crc_length as usize)
                .unwrap_or(data_offset);
            cursor.set_position(crc_offset as u64);
            let expected_hash = cursor.read_uint::<BigEndian>(session.crc_length as usize)? as u32;

            packet_data = data[data_offset..crc_offset].to_vec();
            if compressed {
                packet_data = decompress_to_vec_zlib(&packet_data)?;
            }
            let actual_hash =
                compute_crc(&data[0..crc_offset], session.crc_seed, session.crc_length);

            if actual_hash != expected_hash {
                return Err(DeserializeError::MismatchedHash(
                    actual_hash,
                    expected_hash,
                    session.crc_seed,
                    session.crc_length,
                ));
            }
        } else {
            return Err(DeserializeError::MissingSession(op_code));
        }
    } else {
        packet_data = data[2..].to_vec();
    }

    deserialize_packet_data(&packet_data, op_code)
}
