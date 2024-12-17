use packet_serialize::{NullTerminatedString, SerializePacketError};

use crate::game_server::{
    packets::{
        login::{DefinePointsOfInterest, DeploymentEnv, GameSettings, LoginReply, PointOfInterest},
        player_update::ItemDefinitionsReply,
        tunnel::TunneledPacket,
        GamePacket, Pos,
    },
    Broadcast, GameServer, ProcessPacketError,
};

use super::{
    character::{BattleClass, Character, Player},
    lock_enforcer::ZoneLockRequest,
    minigame::PlayerMinigameStats,
    test_data::{make_test_customizations, make_test_player},
    unique_guid::player_guid,
    zone::Zone,
};

pub fn log_in(sender: u32, game_server: &GameServer) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server
        .lock_enforcer()
        .write_characters(|characters_write_handle, zone_lock_enforcer| {
            // TODO: get player's zone
            let player_zone = 24;

            let mut packets = Vec::new();

            let login_reply = TunneledPacket {
                unknown1: true,
                inner: LoginReply { logged_in: true },
            };
            packets.push(GamePacket::serialize(&login_reply)?);

            let deployment_env = TunneledPacket {
                unknown1: true,
                inner: DeploymentEnv {
                    environment: NullTerminatedString("prod".to_string()),
                },
            };
            packets.push(GamePacket::serialize(&deployment_env)?);

            packets.append(&mut zone_lock_enforcer.read_zones(|_| ZoneLockRequest {
                read_guids: vec![player_zone],
                write_guids: Vec::new(),
                zone_consumer: |_, zones_read, _| zones_read.get(&player_zone).unwrap().send_self(),
            })?);

            let settings = TunneledPacket {
                unknown1: true,
                inner: GameSettings {
                    unknown1: 4,
                    unknown2: 7,
                    unknown3: 268,
                    unknown4: true,
                    time_scale: 1.0,
                },
            };
            packets.push(GamePacket::serialize(&settings)?);

            let item_defs = TunneledPacket {
                unknown1: true,
                inner: ItemDefinitionsReply {
                    definitions: game_server.items(),
                },
            };
            packets.push(GamePacket::serialize(&item_defs)?);

            let player = TunneledPacket {
                unknown1: true,
                inner: make_test_player(sender, game_server.mounts(), game_server.items()),
            };
            packets.push(GamePacket::serialize(&player)?);

            characters_write_handle.insert(Character::from_player(
                sender,
                player.inner.data.pos,
                player.inner.data.rot,
                player_zone,
                Player {
                    ready: false,
                    member: player.inner.data.membership_unknown1,
                    credits: player.inner.data.credits,
                    battle_classes: player
                        .inner
                        .data
                        .battle_classes
                        .into_iter()
                        .map(|(battle_class_guid, battle_class)| {
                            (
                                battle_class_guid,
                                BattleClass {
                                    items: battle_class.items,
                                },
                            )
                        })
                        .collect(),
                    active_battle_class: player.inner.data.active_battle_class,
                    inventory: player.inner.data.inventory.into_keys().collect(),
                    customizations: make_test_customizations(),
                    minigame_stats: PlayerMinigameStats::default(),
                },
                game_server,
            ));

            Ok(vec![Broadcast::Single(sender, packets)])
        })
}

pub fn log_out(
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server
        .lock_enforcer()
        .write_characters(|characters_table_write_handle, _| {
            if let Some((character, (_, instance_guid, chunk))) =
                characters_table_write_handle.remove(player_guid(sender))
            {
                let other_players_nearby = Zone::other_players_nearby(
                    Some(sender),
                    chunk,
                    instance_guid,
                    characters_table_write_handle,
                )?;

                let remove_packets = character.read().remove_packets()?;

                Ok(vec![Broadcast::Multi(other_players_nearby, remove_packets)])
            } else {
                Ok(vec![])
            }
        })
}

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
