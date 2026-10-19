use std::collections::BTreeMap;

use packet_serialize::NullTerminatedString;

use crate::{
    backend::{AccountRole, CharacterSnapshot, Redeemed, SavedCustomization},
    game_server::{
        handlers::{
            character::{BattleClass, PlayerActionBar, PlayerInventory, Toggles},
            minigame::{leave_active_minigame_if_any, LeaveMinigameTarget},
        },
        packets::{
            item::{EquipmentSlot, ItemDefinition},
            login::{DefinePointsOfInterest, DeploymentEnv, GameSettings, LoginReply},
            player_data::{EquippedItem, PlayerData},
            player_update::CustomizationSlot,
            player_update::ItemDefinitionsReply,
            tunnel::TunneledPacket,
            GamePacket, Name, Pos,
        },
        Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
    },
    info,
};

use super::{
    character::{
        Character, CharacterStats, CharacterType, Player, PreviousLocation, RemovalMode, Role,
    },
    guid::IndexedGuid,
    lock_enforcer::{CharacterLockRequest, ZoneLockEnforcer},
    minigame::PlayerMinigameStats,
    test_data::{make_test_customizations, make_test_player, make_test_weapon_abilities},
    unique_guid::{player_guid, zone_template_guid},
    zone::{clean_up_zone_if_no_players, ZoneInstance},
};

const DEFAULT_ZONE_TEMPLATE: u8 = 24;

/// Where and how a character resumes: its saved state applied over the test player.
struct StartingState {
    player: crate::game_server::packets::player_data::Player,
    customizations: BTreeMap<CustomizationSlot, i32>,
    zone_template: u8,
    role: Role,
}

/// Emissary is not a backend role, so only staff can run Emissary-level commands.
fn role_from(role: AccountRole) -> Role {
    match role {
        AccountRole::Member => Role::Citizen,
        AccountRole::Counselor => Role::Counselor,
        AccountRole::Enforcer => Role::Enforcer,
        AccountRole::Admin => Role::Admin,
    }
}

/// Development mode (no `redeemed`) keeps upstream's test player: an admin with a membership.
fn starting_state(
    sender: u32,
    redeemed: Option<Redeemed>,
    game_server: &GameServer,
) -> StartingState {
    let mut start = StartingState {
        player: make_test_player(sender, game_server.mounts(), game_server.items()),
        customizations: make_test_customizations(),
        zone_template: DEFAULT_ZONE_TEMPLATE,
        role: Role::Admin,
    };
    let Some(Redeemed {
        account,
        character: saved,
    }) = redeemed
    else {
        return start;
    };

    start.role = role_from(account.role);
    // Only the first flag is known to mean membership (it sets `Player::member`); the others
    // are unconfirmed and follow it, as they do for upstream's test player
    let member = account.is_member();
    let data = &mut start.player.data;
    data.membership_unknown1 = member;
    data.membership_unknown2 = member;
    data.membership_unknown3 = member;
    data.membership_unknown4 = member;

    start.player.data.name = Name {
        first_name: saved.name,
        last_name: String::new(),
        ..start.player.data.name
    };
    let Some(state) = saved.state else {
        return start;
    };

    let data = &mut start.player.data;
    match game_server.read_zone_templates().get(&state.zone_template) {
        Some(template)
            if template.update_previous_location_on_leave()
                && (member || !template.members_only()) =>
        {
            start.zone_template = state.zone_template;
            data.pos = pos_from(state.pos);
            data.rot = pos_from(state.rot);
        }
        _ => info!(
            "Player {} was saved in zone template {}, which cannot be resumed; using the default zone",
            sender, state.zone_template
        ),
    }
    data.credits = state.credits;

    for (battle_class_guid, battle_class) in data.battle_classes.iter_mut() {
        battle_class.items = state
            .equipment
            .iter()
            .filter(|item| item.battle_class == *battle_class_guid)
            .filter(|item| game_server.items().contains_key(&item.item_guid))
            .filter_map(|item| {
                let slot = EquipmentSlot::try_from(item.slot).ok()?;
                Some((
                    slot,
                    EquippedItem {
                        slot,
                        guid: item.item_guid,
                        category: 0,
                    },
                ))
            })
            .collect();
    }
    if data.battle_classes.contains_key(&state.active_battle_class) {
        data.active_battle_class = state.active_battle_class;
    }

    for saved_customization in &state.customizations {
        let Ok(slot) = CustomizationSlot::try_from(saved_customization.slot) else {
            continue;
        };
        if game_server
            .customizations()
            .contains_key(&saved_customization.customization_id)
        {
            start
                .customizations
                .insert(slot, saved_customization.customization_id);
        }
    }
    apply_appearance(data, &start.customizations, game_server);

    start
}

/// Sets the appearance fields the client reads for its own character from the customizations,
/// the same way `Player::add_packets` does for other players.
fn apply_appearance(
    data: &mut PlayerData,
    customizations: &BTreeMap<CustomizationSlot, i32>,
    game_server: &GameServer,
) {
    let param = |slot| {
        customizations
            .get(&slot)
            .and_then(|guid| game_server.customizations().get(guid))
    };
    if let Some(customization) = param(CustomizationSlot::BodyModel) {
        data.body_model = customization.customization_param2;
    }
    if let Some(customization) = param(CustomizationSlot::HeadModel) {
        data.head_model = customization.customization_param1.clone();
    }
    if let Some(customization) = param(CustomizationSlot::HairStyle) {
        data.hair_model = customization.customization_param1.clone();
    }
    if let Some(customization) = param(CustomizationSlot::HairColor) {
        data.hair_color = customization.customization_param2;
    }
    if let Some(customization) = param(CustomizationSlot::EyeColor) {
        data.eye_color = customization.customization_param2;
    }
    if let Some(customization) = param(CustomizationSlot::SkinTone) {
        data.skin_tone = customization.customization_param1.clone();
    }
    if let Some(customization) = param(CustomizationSlot::FacePattern) {
        data.face_paint = customization.customization_param1.clone();
    }
    if let Some(customization) = param(CustomizationSlot::FacialHair) {
        data.facial_hair = customization.customization_param1.clone();
    }
}

fn pos_from([x, y, z, w]: [f32; 4]) -> Pos {
    Pos { x, y, z, w }
}

fn pos_to(pos: Pos) -> [f32; 4] {
    [pos.x, pos.y, pos.z, pos.w]
}

/// What is saved for a player. Players in a minigame, house or lobby are saved at the location
/// they will be returned to, not inside it. `location` overrides that choice.
pub fn snapshot_player(
    stats: &CharacterStats,
    location: Option<&PreviousLocation>,
) -> Option<CharacterSnapshot> {
    let CharacterType::Player(player) = &stats.character_type else {
        return None;
    };
    let (zone_template, pos, rot) = match location {
        Some(location) => (location.template_guid, location.pos, location.rot),
        None if player.minigame_status.is_some() || !player.update_previous_location_on_leave => (
            player.previous_location.template_guid,
            player.previous_location.pos,
            player.previous_location.rot,
        ),
        None => (
            zone_template_guid(stats.instance_guid),
            stats.pos,
            stats.rot,
        ),
    };
    Some(CharacterSnapshot {
        zone_template,
        pos: pos_to(pos),
        rot: pos_to(rot),
        credits: player.credits,
        active_battle_class: player.inventory.active_battle_class,
        equipment: player.inventory.saved_equipment(),
        customizations: player
            .customizations
            .iter()
            .map(|(slot, customization_id)| SavedCustomization {
                slot: (*slot).into(),
                customization_id: *customization_id,
            })
            .collect(),
    })
}

/// Queues an online player's character to be saved. Does nothing in development mode.
pub fn save_player(sender: u32, game_server: &GameServer) {
    let Some(sink) = game_server.character_sink() else {
        return;
    };
    let snapshot = game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: vec![player_guid(sender)],
            write_guids: Vec::new(),
            character_consumer: |_, characters_read, _, _| {
                characters_read
                    .get(&player_guid(sender))
                    .and_then(|character| snapshot_player(&character.stats, None))
            },
        });
    if let Some(snapshot) = snapshot {
        sink.save(sender, snapshot);
    }
}

/// `redeemed` is the account and character from the backend, or `None` in development mode.
pub fn log_in(
    sender: u32,
    redeemed: Option<Redeemed>,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    // A snapshot from the session that just ended is newer than what the backend returned
    let redeemed = redeemed.map(|mut redeemed| {
        if let Some(recent) = game_server
            .character_sink()
            .and_then(|sink| sink.recent(sender))
        {
            redeemed.character.state = Some(recent);
        }
        redeemed
    });
    let StartingState {
        player: player_data,
        customizations,
        zone_template: player_zone_template,
        role,
    } = starting_state(sender, redeemed, game_server);

    game_server.lock_enforcer().write_characters(
        |characters_table_write_handle, minigame_data_lock_enforcer| {
            let zones_lock_enforcer: ZoneLockEnforcer<'_> = minigame_data_lock_enforcer.into();

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

            let item_defs: BTreeMap<i32, ItemDefinition> = game_server
                .items()
                .iter()
                .map(|(id, config)| (*id, config.to_definition(game_server.abilities())))
                .collect();

            let item_defs_reply = TunneledPacket {
                unknown1: true,
                inner: ItemDefinitionsReply {
                    definitions: &item_defs,
                },
            };
            packets.push(GamePacket::serialize(&item_defs_reply));

            let player = TunneledPacket {
                unknown1: true,
                inner: player_data,
            };
            packets.push(GamePacket::serialize(&player));

            let Some(zone_template) = game_server.read_zone_templates().get(&player_zone_template)
            else {
                return Err(ProcessPacketError::new(
                    ProcessPacketErrorType::ConstraintViolated,
                    format!(
                        "Player {sender} tried to log in at zone template {player_zone_template} that doesn't exist"
                    ),
                ));
            };

            characters_table_write_handle.insert(Character::from_player(
                sender,
                player.inner.data.body_model,
                player.inner.data.pos,
                player.inner.data.rot,
                zone_template.chunk_size,
                instance_guid,
                Player {
                    first_load: true,
                    ready: false,
                    name: player.inner.data.name,
                    squad_guid: None,
                    member: player.inner.data.membership_unknown1,
                    credits: player.inner.data.credits,
                    inventory: PlayerInventory::new(
                        player
                        .inner
                        .data
                        .battle_classes
                        .into_iter()
                        .map(|(battle_class_guid, battle_class)| {
                            (
                                battle_class_guid,
                                BattleClass {
                                    items: battle_class.items.into_iter()
                                        .map(|(slot, item)| (slot, item.guid))
                                        .collect(),
                                },
                            )
                        })
                        .collect(),
                        player.inner.data.active_battle_class,
                        player.inner.data.inventory.into_keys().collect(),
                    ),
                    customizations,
                    minigame_stats: PlayerMinigameStats::default(),
                    minigame_status: None,
                    update_previous_location_on_leave: true,
                    previous_location: PreviousLocation {
                        template_guid: player_zone_template,
                        pos: player.inner.data.pos,
                        rot: player.inner.data.rot,
                    },
                    toggles: Toggles {
                        console: false,
                        free_camera: false,
                        click_to_teleport: false,
                    },
                    role,
                    action_bar: PlayerActionBar {
                        weapon_abilities: make_test_weapon_abilities(),
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

                        // Leaving a minigame can move the player, so remember where they return to first
                        let minigame_return_location = characters_table_write_handle
                            .get(player_guid(sender))
                            .and_then(|character| match &character.read().stats.character_type {
                                CharacterType::Player(player) if player.minigame_status.is_some() => {
                                    Some(player.previous_location.clone())
                                }
                                _ => None,
                            });

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

                        if let Some(sink) = game_server.character_sink() {
                            if let Some(snapshot) = snapshot_player(
                                &character.read().stats,
                                minigame_return_location.as_ref(),
                            ) {
                                sink.save(sender, snapshot);
                            }
                        }

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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::*;
    use crate::backend::{Account, Membership, SavedCharacter, SavedEquippedItem};
    use crate::game_server::handlers::chat_command::process_chat_command;
    use crate::game_server::handlers::zone::{teleport_anywhere, DestinationZoneInstance};
    use crate::persistence::CharacterSink;
    use crate::UtcOffset;

    const GUID: u32 = 4_102_479_591;
    const HOUSE_TEMPLATE: u8 = 100;
    const FELUCIA_TEMPLATE: u8 = 15;
    const MEMBERS_TEMPLATE: u8 = 25;

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<(u32, CharacterSnapshot)>>);

    impl CharacterSink for RecordingSink {
        fn save(&self, guid: u32, snapshot: CharacterSnapshot) {
            self.0.lock().push((guid, snapshot));
        }

        fn recent(&self, _guid: u32) -> Option<CharacterSnapshot> {
            None
        }
    }

    fn game_server() -> (GameServer, Arc<RecordingSink>) {
        let game_server = GameServer::new(Path::new("config"), UtcOffset::default()).unwrap();
        let sink = Arc::new(RecordingSink::default());
        game_server.set_character_sink(sink.clone());
        (game_server, sink)
    }

    fn saved_state() -> CharacterSnapshot {
        CharacterSnapshot {
            zone_template: FELUCIA_TEMPLATE,
            pos: [10.5, 20.25, -30.125, 1.0],
            rot: [0.5, 0.0, 0.0, 0.0],
            credits: 4_321,
            active_battle_class: 1,
            equipment: vec![
                SavedEquippedItem {
                    battle_class: 1,
                    slot: EquipmentSlot::Head.into(),
                    item_guid: 259,
                },
                SavedEquippedItem {
                    battle_class: 1,
                    slot: EquipmentSlot::PrimaryWeapon.into(),
                    item_guid: 2909,
                },
            ],
            customizations: make_test_customizations()
                .into_iter()
                .map(|(slot, customization_id)| SavedCustomization {
                    slot: slot.into(),
                    // A different head than the test player's
                    customization_id: if slot == CustomizationSlot::HeadModel {
                        110001
                    } else {
                        customization_id
                    },
                })
                .collect(),
        }
    }

    fn redeemed(
        role: AccountRole,
        member: bool,
        state: Option<CharacterSnapshot>,
    ) -> Option<Redeemed> {
        Some(Redeemed {
            account: Account {
                role,
                membership: member.then(|| Membership {
                    membership_type: "standard".to_string(),
                    ends_at: None,
                }),
            },
            character: SavedCharacter {
                name: "Rex".to_string(),
                state,
            },
        })
    }

    fn saved(state: Option<CharacterSnapshot>) -> Option<Redeemed> {
        redeemed(AccountRole::Member, true, state)
    }

    fn current_zone_template(game_server: &GameServer) -> u8 {
        game_server
            .lock_enforcer()
            .write_characters(|characters, _| {
                zone_template_guid(
                    characters
                        .get(player_guid(GUID))
                        .unwrap()
                        .read()
                        .stats
                        .instance_guid,
                )
            })
    }

    #[test]
    fn without_a_backend_the_test_player_is_used() {
        let (game_server, _) = game_server();
        let start = starting_state(GUID, None, &game_server);
        assert_eq!(start.player.data.name.first_name, "BLASTER");
        assert_eq!(start.zone_template, DEFAULT_ZONE_TEMPLATE);
        assert_eq!(start.role, Role::Admin);
        assert!(start.player.data.membership_unknown1);
    }

    #[test]
    fn backend_roles_map_onto_oxide_roles() {
        let (game_server, _) = game_server();
        for (role, expected) in [
            (AccountRole::Member, Role::Citizen),
            (AccountRole::Counselor, Role::Counselor),
            (AccountRole::Enforcer, Role::Enforcer),
            (AccountRole::Admin, Role::Admin),
        ] {
            let start = starting_state(GUID, redeemed(role, false, None), &game_server);
            assert_eq!(start.role, expected);
        }
    }

    #[test]
    fn the_membership_sets_the_member_flags() {
        let (game_server, _) = game_server();
        for member in [false, true] {
            let data = starting_state(
                GUID,
                redeemed(AccountRole::Member, member, None),
                &game_server,
            )
            .player
            .data;
            assert_eq!(
                [
                    data.membership_unknown1,
                    data.membership_unknown2,
                    data.membership_unknown3,
                    data.membership_unknown4
                ],
                [member; 4]
            );
        }
    }

    #[test]
    fn only_members_resume_in_the_members_area() {
        let (game_server, _) = game_server();
        let mut state = saved_state();
        state.zone_template = MEMBERS_TEMPLATE;
        for (member, expected) in [(false, DEFAULT_ZONE_TEMPLATE), (true, MEMBERS_TEMPLATE)] {
            let start = starting_state(
                GUID,
                redeemed(AccountRole::Member, member, Some(state.clone())),
                &game_server,
            );
            assert_eq!(start.zone_template, expected);
        }
    }

    #[test]
    fn only_members_can_teleport_into_the_members_area() {
        for member in [false, true] {
            let (game_server, _) = game_server();
            game_server
                .log_in(GUID, redeemed(AccountRole::Member, member, None))
                .unwrap();
            let destination =
                game_server.read_zone_templates()[&MEMBERS_TEMPLATE].default_spawn_pos;
            teleport_anywhere(
                destination,
                Pos::default(),
                DestinationZoneInstance::Any {
                    template_guid: MEMBERS_TEMPLATE,
                },
                GUID,
            )
            .unwrap()(&game_server)
            .unwrap();
            let expected = if member {
                MEMBERS_TEMPLATE
            } else {
                DEFAULT_ZONE_TEMPLATE
            };
            assert_eq!(current_zone_template(&game_server), expected);
        }
    }

    #[test]
    fn only_staff_can_run_chat_commands() {
        for (role, allowed) in [
            (AccountRole::Member, false),
            (AccountRole::Counselor, true),
            (AccountRole::Enforcer, true),
            (AccountRole::Admin, true),
        ] {
            let (game_server, _) = game_server();
            game_server
                .log_in(GUID, redeemed(role, true, None))
                .unwrap();
            let replies = process_chat_command(GUID, &["loc".to_string()], &game_server).unwrap();
            assert_eq!(!replies.is_empty(), allowed, "{role:?}");
        }
    }

    #[test]
    fn a_new_character_gets_its_name_and_the_starting_state() {
        let (game_server, _) = game_server();
        let start = starting_state(GUID, saved(None), &game_server);
        assert_eq!(start.player.data.name.first_name, "Rex");
        assert_eq!(start.player.data.name.last_name, "");
        assert_eq!(start.zone_template, DEFAULT_ZONE_TEMPLATE);
        assert_eq!(start.player.data.credits, 1_000_000);
    }

    #[test]
    fn saved_state_is_applied() {
        let (game_server, _) = game_server();
        let start = starting_state(GUID, saved(Some(saved_state())), &game_server);
        let data = &start.player.data;
        assert_eq!(start.zone_template, FELUCIA_TEMPLATE);
        assert_eq!(pos_to(data.pos), saved_state().pos);
        assert_eq!(pos_to(data.rot), saved_state().rot);
        assert_eq!(data.credits, 4_321);
        let items: Vec<(EquipmentSlot, i32)> = data.battle_classes[&1]
            .items
            .iter()
            .map(|(slot, item)| (*slot, item.guid))
            .collect();
        assert_eq!(
            items,
            vec![
                (EquipmentSlot::Head, 259),
                (EquipmentSlot::PrimaryWeapon, 2909)
            ]
        );
        assert_eq!(start.customizations[&CustomizationSlot::HeadModel], 110001);
        assert_eq!(data.head_model, "Char_HumanMaleHead.adr");
    }

    #[test]
    fn unusable_saved_values_fall_back_to_defaults() {
        let (game_server, _) = game_server();
        let mut state = saved_state();
        state.zone_template = HOUSE_TEMPLATE;
        state.active_battle_class = 999;
        state.equipment.push(SavedEquippedItem {
            battle_class: 1,
            slot: EquipmentSlot::Feet.into(),
            item_guid: -12345,
        });
        state.customizations.push(SavedCustomization {
            slot: 9999,
            customization_id: 1,
        });
        let start = starting_state(GUID, saved(Some(state)), &game_server);
        assert_eq!(start.zone_template, DEFAULT_ZONE_TEMPLATE);
        assert_eq!(start.player.data.active_battle_class, 1);
        assert!(!start.player.data.battle_classes[&1]
            .items
            .contains_key(&EquipmentSlot::Feet));
    }

    #[test]
    fn logging_in_and_out_saves_what_was_loaded() {
        let (game_server, sink) = game_server();
        game_server
            .log_in(GUID, saved(Some(saved_state())))
            .unwrap();

        game_server.save_player(GUID);
        log_out(GUID, &game_server);

        let saves = sink.0.lock();
        assert_eq!(saves.len(), 2);
        for (guid, snapshot) in saves.iter() {
            assert_eq!(*guid, GUID);
            assert_eq!(*snapshot, saved_state());
        }
    }

    #[test]
    fn players_are_not_saved_inside_houses_or_lobbies() {
        let (game_server, sink) = game_server();
        game_server.log_in(GUID, saved(None)).unwrap();
        game_server
            .lock_enforcer()
            .write_characters(|characters, _| {
                let character = characters.get(player_guid(GUID)).unwrap();
                let mut character = character.write();
                let CharacterType::Player(player) = &mut character.stats.character_type else {
                    panic!("not a player");
                };
                player.update_previous_location_on_leave = false;
                player.previous_location = PreviousLocation {
                    template_guid: FELUCIA_TEMPLATE,
                    pos: Pos {
                        x: 1.0,
                        y: 2.0,
                        z: 3.0,
                        w: 1.0,
                    },
                    rot: Pos {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        w: 0.0,
                    },
                };
            });
        game_server.save_player(GUID);
        let saves = sink.0.lock();
        assert_eq!(saves[0].1.zone_template, FELUCIA_TEMPLATE);
        assert_eq!(saves[0].1.pos, [1.0, 2.0, 3.0, 1.0]);
    }
}
