use std::{collections::BTreeMap, io::Write};

use byteorder::{LittleEndian, WriteBytesExt};

use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use serde::Deserialize;

use super::{
    item::{Attachment, BaseAttachmentGroup, ItemDefinition, WieldType},
    Effect, GamePacket, OpCode, Pos,
};

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateOpCode {
    AddPc = 0x1,
    AddNpc = 0x2,
    Remove = 0x3,
    Knockback = 0x4,
    UpdateEquippedItem = 0x6,
    UpdatePower = 0x9,
    PlayCompositeEffect = 0x10,
    AddNotifications = 0xa,
    NpcRelevance = 0xc,
    UpdateTemporaryAppearance = 0xe,
    UpdateRemoveTemporaryAppearance = 0xf,
    UpdateCharacterState = 0x14,
    QueueAnimation = 0x16,
    UpdateSpeed = 0x17,
    LootEvent = 0x1d,
    SlotCompositeEffectOverride = 0x1f,
    Freeze = 0x20,
    ItemDefinitionsRequest = 0x22,
    ItemDefinitionsReply = 0x25,
    UpdateCustomizations = 0x27,
    AddCompositeEffectTag = 0x29,
    RemoveCompositeEffectTag = 0x2a,
    SetSpawnerActivationEffect = 0x2f,
    ReplaceBaseModel = 0x31,
    SetCollision = 0x32,
    MoveOnRail = 0x35,
    ClearRail = 0x36,
    MoveOnRelativeRail = 0x37,
    SeekTarget = 0x3b,
    SeekTargetUpdate = 0x3c,
    UpdateWieldType = 0x3d,
    HudMessage = 0x40,
    InitCustomizations = 0x41,
    NameplateImageId = 0x44,
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
    Standard = 0x0,
    Graceful = 0x1,
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
    pub guid: u64,
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

#[derive(SerializePacket, DeserializePacket)]
pub struct Unknown13Array {
    unknown1: u32,
    unknown2: u32,
    unknown3: u32,
}

#[derive(SerializePacket)]
pub struct AddPc {
    guid: u64,
    first_name: u32,
    last_name_prefix: u32,
    last_name_suffix: u32,
    first_name_override: String,
    last_name_override: String,
    body_model: u32,
    chat_foreground: u32,
    chat_background: u32,
    chat_scale: u32,
    pos: Pos,
    rot: Pos,
    attachments: Vec<Attachment>,
    head_model: String,
    hair_model: String,
    hair_color: u32,
    eye_color: u32,
    unknown7: u32,
    skin_tone: String,
    face_paint: String,
    facial_hair: String,
    speed: f32,
    underage: bool,
    membership: bool,
    moderator: bool,
    temporary_appearance: u32,
    guilds: Vec<Unknown13Array>,
    battle_class: u32,
    title: u32,
    unknown16: u32,
    unknown17: u32,
    effects: Vec<Effect>,
    mount_guid: u64,
    unknown19: u32,
    unknown20: u32,
    wield_type: WieldType,
    unknown22: f32,
    unknown23: u32,
    nameplate_image_id: u32,
}

impl GamePacket for AddPc {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::AddPc;
}

#[derive(Copy, Clone, Debug)]
pub enum NameplateImage {
    Darkside = 6162,
    Lightside = 6163,
    Trooper = 6164,
    Mercenary = 6165,
    Exile = 7021,
    Enforcer = 2087,
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
    pub unknown3: u32,
}

impl GamePacket for UpdatePower {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdatePower;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PlayCompositeEffect {
    pub guid: u64,
    pub triggered_by_guid: u64,
    pub composite_effect: u32,
    pub delay_millis: u32,
    pub duration_millis: u32,
    pub pos: Pos,
}

impl GamePacket for PlayCompositeEffect {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::PlayCompositeEffect;
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
    unknown1: u64,
    unknown2: u64,
    name_id: u32,
    image_id: u32,
    message_id: u32,
    unknown3: u32,
    unknown4: u32,
    unknown5: u32,
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

pub struct ItemDefinitionsReply<'a> {
    pub definitions: &'a BTreeMap<u32, ItemDefinition>,
}

impl SerializePacket for ItemDefinitionsReply<'_> {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        let mut inner_buffer = Vec::new();
        self.definitions.serialize(&mut inner_buffer)?;
        buffer.write_u32::<LittleEndian>(inner_buffer.len() as u32)?;
        buffer.write_all(&inner_buffer)?;
        Ok(())
    }
}

impl GamePacket for ItemDefinitionsReply<'_> {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ItemDefinitionsReply;
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CustomizationSlot {
    None = -1,
    HeadModel = 0,
    SkinTone = 1,
    HairStyle = 2,
    HairColor = 3,
    EyeColor = 4,
    FacialHair = 5,
    FacePattern = 6,
    BodyModel = 8,
}

impl SerializePacket for CustomizationSlot {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u32::<LittleEndian>(*self as u32)?;
        Ok(())
    }
}

#[derive(Clone, Deserialize, SerializePacket)]
pub struct Customization {
    pub customization_slot: CustomizationSlot,
    pub customization_param1: String,
    pub customization_param2: u32,
    pub guid: u32,
}

#[derive(SerializePacket)]
pub struct UpdateCustomizations {
    pub guid: u64,
    pub is_preview: bool,
    pub customizations: Vec<Customization>,
}

impl GamePacket for UpdateCustomizations {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateCustomizations;
}

#[derive(SerializePacket)]
pub struct InitCustomizations {
    pub customizations: Vec<Customization>,
}

impl GamePacket for InitCustomizations {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::InitCustomizations;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AddCompositeEffectTag {
    pub guid: u64,
    pub tag_id: u32,
    pub composite_effect: u32,
    pub triggered_by_guid: u64,
    pub unknown2: u64,
}

impl GamePacket for AddCompositeEffectTag {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::AddCompositeEffectTag;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RemoveCompositeEffectTag {
    pub guid: u64,
    pub tag_id: u32,
}

impl GamePacket for RemoveCompositeEffectTag {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::RemoveCompositeEffectTag;
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

#[derive(SerializePacket)]
pub struct UpdateEquippedItem {
    pub guid: u64,
    pub unknown: u32,
    pub item: Attachment,
    pub battle_class: u32,
    pub wield_type: u32,
}

impl GamePacket for UpdateEquippedItem {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateEquippedItem;
}

#[derive(SerializePacket)]
pub struct UpdateWieldType {
    pub guid: u64,
    pub wield_type: WieldType,
}

impl GamePacket for UpdateWieldType {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateWieldType;
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
pub struct QueueAnimation {
    pub character_guid: u64,
    pub animation_id: i32,
    pub queue_pos: u32,
    pub delay_seconds: f32,
    pub duration_seconds: f32,
}

impl GamePacket for QueueAnimation {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::QueueAnimation;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateSpeed {
    pub guid: u64,
    pub speed: f32,
}

impl GamePacket for UpdateSpeed {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateSpeed;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct SetCollision {
    pub guid: u64,
    pub collide: bool,
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
    pub name_id: u32,
    pub unknown4: u32,
    pub hide_icon: bool,
    pub unknown6: u32,
}

pub struct SingleNotification {
    pub guid: u64,
    pub unknown1: u32,
    pub notification: Option<NotificationData>,
    pub unknown2: bool,
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
    pub notifications: Vec<SingleNotification>,
}

impl GamePacket for AddNotifications {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::AddNotifications;
}

pub struct SingleNpcRelevance {
    pub guid: u64,
    pub cursor: Option<u8>,
    pub unknown1: bool,
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
    pub new_states: Vec<SingleNpcRelevance>,
}

impl GamePacket for NpcRelevance {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::NpcRelevance;
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
    pub wield_type: WieldType,
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
    pub sub_title_id: u32,
    pub active_animation_slot2: u32,
    pub head_model_id: u32,
    pub effects: Vec<Effect>,
    pub disable_interact_popup: bool,
    pub unknown33: u32,
    pub unknown34: bool,
    pub show_health: bool,
    pub hide_despawn_fade: bool,
    pub disable_rotation_and_shadow: bool,
    pub base_attachment_group: BaseAttachmentGroup,
    pub unknown39: Pos,
    pub unknown40: u32,
    pub bounce_area_id: i32,
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
    pub pet_customization_model_name1: String,
    pub pet_customization_model_name2: String,
    pub pet_customization_model_name3: String,
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
