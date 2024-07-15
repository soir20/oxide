use packet_serialize::SerializePacketError;

use crate::game_server::{
    packets::{
        login::{DefinePointsOfInterest, PointOfInterest},
        tunnel::TunneledPacket,
        GamePacket, Pos,
    },
    GameServer,
};

pub fn send_points_of_interest(
    game_server: &GameServer,
) -> Result<Vec<Vec<u8>>, SerializePacketError> {
    let mut points = Vec::new();
    for (guid, _) in game_server.zone_templates.iter() {
        points.push(PointOfInterest {
            id: *guid as u32,
            name_id: 0,
            location_id: 0,
            teleport_pos: Pos {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            icon_id: 0,
            notification_type: 0,
            subtitle_id: 0,
            unknown: 0,
            quest_id: 0,
            teleport_pos_id: 0,
        });
    }

    Ok(vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: DefinePointsOfInterest { points },
    })?])
}
