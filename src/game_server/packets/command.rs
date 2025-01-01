use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode, Pos, Rgba, Target};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum CommandOpCode {
    EnterDialog = 0x3,
    ExitDialog = 0x4,
    AdvanceDialog = 0x6,
    InteractionList = 0x9,
    StartFlashGame = 0xc,
    ChatBubbleColor = 0xe,
    SelectPlayer = 0xf,
    DialogEffect = 0x17,
    PlaySoundOnTarget = 0x22,
}

impl SerializePacket for CommandOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Command.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
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
pub struct DialogAdvancement {
    pub button_id: u32,
    pub unknown2: u32,
    pub button_text_id: u32,
    pub unknown4: u32,
    pub unknown5: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct EnterDialog {
    pub dialog_message_id: u32,
    pub unknown2: u32,
    pub guid: u64,
    pub enable_exit_button: bool,
    pub unknown4: f32,
    pub dialog_advancements: Vec<DialogAdvancement>,
    pub camera_placement: Pos,
    pub look_at: Pos,
    pub change_player_pos: bool,
    pub player_pos: Pos,
    pub unknown8: f32,
    pub hide_player: bool,
    pub unknown10: bool,
    pub unknown11: bool,
    pub zoom_scale: f32,
    pub unknown13: u32,
}

impl GamePacket for EnterDialog {
    type Header = CommandOpCode;
    const HEADER: Self::Header = CommandOpCode::EnterDialog;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StartFlashGame {
    pub loader_script_name: String,
    pub game_swf_name: String,
    pub is_micro: bool,
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
pub struct Interaction {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct InteractionList {
    pub guid: u64,
    pub unknown1: bool,
    pub interactions: Vec<Interaction>,
    pub unknown2: String,
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
