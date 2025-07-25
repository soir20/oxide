use std::collections::BTreeMap;

use packet_serialize::{DeserializePacket, SerializePacket};
use serde::{de::IgnoredAny, Deserialize};

use super::{
    item::{Attachment, BaseAttachmentGroup, ItemDefinition, WieldType},
    Effect, GamePacket, Name, OpCode, Pos, Rgba, Target,
};

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateOpCode {
    AddPc = 0x1,
    AddNpc = 0x2,
    Remove = 0x3,
    Knockback = 0x4,
    UpdateEquippedItem = 0x6,
    SetAnimation = 0x8,
    UpdatePower = 0x9,
    PlayCompositeEffect = 0x10,
    AddNotifications = 0xa,
    NpcRelevance = 0xc,
    UpdateScale = 0xd,
    UpdateTemporaryModel = 0xe,
    RemoveTemporaryModel = 0xf,
    UpdateCharacterState = 0x14,
    QueueAnimation = 0x16,
    UpdateSpeed = 0x17,
    LootEvent = 0x1d,
    ProgressiveHeadScale = 0x1e,
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
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::PlayerUpdate.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PlayerUpdateRemoveOpCode {
    Standard = 0x0,
    Graceful = 0x1,
}

impl SerializePacket for PlayerUpdateRemoveOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        PlayerUpdateOpCode::Remove.serialize(buffer);
        (*self as u16).serialize(buffer);
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
    pub use_death_animation: bool,
    pub delay_millis: u32,
    pub composite_effect_delay_millis: u32,
    pub composite_effect: u32, // Continuous effects remain looping after character removal
    pub fade_duration_millis: u32,
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
    pub guid: u64,
    pub name: Name,
    pub body_model: u32,
    pub chat_text_color: Rgba,
    pub chat_bubble_color: Rgba,
    pub chat_scale: u32,
    pub pos: Pos,
    pub rot: Pos,
    pub attachments: Vec<Attachment>,
    pub head_model: String,
    pub hair_model: String,
    pub hair_color: u32,
    pub eye_color: u32,
    pub unknown7: u32,
    pub skin_tone: String,
    pub face_paint: String,
    pub facial_hair: String,
    pub speed: f32,
    pub underage: bool,
    pub member: bool,
    pub moderator: bool,
    pub temporary_model: u32,
    pub squads: Vec<Unknown13Array>,
    pub battle_class: u32,
    pub title: u32,
    pub unknown16: u32,
    pub unknown17: u32,
    pub effects: Vec<Effect>,
    pub mount_guid: u64,
    pub unknown19: u32,
    pub unknown20: u32,
    pub wield_type: WieldType,
    pub unknown22: f32,
    pub unknown23: u32,
    pub nameplate_image_id: NameplateImage,
}

impl GamePacket for AddPc {
    type Header = PlayerUpdateOpCode;

    const HEADER: Self::Header = PlayerUpdateOpCode::AddPc;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ProgressiveHeadScale {
    pub guid: u64,
    pub scale: f32,
}

impl GamePacket for ProgressiveHeadScale {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ProgressiveHeadScale;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateScale {
    pub guid: u64,
    pub scale: f32,
}

impl GamePacket for UpdateScale {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateScale;
}

#[derive(Copy, Clone, Debug)]
pub enum NameplateImage {
    None = 0,
    Darkside = 6162,
    Lightside = 6163,
    Trooper = 6164,
    Mercenary = 6165,
    Exile = 7021,
    Enforcer = 2087,
}

impl SerializePacket for NameplateImage {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (*self as u32).serialize(buffer);
    }
}

impl NameplateImage {
    pub fn from_battle_class_guid(battle_class_guid: u32) -> Self {
        match battle_class_guid {
            1 => NameplateImage::Trooper,
            2 => NameplateImage::Lightside,
            3 => NameplateImage::Mercenary,
            4 => NameplateImage::Darkside,
            _ => NameplateImage::None,
        }
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
pub struct RemoveTemporaryModel {
    pub guid: u64,
    pub model_id: u32,
}

impl GamePacket for RemoveTemporaryModel {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::RemoveTemporaryModel;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateTemporaryModel {
    pub model_id: u32,
    pub guid: u64,
}

impl GamePacket for UpdateTemporaryModel {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::UpdateTemporaryModel;
}

pub struct ItemDefinitionsReply<'a> {
    pub definitions: &'a BTreeMap<u32, ItemDefinition>,
}

impl SerializePacket for ItemDefinitionsReply<'_> {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let mut inner_buffer = Vec::new();
        self.definitions.serialize(&mut inner_buffer);
        inner_buffer.serialize(buffer);
    }
}

impl GamePacket for ItemDefinitionsReply<'_> {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::ItemDefinitionsReply;
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
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
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (*self as u32).serialize(buffer);
    }
}

#[derive(Clone, Deserialize, SerializePacket)]
#[serde(deny_unknown_fields)]
pub struct Customization {
    #[serde(default)]
    pub comment: IgnoredAny,
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
    pub guid: u64,
    pub rail_id: u32,
    pub elapsed_seconds: f32,
    pub rail_offset: Pos,
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

#[derive(SerializePacket, DeserializePacket)]
pub struct SetAnimation {
    pub character_guid: u64,
    pub animation_id: i32,
    pub animation_group_id: i32,
    pub override_animation: bool,
}

impl GamePacket for SetAnimation {
    type Header = PlayerUpdateOpCode;
    const HEADER: Self::Header = PlayerUpdateOpCode::SetAnimation;
}

#[derive(SerializePacket)]
pub struct UpdateEquippedItem {
    pub guid: u64,
    pub item_guid: u32,
    pub item: Attachment,
    pub battle_class: u32,
    pub wield_type: WieldType,
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
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.guid.serialize(buffer);
        self.notification.is_none().serialize(buffer);
        self.unknown1.serialize(buffer);
        if let Some(notification) = &self.notification {
            notification.serialize(buffer);
        }
        self.unknown2.serialize(buffer);
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
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.guid.serialize(buffer);
        self.cursor.is_some().serialize(buffer);
        if let Some(cursor) = self.cursor {
            cursor.serialize(buffer);
        }
        self.unknown1.serialize(buffer);
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

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub enum Hostility {
    Hostile,
    Neutral,
    Friendly,
}

impl SerializePacket for Hostility {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (*self as u32).serialize(buffer);
    }
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
    fn serialize(&self, buffer: &mut Vec<u8>) {
        (*self as u32).serialize(buffer);
    }
}

#[derive(SerializePacket)]
pub struct AddNpc {
    pub guid: u64,
    pub name_id: u32,
    pub model_id: u32,
    pub unknown3: bool,
    pub chat_text_color: Rgba,
    pub chat_bubble_color: Rgba,
    pub chat_scale: u32,
    pub scale: f32,
    pub pos: Pos,
    pub rot: Pos,
    pub spawn_animation_id: i32,
    pub attachments: Vec<Attachment>,
    pub hostility: Hostility,
    pub unknown10: u32,
    pub texture_alias: String,
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
    pub speed: f32,
    pub unknown21: bool,
    pub interactable_size_pct: u32,
    pub unknown23: i32,
    pub unknown24: i32,
    pub looping_animation_id: i32,
    pub unknown26: bool,
    pub disable_gravity: bool,
    pub sub_title_id: u32,
    pub one_shot_animation_id: i32,
    pub temporary_model: u32,
    pub effects: Vec<Effect>,
    pub disable_interact_popup: bool,
    pub unknown33: u32,
    pub unknown34: bool,
    pub show_health: bool,
    pub hide_despawn_fade: bool,
    pub enable_tilt: bool,
    pub base_attachment_group: BaseAttachmentGroup,
    pub tilt: Pos,
    pub unknown40: u32,
    pub bounce_area_id: i32,
    pub image_set_id: u32,
    pub collision: bool,
    pub rider_guid: u64,
    pub npc_type: u32,
    pub interact_popup_radius: f32,
    pub target: Target,
    pub variables: Vec<Variable>,
    pub rail_id: u32,
    pub rail_elapsed_seconds: f32,
    pub rail_offset: Pos,
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
