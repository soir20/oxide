use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use byteorder::{LittleEndian, WriteBytesExt};
use crate::game_server::login::{extract_tunneled_packet_data, make_tunneled_packet, send_self_to_client};

mod login;

pub struct GameServer {
    
}

impl GameServer {
    
    pub fn process_packet(&mut self, data: Vec<u8>) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        
        if data[0] == 1 {
            packets.push(make_tunneled_packet(2, &vec![1]).unwrap());

            let mut live_buf = "live".as_bytes().to_vec();
            live_buf.push(0);
            packets.push(make_tunneled_packet(165, &live_buf).unwrap());

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
            packets.push(make_tunneled_packet(43, &zone_buffer).unwrap());

            let mut settings_buffer = Vec::new();
            settings_buffer.write_u32::<LittleEndian>(4).unwrap();
            settings_buffer.write_u32::<LittleEndian>(7).unwrap();
            settings_buffer.write_u32::<LittleEndian>(268).unwrap();
            settings_buffer.write_u8(1).unwrap();
            settings_buffer.write_f32::<LittleEndian>(1.0f32).unwrap();
            //packets.push(make_tunneled_packet(0x8f, &settings_buffer).unwrap());

            //packets.push(send_item_definitions().unwrap());

            //println!("DONE SENDING ITEM DEFINITIONS");

            packets.push(send_self_to_client().unwrap());
        } else if data[0] == 5 {
            let (op_code, payload) = extract_tunneled_packet_data(&data).unwrap();
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
                //packets.push(make_tunneled_packet(0x39, &poi_buffer2).unwrap());

                let mut hp_buffer = Vec::new();
                hp_buffer.write_u16::<LittleEndian>(1).unwrap();
                hp_buffer.write_u32::<LittleEndian>(25000).unwrap();
                hp_buffer.write_u32::<LittleEndian>(25000).unwrap();
                packets.push(make_tunneled_packet(0x26, &hp_buffer).unwrap());

                let mut mana_buffer = Vec::new();
                mana_buffer.write_u16::<LittleEndian>(0xd).unwrap();
                mana_buffer.write_u32::<LittleEndian>(300).unwrap();
                mana_buffer.write_u32::<LittleEndian>(300).unwrap();
                packets.push(make_tunneled_packet(0x26, &mana_buffer).unwrap());

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

                packets.push(make_tunneled_packet(0x26, &stat_buffer).unwrap());

                // Welcome screen
                packets.push(make_tunneled_packet(0x5d, &vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap());

                // Zone done sending init data
                packets.push(make_tunneled_packet(0xe, &Vec::new()).unwrap());

                // Preload characters
                packets.push(make_tunneled_packet(0x26, &vec![0x1a, 0, 0]).unwrap());

            } else if op_code == 0x34 {
                let mut buffer = Vec::new();
                let time = SystemTime::now().duration_since(UNIX_EPOCH)
                    .expect("Time went backwards").as_secs();
                println!("Sending time: {}", time);
                buffer.write_u64::<LittleEndian>(time).unwrap();
                buffer.write_u32::<LittleEndian>(0).unwrap();
                buffer.write_u8(1).unwrap();
                packets.push(make_tunneled_packet(0x34, &buffer).unwrap());
            } else {
                println!("Received unknown op code: {}", op_code);
            }
        }
        
        packets
    }
    
}
