use serde::Deserialize;
use std::collections::HashMap;

use crate::game_server::{
    packets::{
        command::{DialogChoice, EnterDialog, ExitDialog},
        player_update::{QueueAnimation, RemoveTemporaryModel, UpdateTemporaryModel},
        tunnel::TunneledPacket,
        GamePacket, Pos,
    },
    ProcessPacketError, ProcessPacketErrorType,
};

use super::{
    character::{OneShotAction, Player},
    unique_guid::player_guid,
    zone::ZoneInstance,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogEffectsReferenceConfig {
    pub npc_key: String,
    pub animation_id: Option<i32>,
    pub apply_model_id: Option<u32>,
    pub remove_model_id: Option<u32>,
}

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
    pub dialog_message_id: Option<u32>,
    pub speaker_animation_id: Option<i32>,
    pub speaker_sound_id: Option<u32>,
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
    pub synchronized_effects: Option<Vec<DialogEffectsReferenceConfig>>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogChoiceConfig {
    pub button_key: String,
    #[serde(flatten)]
    pub new_dialog: Option<NewDialogConfig>,
    #[serde(default)]
    pub close_dialog: bool,
    #[serde(flatten, default)]
    pub one_shot_action: OneShotAction,
}

pub fn handle_dialog_buttons(
    sender: u32,
    button_id: u32,
    player_stats: &mut Player,
    zone_instance: &ZoneInstance,
) -> Result<Vec<Vec<u8>>, ProcessPacketError> {
    let requester_guid = player_guid(sender);

    let config = zone_instance.dialog_choices
        .iter()
        .find(|choice| choice.button_id == button_id)
        .ok_or_else(|| ProcessPacketError::new(
            ProcessPacketErrorType::ConstraintViolated,
            format!(
                "(Requester: {}) tried to select (Button ID: {}) but it was not found in (Zone Instance: {}) (Template GUID: {})",
                requester_guid, button_id, zone_instance.guid, zone_instance.template_guid,
            ),
        ))?;

    let mut packets = Vec::new();

    if let Some(dialog) = &config.new_dialog {
        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: EnterDialog {
                dialog_message_id: dialog.dialog_message_id.unwrap_or(0),
                speaker_animation_id: dialog.speaker_animation_id.unwrap_or(0),
                speaker_guid: dialog.npc_guid.unwrap_or(0),
                enable_escape: true,
                unknown4: 0.0,
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
                unknown8: 0.0,
                hide_players: !dialog.show_players,
                unknown10: true,
                unknown11: true,
                zoom: dialog.zoom,
                speaker_sound_id: dialog.speaker_sound_id.unwrap_or(0),
            },
        }));

        if let Some(effects) = &dialog.synchronized_effects {
            for effect in effects {
                if let Some(model_id) = effect.apply_model_id {
                    packets.push(GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: UpdateTemporaryModel {
                            model_id,
                            guid: effect.npc_guid,
                        },
                    }));
                }

                if let Some(model_id) = effect.remove_model_id {
                    packets.push(GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: RemoveTemporaryModel {
                            guid: effect.npc_guid,
                            model_id,
                        },
                    }));
                }

                if let Some(animation_id) = effect.animation_id {
                    packets.push(GamePacket::serialize(&TunneledPacket {
                        unknown1: true,
                        inner: QueueAnimation {
                            character_guid: effect.npc_guid,
                            animation_id,
                            queue_pos: 0,
                            delay_seconds: 0.0,
                            duration_seconds: 0.0,
                        },
                    }));
                }
            }
        }
    }

    if config.close_dialog {
        packets.push(GamePacket::serialize(&TunneledPacket {
            unknown1: true,
            inner: ExitDialog {},
        }));
    }

    packets.extend(config.one_shot_action.apply(player_stats)?);

    Ok(packets)
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
    pub dialog_message_id: Option<u32>,
    pub speaker_animation_id: Option<i32>,
    pub speaker_sound_id: Option<u32>,
    pub zoom: f32,
    pub show_players: bool,
    pub choices: Vec<DialogChoiceReference>,
    pub npc_key: Option<String>,
    pub synchronized_effects: Option<Vec<DialogEffectsReferenceConfig>>,
}

#[derive(Clone)]
pub struct DialogChoiceTemplate {
    pub button_id: u32,
    pub new_dialog: Option<DialogTemplate>,
    pub one_shot_action: OneShotAction,
    pub close_dialog: bool,
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
                synchronized_effects: new_dialog.synchronized_effects.clone(),
            }
        });

        DialogChoiceTemplate {
            button_id,
            new_dialog,
            one_shot_action: choice.one_shot_action.clone(),
            close_dialog: choice.close_dialog,
        }
    }
}

pub struct DialogEffectsInstance {
    pub npc_guid: u64,
    pub animation_id: Option<i32>,
    pub apply_model_id: Option<u32>,
    pub remove_model_id: Option<u32>,
}

pub struct DialogInstance {
    pub camera_placement: Pos,
    pub look_at: Pos,
    pub dialog_message_id: Option<u32>,
    pub speaker_animation_id: Option<i32>,
    pub speaker_sound_id: Option<u32>,
    pub zoom: f32,
    pub show_players: bool,
    pub choices: Vec<DialogChoiceReference>,
    pub npc_guid: Option<u64>,
    pub synchronized_effects: Option<Vec<DialogEffectsInstance>>,
}

impl DialogInstance {
    pub fn from_template(
        template: &DialogTemplate,
        character_keys_to_guid: &HashMap<&String, u64>,
    ) -> DialogInstance {
        let npc_guid = template.npc_key.as_ref().map(|key| {
            character_keys_to_guid
                .get(key)
                .copied()
                .unwrap_or_else(|| panic!("Unknown (NPC Key: {}) referenced in dialog", key))
        });

        let synchronized_effects = template.synchronized_effects.as_ref().map(|effects| {
            effects
                .iter()
                .map(|effect| {
                    let npc_guid = character_keys_to_guid
                        .get(&effect.npc_key)
                        .copied()
                        .unwrap_or_else(|| {
                            panic!(
                                "Unknown (NPC Key: {}) referenced in synchronized dialog effects",
                                effect.npc_key
                            )
                        });

                    DialogEffectsInstance {
                        npc_guid,
                        animation_id: effect.animation_id,
                        apply_model_id: effect.apply_model_id,
                        remove_model_id: effect.remove_model_id,
                    }
                })
                .collect()
        });

        DialogInstance {
            camera_placement: template.camera_placement,
            look_at: template.look_at,
            dialog_message_id: template.dialog_message_id,
            speaker_animation_id: template.speaker_animation_id,
            speaker_sound_id: template.speaker_sound_id,
            zoom: template.zoom,
            show_players: template.show_players,
            choices: template.choices.clone(),
            npc_guid,
            synchronized_effects,
        }
    }
}

pub struct DialogChoiceInstance {
    pub button_id: u32,
    pub new_dialog: Option<DialogInstance>,
    pub one_shot_action: OneShotAction,
    pub close_dialog: bool,
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
            one_shot_action: template.one_shot_action.clone(),
            close_dialog: template.close_dialog,
        }
    }
}
