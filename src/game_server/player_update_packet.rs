use byteorder::{LittleEndian, WriteBytesExt};

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use crate::game_server::game_packet::{Effect, GamePacket, OpCode, Pos, StringId};

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateOpCode {
    AddNpc                          = 0x2,
    Remove                          = 0x3,
    Knockback                       = 0x4,
    UpdatePower                     = 0x9,
    AddNotifications                = 0xa,
    NpcRelevance                    = 0xc,
    UpdateTemporaryAppearance       = 0xe,
    UpdateRemoveTemporaryAppearance = 0xf,
    UpdateCharacterState            = 0x14,
    LootEvent                       = 0x1d,
    SlotCompositeEffectOverride     = 0x1f,
    Freeze                          = 0x20,
    ItemDefinitionsRequest          = 0x22,
    ItemDefinitionsReply            = 0x25,
    SetSpawnerActivationEffect      = 0x2f,
    ReplaceBaseModel                = 0x31,
    SetCollision                    = 0x32,
    MoveOnRail                      = 0x35,
    ClearRail                       = 0x36,
    MoveOnRelativeRail              = 0x37,
    SeekTarget                      = 0x3b,
    SeekTargetUpdate                = 0x3c,
    WieldType                       = 0x3d,
    HudMessage                      = 0x40,
    NameplateImageId                = 0x44,
}

impl SerializePacket for PlayerUpdateOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::PlayerUpdate.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateRemoveOpCode {
    Standard                        = 0x0,
    Graceful                        = 0x1,
}

impl SerializePacket for PlayerUpdateRemoveOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        PlayerUpdateOpCode::Remove.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RemoveStandard {
    pub guid: u64
}

impl GamePacket for RemoveStandard {
    type Header = PlayerUpdateRemoveOpCode;
    const HEADER: Self::Header = PlayerUpdateRemoveOpCode::Standard;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RemoveGracefully {
    pub guid: u64,
    pub unknown1: bool,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub timer: u32,
}

impl GamePacket for RemoveGracefully {
    type Header = PlayerUpdateRemoveOpCode;
    const HEADER: Self::Header = PlayerUpdateRemoveOpCode::Graceful;
}

#[derive(Copy, Clone, Debug)]
pub enum NameplateImage {
    Darkside    = 6162,
    Lightside   = 6163,
    Trooper     = 6164,
    Mercenary   = 6165,
    Exile       = 7021,
    Enforcer    = 2087,
}

impl SerializePacket for NameplateImage {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct NameplateImageId {
    pub image_id: NameplateImage,
    pub guid: u64,
}

impl GamePacket for NameplateImageId {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::NameplateImageId;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdatePower {
    pub guid: u64,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32
}

impl GamePacket for UpdatePower {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdatePower;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct LootEvent {
    guid: u64,
    pos: Pos,
    rot: Pos,
    model_name: String,
}

impl GamePacket for LootEvent {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::LootEvent;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct HudMessage {
    guid: u64,
    unknown_guid: u64,
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: u32,
}

impl GamePacket for HudMessage {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::HudMessage;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SlotCompositeEffectOverride {
    guid: u64,
    slot_id: u32,
    composite_effect: u32,
}

impl GamePacket for SlotCompositeEffectOverride {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::SlotCompositeEffectOverride;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateRemoveTemporaryAppearance {
    guid: u64,
    model_id: u32,
}

impl GamePacket for UpdateRemoveTemporaryAppearance {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateRemoveTemporaryAppearance;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateTemporaryAppearance {
    model_id: u32,
    guid: u64,
}

impl GamePacket for UpdateTemporaryAppearance {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateTemporaryAppearance;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SetSpawnerActivationEffect {
    guid: u64,
    composite_effect: u32,
}

impl GamePacket for SetSpawnerActivationEffect {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::SetSpawnerActivationEffect;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MoveOnRelativeRail {
    guid: u64,
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
    unknown6: Pos,
}

impl GamePacket for MoveOnRelativeRail {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::MoveOnRelativeRail;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ClearRail {
    guid: u64,

}

impl GamePacket for ClearRail {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ClearRail;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MoveOnRail {
    guid: u64,
    unknown1: u32,
    unknown2: u32,
    pos: Pos,

}

impl GamePacket for MoveOnRail {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::MoveOnRail;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SeekTargetUpdate {
    guid: u64,
    target_id: u64,
}

impl GamePacket for SeekTargetUpdate {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::SeekTargetUpdate;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SeekTarget {
    guid: u64,
    target_id: u64,
    init_speed: f32,
    acceleration: f32,
    speed: f32,
    unknown1: f32,
    rot_y: f32,
    rot: Pos,
}

impl GamePacket for SeekTarget {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::SeekTarget;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ReplaceBaseModel {
    pub guid: u64,
    pub model: u32,
    pub composite_effect: u32,
}

impl GamePacket for ReplaceBaseModel {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ReplaceBaseModel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Knockback {
    guid: u64,
    unknown1: u32,
    pos: Pos,
    rot: Pos,
    unknown2: u32,	
}

impl GamePacket for Knockback {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::Knockback;
}

#[derive(Copy, Clone, Debug)]
pub enum Wield  {
    SingleSaber             = 1,
    StaffSaber              = 2,
    ReverseSingleSaber      = 3,
    DualSaber               = 4,
    SinglePistol            = 5,
    Rifle                   = 6,
    SniperRifle             = 7,
    RocketLauncher          = 8,
    FlameThrower            = 9,
    DualPistol              = 10,
    Staff                   = 11,
    Misc                    = 12,
    Bow                     = 13,
    Sparklers               = 14,
    HipBraceLauncherOneShot = 15,
}

impl SerializePacket for Wield {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct WieldType {
    pub guid: u64,
    pub wield_type: Wield,
}

impl GamePacket for WieldType {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::WieldType;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Freeze {
    freeze: bool,
}

impl GamePacket for Freeze {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::Freeze;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateCharacterState {
    pub guid: u64,
    pub bitflags: u32,
}

impl GamePacket for UpdateCharacterState {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateCharacterState;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SetCollision {
    pub guid: u64,
    pub collide: bool
}

impl GamePacket for SetCollision {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::SetCollision;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct NotificationData {
    pub unknown1: u32,
    pub icon_id: u32,
    pub unknown3: u32,
    pub name_id: StringId,
    pub unknown4: u32,
    pub hide_icon: bool,
    pub unknown6: u32,
}

pub struct SingleNotification {
    pub guid: u64,
    pub unknown1: u32,
    pub notification: Option<NotificationData>,
    pub unknown2: bool
}

impl SerializePacket for SingleNotification {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u64::<LittleEndian>(self.guid)?;
        buffer.write_u8(self.notification.is_none() as u8)?;
        buffer.write_u32::<LittleEndian>(self.unknown1)?;
        if let Some(notification) = &self.notification {
            notification.serialize(buffer)?;
        }
        buffer.write_u8(self.unknown2 as u8)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct AddNotifications {
    pub notifications: Vec<SingleNotification>
}

impl GamePacket for AddNotifications {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::AddNotifications;
}

pub struct SingleNpcRelevance {
    pub guid: u64,
    pub cursor: Option<u8>,
    pub unknown1: bool
}

impl SerializePacket for SingleNpcRelevance {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u64::<LittleEndian>(self.guid)?;
        buffer.write_u8(self.cursor.is_some() as u8)?;
        if let Some(cursor) = self.cursor {
            buffer.write_u8(cursor)?;
        }
        buffer.write_u8(self.unknown1 as u8)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct NpcRelevance {
    pub new_states: Vec<SingleNpcRelevance>
}

impl GamePacket for NpcRelevance {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::NpcRelevance;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Attachment {
    pub unknown1: String,
    pub unknown2: String,
    pub unknown3: String,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct BaseAttachmentGroup {
    pub unknown1: u32,
    pub unknown2: String,
    pub unknown3: String,
    pub unknown4: u32,
    pub unknown5: String,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Variable {
    pub unknown1: u32,
    pub unknown2: String,
    pub unknown3: u32,
}

#[derive(Copy, Clone, Debug)]
pub enum Icon {
    None = 0,
    Member = 1,
    Enforcer = 2,
    FancyMember = 3,
}

impl SerializePacket for Icon {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub enum WeaponAnimation {
    None = 0,
    SingleSaber = 1,
    StaffSaber = 2,
    ReverseSingleSaber = 3,
    DoubleSaber = 4,
    SinglePistol = 5,
    Rifle = 6,
    SniperRifle = 7,
    RocketLauncher = 8,
    Flamethrower = 9,
    DoublePistol = 10,
    Staff = 11,
}

impl SerializePacket for WeaponAnimation {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct AddNpc {
    pub guid: u64,
    pub name_id: u32,
    pub model_id: u32,
    pub unknown3: bool,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub scale: f32,
    pub pos: Pos,
    pub rot: Pos,
    pub unknown8: u32,
    pub attachments: Vec<Attachment>,
    pub is_not_targetable: u32,
    pub unknown10: u32,
    pub texture_name: String,
    pub tint_name: String,
    pub tint_id: u32,
    pub unknown11: bool,
    pub offset_y: f32,
    pub composite_effect: u32,
    pub weapon_animation: WeaponAnimation,
    pub name_override: String,
    pub hide_name: bool,
    pub name_offset_x: f32,
    pub name_offset_y: f32,
    pub name_offset_z: f32,
    pub terrain_object_id: u32,
    pub invisible: bool,
    pub unknown20: f32,
    pub unknown21: bool,
    pub interactable_size_pct: u32,
    pub unknown23: i32,
    pub unknown24: i32,
    pub active_animation_slot: i32,
    pub unknown26: bool,
    pub ignore_position: bool,
    pub sub_title_id: StringId,
    pub active_animation_slot2: u32,
    pub head_model_id: u32,
    pub unknown31: Vec<Effect>,
    pub disable_interact_popup: bool,
    pub unknown33: u32,
    pub unknown34: bool,
    pub show_health: bool,
    pub unknown36: bool,
    pub ignore_rotation_and_shadow: bool,
    pub base_attachment_group: BaseAttachmentGroup,
    pub unknown39: Pos,
    pub unknown40: u32,
    pub unknown41: i32,
    pub unknown42: u32,
    pub collision: bool,
    pub unknown44: u64,
    pub npc_type: u32,
    pub unknown46: f32,
    pub target: u32,
    pub unknown50: Vec<Variable>,
    pub rail_id: u32,
    pub rail_speed: f32,
    pub rail_origin: Pos,
    pub unknown54: u32,
    pub rail_unknown1: f32,
    pub rail_unknown2: f32,
    pub rail_unknown3: f32,
    pub attachment_group_unknown: String,
    pub unknown59: String,
    pub unknown60: String,
    pub override_terrain_model: bool,
    pub hover_glow: u32,
    pub hover_description: u32,
    pub fly_over_effect: u32,
    pub unknown65: u32,
    pub unknown66: u32,
    pub unknown67: u32,
    pub disable_move_to_interact: bool,
    pub unknown69: f32,
    pub unknown70: f32,
    pub unknown71: u64,
    pub icon_id: Icon,
}

impl GamePacket for AddNpc {
    type Header = PlayerUpdateOpCode;
    const HEADER: PlayerUpdateOpCode = PlayerUpdateOpCode::AddNpc;
}

pub fn make_test_npc() -> AddNpc {
    AddNpc {
        guid: 102,
        name_id: 0,
        model_id: 458,
        unknown3: false,
        unknown4: 408679,
        unknown5: 13951728,
        unknown6: 1,
        scale: 1.0,
        pos: Pos {
            x: 887.3,
            y: 171.93376,
            z: 1546.956,
            w: 1.0,
        },
        rot: Pos {
            x: 0.0,
            y: 0.0,
            z: 1.0,
            w: 0.0,
        },
        unknown8: 1,
        attachments: vec![],
        is_not_targetable: 1,
        unknown10: 0,
        texture_name: "Rose".to_string(),
        tint_name: "".to_string(),
        tint_id: 0,
        unknown11: true,
        offset_y: 0.0, // Only enabled when unknown45 == 2
        composite_effect: 0,
        weapon_animation: WeaponAnimation::None,
        name_override: "".to_string(),
        hide_name: true,
        name_offset_x: 0.0,
        name_offset_y: 0.0,
        name_offset_z: 0.0,
        terrain_object_id: 0,
        invisible: false,
        unknown20: 0.0,
        unknown21: false,
        interactable_size_pct: 100,
        unknown23: -1,
        unknown24: -1,
        active_animation_slot: -1,
        unknown26: true,
        ignore_position: true,
        sub_title_id: 0,
        active_animation_slot2: 0,
        head_model_id: 0,
        unknown31: vec![],
        disable_interact_popup: false,
        unknown33: 0, // If non-zero, crashes when NPC is clicked on
        unknown34: false,
        show_health: false,
        unknown36: false,
        ignore_rotation_and_shadow: true,
        base_attachment_group: BaseAttachmentGroup {
            unknown1: 0,
            unknown2: "".to_string(),
            unknown3: "".to_string(),
            unknown4: 0,
            unknown5: "".to_string(),
        },
        unknown39: Pos {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        unknown40: 0,
        unknown41: -1,
        unknown42: 0,
        collision: true, // To be interactable, every NPC must have collision set,
                         // even if the model does not actually support collision
        unknown44: 0,
        npc_type: 0,
        unknown46: 0.0,
        target: 0,
        unknown50: vec![],
        rail_id: 0,
        rail_speed: 0.0,
        rail_origin: Pos {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        unknown54: 0,
        rail_unknown1: 0.0,
        rail_unknown2: 0.0,
        rail_unknown3: 0.0,
        attachment_group_unknown: "".to_string(),
        unknown59: "".to_string(),
        unknown60: "".to_string(),
        override_terrain_model: false,
        hover_glow: 0,
        hover_description: 0, // max 7
        fly_over_effect: 0, // max 3
        unknown65: 0, // max 32
        unknown66: 0,
        unknown67: 0,
        disable_move_to_interact: false,
        unknown69: 0.0,
        unknown70: 0.0,
        unknown71: 0,
        icon_id: Icon::None,
    }
}
