use std::io::{Cursor};
use std::time::{SystemTime, UNIX_EPOCH};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crate::game_server::login::{extract_tunneled_packet_data, make_tunneled_packet, send_player_data};

mod login;

pub enum OpCode {
    LoginRequest             = 0x1,
    LoginReply               = 0x2,
    TunneledClient           = 0x5,
    PlayerData               = 0xc,
    ClientIsReady            = 0xd,
    ZoneDetailsDone          = 0xe,
    ClientUpdate             = 0x26,
    ZoneDetails              = 0x2b,
    GameTimeSync             = 0x34,
    WelcomeScreen            = 0x5d,
    ClientGameSettings       = 0x8f,
    DeploymentEnv            = 0xa5,
}

pub enum ClientUpdateOpCode {
    Health                   = 0x1,
    Power                    = 0xd,
    Stats                    = 0x7,
    PreloadCharactersDone    = 0x1a
}

pub struct GameServer {
    
}

impl GameServer {
    
    pub fn process_packet(&mut self, data: Vec<u8>) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        let mut cursor = Cursor::new(&data);
        let op_code = cursor.read_u16::<LittleEndian>().unwrap();
        
        if op_code == OpCode::LoginRequest as u16 {
            packets.push(make_tunneled_packet(OpCode::LoginReply as u16, &vec![1]).unwrap());

            let mut live_buf = "live".as_bytes().to_vec();
            live_buf.push(0);
            packets.push(make_tunneled_packet(OpCode::DeploymentEnv as u16, &live_buf).unwrap());

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
            packets.push(make_tunneled_packet(OpCode::ZoneDetails as u16, &zone_buffer).unwrap());

            let mut settings_buffer = Vec::new();
            settings_buffer.write_u32::<LittleEndian>(4).unwrap();
            settings_buffer.write_u32::<LittleEndian>(7).unwrap();
            settings_buffer.write_u32::<LittleEndian>(268).unwrap();
            settings_buffer.write_u8(1).unwrap();
            settings_buffer.write_f32::<LittleEndian>(1.0f32).unwrap();
            packets.push(make_tunneled_packet(OpCode::ClientGameSettings as u16, &settings_buffer).unwrap());

            //packets.push(send_item_definitions().unwrap());

            //println!("DONE SENDING ITEM DEFINITIONS");

            packets.push(send_player_data().unwrap());
        } else if op_code == OpCode::TunneledClient as u16 {
            let (op_code, payload) = extract_tunneled_packet_data(&data).unwrap();
            if op_code == OpCode::ClientIsReady as u16 {
                println!("received client ready packet");

                let mut hp_buffer = Vec::new();
                hp_buffer.write_u16::<LittleEndian>(ClientUpdateOpCode::Health as u16).unwrap();
                hp_buffer.write_u32::<LittleEndian>(25000).unwrap();
                hp_buffer.write_u32::<LittleEndian>(25000).unwrap();
                packets.push(make_tunneled_packet(OpCode::ClientUpdate as u16, &hp_buffer).unwrap());

                let mut power_buffer = Vec::new();
                power_buffer.write_u16::<LittleEndian>(ClientUpdateOpCode::Power as u16).unwrap();
                power_buffer.write_u32::<LittleEndian>(300).unwrap();
                power_buffer.write_u32::<LittleEndian>(300).unwrap();
                packets.push(make_tunneled_packet(OpCode::ClientUpdate as u16, &power_buffer).unwrap());

                let mut stat_buffer = Vec::new();
                stat_buffer.write_u16::<LittleEndian>(ClientUpdateOpCode::Stats as u16).unwrap();
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

                packets.push(make_tunneled_packet(OpCode::ClientUpdate as u16, &stat_buffer).unwrap());

                // Welcome screen
                packets.push(make_tunneled_packet(OpCode::WelcomeScreen as u16, &vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap());

                // Zone done sending init data
                packets.push(make_tunneled_packet(OpCode::ZoneDetailsDone as u16, &Vec::new()).unwrap());

                // Preload characters
                let mut preload_characters_buffer = Vec::new();
                preload_characters_buffer.write_u16::<LittleEndian>(ClientUpdateOpCode::PreloadCharactersDone as u16).unwrap();
                preload_characters_buffer.write_u8(0).unwrap();
                packets.push(make_tunneled_packet(OpCode::ClientUpdate as u16, &preload_characters_buffer).unwrap());

            } else if op_code == OpCode::GameTimeSync as u16 {
                let mut buffer = Vec::new();
                let time = SystemTime::now().duration_since(UNIX_EPOCH)
                    .expect("Time went backwards").as_secs();
                println!("Sending time: {}", time);
                buffer.write_u64::<LittleEndian>(time).unwrap();
                buffer.write_u32::<LittleEndian>(0).unwrap();
                buffer.write_u8(1).unwrap();
                packets.push(make_tunneled_packet(OpCode::GameTimeSync as u16, &buffer).unwrap());
            } else {
                println!("Received unknown op code: {}", op_code);
            }
        }
        
        packets
    }
    
}
