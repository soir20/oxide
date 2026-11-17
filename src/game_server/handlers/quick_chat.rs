use std::io::Cursor;

use packet_serialize::DeserializePacket;

use crate::game_server::{
    handlers::{
        character::{Character, CharacterType},
        chat_command::{
            effect_pos, emote_duration_seconds, mind_trick_effect_duration_millis,
            weapon_move_animation_id, EMOTE_EFFECTS, HOLOPROJECTOR_MODELS, MIND_TRICKS,
            PERSISTENT_MIND_TRICKS, WEAPON_MOVES,
        },
        guid::GuidTableIndexer,
        lock_enforcer::CharacterLockRequest,
        unique_guid::player_guid,
        zone::ZoneInstance,
    },
    packets::{
        player_update::{
            AddCompositeEffectTag, PlayCompositeEffect, QueueAnimation, RemoveCompositeEffectTag,
            RemoveTemporaryModel, UpdateTemporaryModel,
        },
        tunnel::TunneledPacket,
        GamePacket, Pos,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

/// Client -> server: the player clicked an entry in the Actions menu
const USE_QUICK_CHAT: u16 = 0x3;

pub fn process_quick_chat_packet(
    cursor: &mut Cursor<&[u8]>,
    sender: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let raw_op_code: u16 = DeserializePacket::deserialize(cursor)?;
    if raw_op_code != USE_QUICK_CHAT {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::UnknownOpCode,
            format!("Unimplemented quick chat op code: {raw_op_code}"),
        ));
    }

    let quick_chat_id: i32 = DeserializePacket::deserialize(cursor)?;
    let Some(quick_chat) = game_server
        .quick_chats()
        .iter()
        .find(|quick_chat| quick_chat.id == quick_chat_id)
    else {
        return Err(ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!("Player {sender} used unknown quick chat {quick_chat_id}"),
        ));
    };
    let animation_id = quick_chat.animation_id;
    let item_guid = quick_chat.item_id;
    let requester = player_guid(sender);

    game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![requester],
            character_consumer: move |characters_table_read_handle, _, mut characters_write, _| {
                let Some((_, instance_guid, chunk)) =
                    characters_table_read_handle.index1(requester)
                else {
                    return Ok(Vec::new());
                };
                let nearby_players = ZoneInstance::all_players_nearby(
                    chunk,
                    instance_guid,
                    characters_table_read_handle,
                );

                let Some(character) = characters_write.get_mut(&requester) else {
                    return Ok(Vec::new());
                };
                let packets =
                    use_quick_chat(character, requester, animation_id, item_guid, game_server);

                if packets.is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(vec![Broadcast::Multi(nearby_players, packets)])
                }
            },
        })
}

fn use_quick_chat(
    character: &mut Character,
    guid: u64,
    animation_id: i32,
    item_guid: i32,
    game_server: &GameServer,
) -> Vec<Vec<u8>> {
    let CharacterType::Player(player) = &character.stats.character_type else {
        return Vec::new();
    };
    if item_guid != 0 && !player.inventory.owns_item(item_guid) {
        return Vec::new();
    }

    let item = item_guid as u32;
    let pos = character.stats.pos;
    let rot = character.stats.rot;

    // One-shot mind trick:
    if let Some(&(_, _, trick_animation_id, offset, effects)) =
        MIND_TRICKS.iter().find(|trick| trick.1 == item)
    {
        let mut packets: Vec<Vec<u8>> = effects
            .iter()
            .map(|&(effect_id, delay_millis)| {
                play_effect(guid, effect_id, delay_millis, effect_pos(pos, rot, offset))
            })
            .collect();
        if let Some(trick_animation_id) = trick_animation_id {
            packets.push(queue_animation(guid, trick_animation_id, 2.0));
        }
        return packets;
    }

    // Persistent mind trick:
    if let Some(&(_, _, effect_id, tag_id)) =
        PERSISTENT_MIND_TRICKS.iter().find(|trick| trick.1 == item)
    {
        let tags = &mut character.stats.composite_effect_tags;
        return if tags.remove(&tag_id).is_some() {
            vec![tunneled(RemoveCompositeEffectTag { guid, tag_id })]
        } else {
            tags.insert(tag_id, effect_id);
            vec![tunneled(AddCompositeEffectTag {
                guid,
                tag_id,
                composite_effect_id: effect_id,
                triggered_by_guid: 0,
                unknown2: 0,
            })]
        };
    }

    // Holoprojector disguise:
    if let Some(&(_, model_id)) = HOLOPROJECTOR_MODELS
        .iter()
        .find(|holoprojector| holoprojector.0 == item)
    {
        let mut packets = Vec::new();
        let previous_model_id = character.stats.temporary_model_id.take();
        if let Some(previous_model_id) = previous_model_id {
            packets.push(tunneled(RemoveTemporaryModel {
                guid,
                model_id: previous_model_id,
            }));
        }
        if previous_model_id != Some(model_id) {
            character.stats.temporary_model_id = Some(model_id);
            packets.push(tunneled(UpdateTemporaryModel { model_id, guid }));
        }
        return packets;
    }

    // Weapon move:
    if let Some(&(_, _, pack, move_index)) = WEAPON_MOVES
        .iter()
        .find(|weapon_move| weapon_move.1 == item)
    {
        let CharacterType::Player(player) = &character.stats.character_type else {
            return Vec::new();
        };
        return weapon_move_animation_id(player, game_server, pack, move_index)
            .map(|animation_id| vec![queue_animation(guid, animation_id, 2.0)])
            .unwrap_or_default();
    }

    // Normal emote, plus an effect for the emotes
    if animation_id == 0 {
        return Vec::new();
    }
    let mut packets = vec![queue_animation(
        guid,
        animation_id,
        emote_duration_seconds(animation_id),
    )];
    if let Some(&(_, effect_id, offset, delay_millis)) = EMOTE_EFFECTS
        .iter()
        .find(|emote_effect| emote_effect.0 == item)
    {
        packets.push(play_effect(
            guid,
            effect_id,
            delay_millis,
            effect_pos(pos, rot, offset),
        ));
    }
    packets
}

fn tunneled<T: GamePacket>(inner: T) -> Vec<u8> {
    GamePacket::serialize(&TunneledPacket {
        unknown1: true,
        inner,
    })
}

fn queue_animation(guid: u64, animation_id: i32, duration_seconds: f32) -> Vec<u8> {
    tunneled(QueueAnimation {
        character_guid: guid,
        animation_id,
        queue_pos: 0,
        delay_seconds: 0.0,
        duration_seconds,
    })
}

fn play_effect(guid: u64, effect_id: u32, delay_millis: u32, pos: Pos) -> Vec<u8> {
    tunneled(PlayCompositeEffect {
        guid,
        triggered_by_guid: guid,
        composite_effect: effect_id,
        delay_millis,
        duration_millis: mind_trick_effect_duration_millis(effect_id),
        pos,
    })
}
