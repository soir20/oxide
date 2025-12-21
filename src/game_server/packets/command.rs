use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket};

use super::{GamePacket, OpCode, Pos, Rgba, Target};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum CommandOpCode {
    EnterDialog = 0x3,
    ExitDialog = 0x4,
    AdvanceDialog = 0x6,
    InteractRequest = 0x8,
    InteractionList = 0x9,
    StartFlashGame = 0xc,
    ChatBubbleColor = 0xe,
    SelectPlayer = 0xf,
    FreeInteractNpc = 0x10,
    MoveToInteract = 0x11,
    DialogEffect = 0x17,
    PlaySoundOnTarget = 0x22,
}

impl SerializePacket for CommandOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Command.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InteractRequest {
    pub target: u64,
}

impl GamePacket for InteractRequest {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::InteractRequest;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MoveToInteract {
    pub destination: Pos,
    pub target: u64,
}

impl GamePacket for MoveToInteract {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::MoveToInteract;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DialogEffect {
    pub guid: u64,
    pub composite_effect: u32,
}

impl GamePacket for DialogEffect {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::DialogEffect;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ExitDialog {}

impl GamePacket for ExitDialog {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::ExitDialog;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AdvanceDialog {
    pub button_id: u32,
}

impl GamePacket for AdvanceDialog {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::AdvanceDialog;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DialogChoice {
    pub button_id: u32,
    pub unknown2: u32,
    pub button_text_id: u32,
    pub unknown4: u32,
    pub unknown5: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EnterDialog {
    pub dialog_message_id: u32,
    pub speaker_animation_id: i32,
    pub speaker_guid: u64,
    pub enable_escape: bool,
    pub unknown4: f32,
    pub dialog_choices: Vec<DialogChoice>,
    pub camera_placement: Pos,
    pub look_at: Pos,
    pub change_player_pos: bool,
    pub new_player_pos: Pos,
    pub unknown8: f32,
    pub hide_players: bool,
    pub unknown10: bool,
    pub unknown11: bool,
    pub zoom: f32,
    pub speaker_sound_id: u32,
}

impl GamePacket for EnterDialog {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::EnterDialog;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StartFlashGame {
    pub loader_script_name: String,
    pub game_swf_name: String,
    pub return_to_portal: bool,
}

impl GamePacket for StartFlashGame {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::StartFlashGame;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ChatBubbleColor {
    text_color: Rgba,
    bubble_color: Rgba,
    size: u32,
    guid: u64,
}

impl GamePacket for ChatBubbleColor {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::ChatBubbleColor;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InteractionButton {
    pub event_id: u32,
    pub icon_id: u32,
    pub label_id: u32,
    pub interaction_type: u32,
    pub tooltip_id: u32,
    pub param1: u32,
    pub param2: u32,
    pub param3: u32,
    pub sort_order: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InteractionList {
    pub guid: u64,
    pub auto_select_if_single_button: bool,
    pub buttons: Vec<InteractionButton>,
    pub context_name: String,
    pub unknown3: bool,
    pub unknown4: bool,
}

impl GamePacket for InteractionList {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::InteractionList;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SelectPlayer {
    pub requester: u64,
    pub target: u64,
}

impl GamePacket for SelectPlayer {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::SelectPlayer;
}

#[derive(SerializePacket)]
pub struct PlaySoundIdOnTarget {
    pub sound_id: u32,
    pub target: Target,
}

impl GamePacket for PlaySoundIdOnTarget {
    type Header = CommandOpCode;

    const HEADER: Self::Header = CommandOpCode::PlaySoundOnTarget;
}
