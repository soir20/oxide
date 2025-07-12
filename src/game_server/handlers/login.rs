use packet_serialize::NullTerminatedString;

use crate::{
    game_server::{
        handlers::minigame::{leave_active_minigame_if_any, LeaveMinigameTarget},
        packets::{
            login::{DefinePointsOfInterest, DeploymentEnv, GameSettings, LoginReply},
            player_update::ItemDefinitionsReply,
            tunnel::TunneledPacket,
            GamePacket,
        },
        Broadcast, GameServer, ProcessPacketError,
    },
    info,
};

use super::{
    character::{BattleClass, Character, Player, PreviousLocation, RemovalMode},
    guid::IndexedGuid,
    lock_enforcer::ZoneLockEnforcer,
    minigame::PlayerMinigameStats,
    test_data::{make_test_customizations, make_test_player},
    unique_guid::player_guid,
    zone::{clean_up_zone_if_no_players, ZoneInstance},
};

pub fn log_in(sender: u32, game_server: &GameServer) -> Result<Vec<Broadcast>, ProcessPacketError> {
    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, minigame_data_lock_enforcer| {
            let zones_lock_enforcer: ZoneLockEnforcer<'_> = minigame_data_lock_enforcer.into();
            // TODO: get player's zone
            let player_zone_template = 24;

            let mut packets = Vec::new();

            let login_reply = TunneledPacket {
                unknown1: true,
                inner: LoginReply { logged_in: true },
            };
            packets.push(GamePacket::serialize(&login_reply));

            let deployment_env = TunneledPacket {
                unknown1: true,
                inner: DeploymentEnv {
                    environment: NullTerminatedString("prod".to_string()),
                },
            };
            packets.push(GamePacket::serialize(&deployment_env));

            let (instance_guid, mut zone_packets) =
                zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                    let instance_guid = game_server.get_or_create_instance(
                        characters_table_write_handle,
                        zones_table_write_handle,
                        player_zone_template,
                        1,
                    )?;
                    let zone_read_handle =
                        zones_table_write_handle.get(instance_guid).unwrap().read();
                    Ok::<(u64, Vec<Vec<u8>>), ProcessPacketError>((
                        zone_read_handle.guid(),
                        zone_read_handle.send_self(sender)?,
                    ))
                })?;
            packets.append(&mut zone_packets);

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
            packets.push(GamePacket::serialize(&settings));

            let item_defs = TunneledPacket {
                unknown1: true,
                inner: ItemDefinitionsReply {
                    definitions: game_server.items(),
                },
            };
            packets.push(GamePacket::serialize(&item_defs));

            let player = TunneledPacket {
                unknown1: true,
                inner: make_test_player(sender, game_server.mounts(), game_server.items()),
            };
            packets.push(GamePacket::serialize(&player));

            characters_table_write_handle.insert(Character::from_player(
                sender,
                player.inner.data.body_model,
                player.inner.data.pos,
                player.inner.data.rot,
                instance_guid,
                Player {
                    first_load: true,
                    ready: false,
                    name: player.inner.data.name,
                    squad_guid: None,
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
                    minigame_status: None,
                    update_previous_location_on_leave: true,
                    previous_location: PreviousLocation {
                        template_guid: player_zone_template,
                        pos: player.inner.data.pos,
                        rot: player.inner.data.rot,
                    },
                },
                game_server,
            ));

            Ok(vec![Broadcast::Single(sender, packets)])
        },
    )
}

pub fn log_out(sender: u32, game_server: &GameServer) -> Vec<Broadcast> {
    info!("Logging out player {}", sender);
    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, minigame_data_lock_enforcer| {
            minigame_data_lock_enforcer.write_minigame_data(
                |minigame_data_write_handle, zones_lock_enforcer| {
                    zones_lock_enforcer.write_zones(|zones_table_write_handle| {
                        let mut broadcasts = Vec::new();

                        let leave_minigame_result = leave_active_minigame_if_any(
                            LeaveMinigameTarget::Single(sender),
                            characters_table_write_handle,
                            minigame_data_write_handle,
                            zones_table_write_handle,
                            None,
                            false,
                            game_server,
                        );
                        match leave_minigame_result {
                            Ok(mut leave_minigame_broadcasts) => broadcasts.append(&mut leave_minigame_broadcasts),
                            Err(err) => info!("Unable to remove player {} from minigame as they were logging out: {}", sender, err),
                        }

                        let Some((character, (_, instance_guid, chunk), ..)) =
                            characters_table_write_handle.remove(player_guid(sender))
                        else {
                            return broadcasts;
                        };

                        let other_players_nearby = ZoneInstance::other_players_nearby(
                            Some(sender),
                            chunk,
                            instance_guid,
                            characters_table_write_handle,
                        );

                        let remove_packets = character
                            .read()
                            .stats
                            .remove_packets(RemovalMode::default());
                        broadcasts.push(Broadcast::Multi(other_players_nearby, remove_packets));

                        clean_up_zone_if_no_players(
                            instance_guid,
                            characters_table_write_handle,
                            zones_table_write_handle,
                        );

                        broadcasts
                    })
                },
            )
        },
    )
}

pub fn send_points_of_interest(game_server: &GameServer) -> Vec<Vec<u8>> {
    let mut points = Vec::new();
    for point_of_interest in game_server.points_of_interest().values() {
        points.push(point_of_interest.into());
    }

    vec![GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner: DefinePointsOfInterest { points },
    })]
}
