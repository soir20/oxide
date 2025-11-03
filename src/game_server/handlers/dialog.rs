use serde::Deserialize;
use std::collections::HashMap;

use crate::game_server::{
    packets::{
        command::{DialogChoice, EnterDialog, ExitDialog},
        tunnel::TunneledPacket,
        ui::{ExecuteScriptWithIntParams, ExecuteScriptWithStringParams},
        GamePacket, Pos,
    },
    Broadcast, GameServer, ProcessPacketError, ProcessPacketErrorType,
};

use super::{
    character::coerce_to_broadcast_supplier,
    lock_enforcer::CharacterLockRequest,
    unique_guid::{player_guid, zone_template_guid},
    zone::{teleport_anywhere, Destination},
    WriteLockingBroadcastSupplier,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogChoiceReferenceConfig {
    pub button_key: String,
    pub button_text_id: u32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogConfig {
    pub camera_placement: Pos,
    pub look_at: Pos,
    #[serde(default)]
    pub dialog_message_id: u32,
    #[serde(default)]
    pub speaker_animation_id: i32,
    #[serde(default)]
    pub speaker_sound_id: u32,
    #[serde(default)]
    pub zoom: f32,
    #[serde(default)]
    pub show_players: bool,
    pub choices: Vec<DialogChoiceReferenceConfig>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewDialogConfig {
    pub npc_key: Option<String>,
    pub new_dialog: DialogConfig,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogChoiceConfig {
    pub button_key: String,
    #[serde(flatten)]
    pub new_dialog: Option<NewDialogConfig>,
    pub script_name: Option<String>,
    #[serde(default)]
    pub close_dialog: bool,
    pub player_destination: Option<Destination>,
    pub minigame_stage_group_guid: Option<i32>,
}

pub fn handle_dialog_buttons(
    sender: u32,
    button_id: u32,
    game_server: &GameServer,
) -> Result<Vec<Broadcast>, ProcessPacketError> {
    let requester_guid = player_guid(sender);

    let broadcast_supplier: WriteLockingBroadcastSupplier = game_server
        .lock_enforcer()
        .read_characters(|_| CharacterLockRequest {
            read_guids: Vec::new(),
            write_guids: vec![requester_guid],
            character_consumer: move |_, _, mut characters_write, _| {
                let Some(player_handle) = characters_write.get_mut(&requester_guid) else {
                    return coerce_to_broadcast_supplier(move |_| Ok(Vec::new()));
                };

                let instance_guid = player_handle.stats.instance_guid;
                let template_guid = zone_template_guid(instance_guid);

                let zone_template = game_server.zone_templates.get(&template_guid).ok_or_else(|| {
                    ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "(Requester: {}) tried to select (Button ID: {}) but (Zone Template: {}) was not found",
                            requester_guid, button_id, template_guid
                        ),
                    )
                })?;

                let config = zone_template.dialog_choices.get(&button_id).ok_or_else(|| {
                    ProcessPacketError::new(
                        ProcessPacketErrorType::ConstraintViolated,
                        format!(
                            "(Requester: {}) tried to select (Button ID: {}) but it was not found in (Zone Template ID: {})",
                            requester_guid, button_id, template_guid
                        ),
                    )
                })?;

                let mut packets = Vec::new();

                if let Some(dialog) = &config.new_dialog {
                    packets.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: EnterDialog {
                                dialog_message_id: dialog.dialog_message_id,
                                speaker_animation_id: dialog.speaker_animation_id,
                                speaker_guid: dialog.npc_guid.unwrap_or(0),
                                enable_escape: true,
                                unknown4: 10.0,
                                dialog_choices: dialog
                                    .choices
                                    .iter()
                                    .map(|choice| DialogChoice {
                                        button_id: choice.button_id,
                                        unknown2: 0,
                                        button_text_id: choice.button_text_id,
                                        unknown4: 0,
                                        unknown5: 0,
                                    })
                                    .collect(),
                                camera_placement: dialog.camera_placement,
                                look_at: dialog.look_at,
                                change_player_pos: false,
                                new_player_pos: Pos::default(),
                                unknown8: 10.0,
                                hide_players: !dialog.show_players,
                                unknown10: true,
                                unknown11: true,
                                zoom: dialog.zoom,
                                speaker_sound_id: dialog.speaker_sound_id,
                            },
                        })],
                    ));
                }

                if config.close_dialog {
                    packets.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: ExitDialog {},
                        })],
                    ));
                }

                if let Some(script_name) = &config.script_name {
                    packets.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: ExecuteScriptWithStringParams {
                                script_name: script_name.clone(),
                                params: vec![],
                            },
                        })],
                    ));
                }

                if let Some(destination) = &config.player_destination {
                    packets.extend((teleport_anywhere(
                        destination.pos,
                        destination.rot,
                        destination.destination_zone,
                        sender,
                    )?)(game_server)?);
                }

                if let Some(minigame_stage_group_guid) = config.minigame_stage_group_guid {
                    packets.push(Broadcast::Single(
                        sender,
                        vec![GamePacket::serialize(&TunneledPacket {
                            unknown1: true,
                            inner: ExecuteScriptWithIntParams {
                                script_name: "MiniGameFlow.CreateMiniGameGroup".to_string(),
                                params: vec![minigame_stage_group_guid],
                            },
                        })],
                    ));
                }

                coerce_to_broadcast_supplier(move |_| Ok(packets))
            },
        });

    broadcast_supplier?(game_server)
}

#[derive(Clone)]
pub struct DialogChoiceReference {
    pub button_id: u32,
    pub button_text_id: u32,
}

#[derive(Clone)]
pub struct DialogTemplate {
    pub camera_placement: Pos,
    pub look_at: Pos,
    pub dialog_message_id: u32,
    pub speaker_animation_id: i32,
    pub speaker_sound_id: u32,
    pub zoom: f32,
    pub show_players: bool,
    pub choices: Vec<DialogChoiceReference>,
    pub npc_key: Option<String>,
}

#[derive(Clone)]
pub struct DialogChoiceTemplate {
    pub button_id: u32,
    pub new_dialog: Option<DialogTemplate>,
    pub script_name: Option<String>,
    pub close_dialog: bool,
    pub player_destination: Option<Destination>,
    pub minigame_stage_group_guid: Option<i32>,
}

impl DialogChoiceTemplate {
    pub fn from_config(
        choice: &DialogChoiceConfig,
        template_guid: u8,
        button_keys_to_id: &HashMap<String, u32>,
    ) -> Self {
        let button_id = *button_keys_to_id
            .get(&choice.button_key)
            .unwrap_or_else(|| {
                panic!(
                    "Unknown (Dialog Button Key: {}) in (Zone Template GUID: {})",
                    choice.button_key, template_guid
                )
            });

        let new_dialog = choice.new_dialog.as_ref().map(|new_dialog| {
            let config = &new_dialog.new_dialog;

            let choices = config
                .choices
                .iter()
                .map(|choice| {
                    let button_id =
                        *button_keys_to_id
                            .get(&choice.button_key)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Unknown (Choice Button Key: {}) in (Dialog Button Key: {})",
                                    choice.button_key, choice.button_key
                                )
                            });

                    DialogChoiceReference {
                        button_id,
                        button_text_id: choice.button_text_id,
                    }
                })
                .collect();

            DialogTemplate {
                camera_placement: config.camera_placement,
                look_at: config.look_at,
                dialog_message_id: config.dialog_message_id,
                speaker_animation_id: config.speaker_animation_id,
                speaker_sound_id: config.speaker_sound_id,
                zoom: config.zoom,
                show_players: config.show_players,
                choices,
                npc_key: new_dialog.npc_key.clone(),
            }
        });

        DialogChoiceTemplate {
            button_id,
            new_dialog,
            script_name: choice.script_name.clone(),
            close_dialog: choice.close_dialog,
            player_destination: choice.player_destination.clone(),
            minigame_stage_group_guid: choice.minigame_stage_group_guid,
        }
    }
}

pub struct DialogInstance {
    pub camera_placement: Pos,
    pub look_at: Pos,
    pub dialog_message_id: u32,
    pub speaker_animation_id: i32,
    pub speaker_sound_id: u32,
    pub zoom: f32,
    pub show_players: bool,
    pub choices: Vec<DialogChoiceReference>,
    pub npc_guid: Option<u64>,
}

impl DialogInstance {
    pub fn from_template(
        template: &DialogTemplate,
        character_keys_to_guid: &HashMap<&String, u64>,
    ) -> DialogInstance {
        DialogInstance {
            camera_placement: template.camera_placement,
            look_at: template.look_at,
            dialog_message_id: template.dialog_message_id,
            speaker_animation_id: template.speaker_animation_id,
            speaker_sound_id: template.speaker_sound_id,
            zoom: template.zoom,
            show_players: template.show_players,
            choices: template.choices.clone(),
            npc_guid: template
                .npc_key
                .as_ref()
                .and_then(|key| character_keys_to_guid.get(key))
                .copied(),
        }
    }
}

pub struct DialogChoiceInstance {
    pub button_id: u32,
    pub new_dialog: Option<DialogInstance>,
    pub script_name: Option<String>,
    pub close_dialog: bool,
    pub player_destination: Option<Destination>,
    pub minigame_stage_group_guid: Option<i32>,
}

impl DialogChoiceInstance {
    pub fn from_template(
        template: &DialogChoiceTemplate,
        character_keys_to_guid: &HashMap<&String, u64>,
    ) -> DialogChoiceInstance {
        DialogChoiceInstance {
            button_id: template.button_id,
            new_dialog: template.new_dialog.as_ref().map(|dialog_template| {
                DialogInstance::from_template(dialog_template, character_keys_to_guid)
            }),
            script_name: template.script_name.clone(),
            close_dialog: template.close_dialog,
            player_destination: template.player_destination,
            minigame_stage_group_guid: template.minigame_stage_group_guid,
        }
    }
}
