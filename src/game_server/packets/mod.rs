pub mod chat;
pub mod client_update;
pub mod combat;
pub mod command;
pub mod housing;
pub mod inventory;
pub mod item;
pub mod login;
pub mod minigame;
pub mod mount;
pub mod player_data;
pub mod player_update;
pub mod purchase;
pub mod reference_data;
pub mod saber_strike;
pub mod squad;
pub mod store;
pub mod time;
pub mod tunnel;
pub mod ui;
pub mod update_position;
pub mod zone;

use std::fmt::Display;

use byteorder::{LittleEndian, WriteBytesExt};
use num_enum::TryFromPrimitive;
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};
use serde::Deserialize;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum OpCode {
    LoginRequest = 0x1,
    LoginReply = 0x2,
    TunneledClient = 0x5,
    TunneledWorld = 0x6,
    Player = 0xc,
    ClientIsDoneLoading = 0xa,
    ClientIsReady = 0xd,
    ZoneDetailsDone = 0xe,
    Chat = 0xf,
    Logout = 0x10,
    Command = 0x1a,
    ClientBeginZoning = 0x1f,
    Combat = 0x20,
    PlayerUpdate = 0x23,
    ClientUpdate = 0x26,
    Minigame = 0x27,
    Inventory = 0x2a,
    ZoneDetails = 0x2b,
    ReferenceData = 0x2c,
    Ui = 0x2f,
    GameTimeSync = 0x34,
    DefinePointsOfInterest = 0x39,
    ZoneCombatSettings = 0x3e,
    Purchase = 0x42,
    QuickChat = 0x43,
    PointOfInterestTeleportRequest = 0x5a,
    WelcomeScreen = 0x5d,
    LobbyGameDefinition = 0x66,
    ClientMetrics = 0x69,
    ClientLog = 0x6d,
    TeleportToSafety = 0x7a,
    UpdatePlayerPosition = 0x7d,
    UpdatePlayerCamera = 0x7e,
    Housing = 0x7f,
    Squad = 0x81,
    UpdatePlayerPlatformPosition = 0xb8,
    ClientGameSettings = 0x8f,
    Portrait = 0x9b,
    PlayerJump = 0xa3,
    Mount = 0xa7,
    Store = 0xa4,
    DeploymentEnv = 0xa5,
    BrandishHolster = 0xb4,
    UiInteractions = 0xbd,
}

impl SerializePacket for OpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

pub trait GamePacket: SerializePacket {
    type Header: SerializePacket;
    const HEADER: Self::Header;

    fn serialize(&self) -> Result<Vec<u8>, SerializePacketError> {
        let mut buffer = Vec::new();
        SerializePacket::serialize(&Self::HEADER, &mut buffer)?;
        SerializePacket::serialize(self, &mut buffer)?;
        Ok(buffer)
    }
}

#[derive(Copy, Clone, SerializePacket, DeserializePacket, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct Name {
    pub first_name_id: u32,
    pub middle_name_id: u32,
    pub last_name_id: u32,
    pub first_name: String,
    pub last_name: String,
}

impl Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let full_name = format!("{} {}", self.first_name, self.last_name);
        f.write_str(full_name.trim())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct Rgba {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba { b, g, r, a }
    }
}

impl From<Rgba> for u32 {
    fn from(val: Rgba) -> Self {
        ((val.a as u32) << 24) | ((val.r as u32) << 16) | ((val.g as u32) << 8) | (val.b as u32)
    }
}

#[derive(Clone, SerializePacket, DeserializePacket)]
pub struct Effect {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: bool,
    pub unknown9: u64,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub composite_effect: u32,
    pub unknown14: u64,
    pub unknown15: u32,
    pub unknown16: u32,
    pub unknown17: bool,
    pub unknown18: bool,
    pub unknown19: bool,
}

#[derive(SerializePacket)]
pub struct GuidTarget {
    pub fallback_pos: Pos,
    pub guid: u64,
}

#[derive(SerializePacket)]
pub struct BoundingBoxTarget {
    pub fallback_pos: Pos,
    pub min_pos: Pos,
    pub max_pos: Pos,
}

#[derive(SerializePacket)]
pub struct CharacterBoneNameTarget {
    pub fallback_pos: Pos,
    pub character_guid: u64,
    pub bone_name: String,
}

#[derive(SerializePacket)]
pub struct CharacterBoneIdTarget {
    pub fallback_pos: Pos,
    pub character_guid: u64,
    pub bone_id: u32,
}

#[derive(SerializePacket)]
pub struct ActorBoneNameTarget {
    pub fallback_pos: Pos,
    pub actor_id: u32,
    pub bone_name: String,
}

#[derive(SerializePacket)]
pub struct ActorBoneIdTarget {
    pub fallback_pos: Pos,
    pub actor_id: u32,
    pub bone_id: u32,
}

#[allow(dead_code)]
#[derive(Default)]
pub enum Target {
    #[default]
    None,
    Guid(GuidTarget),
    BoundingBox(BoundingBoxTarget),
    CharacterBone(CharacterBoneNameTarget),
    CharacterBoneId(CharacterBoneIdTarget),
    ActorBoneName(ActorBoneNameTarget),
    ActorBoneId(ActorBoneIdTarget),
}

impl SerializePacket for Target {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            Target::None => {
                buffer.write_u32::<LittleEndian>(0)?;
            }
            Target::Guid(guid_target) => {
                buffer.write_u32::<LittleEndian>(1)?;
                guid_target.serialize(buffer)?;
            }
            Target::BoundingBox(bounding_box_target) => {
                buffer.write_u32::<LittleEndian>(2)?;
                bounding_box_target.serialize(buffer)?;
            }
            Target::CharacterBone(character_bone_name_target) => {
                buffer.write_u32::<LittleEndian>(3)?;
                character_bone_name_target.serialize(buffer)?;
            }
            Target::CharacterBoneId(character_bone_id_target) => {
                buffer.write_u32::<LittleEndian>(4)?;
                character_bone_id_target.serialize(buffer)?;
            }
            Target::ActorBoneName(actor_bone_name_target) => {
                buffer.write_u32::<LittleEndian>(5)?;
                actor_bone_name_target.serialize(buffer)?;
            }
            Target::ActorBoneId(actor_bone_id_target) => {
                buffer.write_u32::<LittleEndian>(1)?;
                actor_bone_id_target.serialize(buffer)?;
            }
        }

        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct BaseRewardEntry {
    pub unknown1: bool,
    pub icon_set_id: u32,
    pub icon_tint: u32,
    pub unknown4: u32,
    pub quantity: u32,
    pub item_guid: u32,
    pub unknown7: u32,
    pub unknown8: String,
    pub unknown9: u32,
    pub unknown10: bool,
}

pub struct NewItemRewardEntry {
    pub base: BaseRewardEntry,
    pub unknown1: Option<u32>,
}

impl SerializePacket for NewItemRewardEntry {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        self.base.serialize(buffer)?;
        if let Some(value) = self.unknown1 {
            value.serialize(buffer)?;
        }
        Ok(())
    }
}

#[derive(SerializePacket)]
pub struct NewQuestRewardEntry {
    pub base: BaseRewardEntry,
    pub quest_guid: u32,
}

#[derive(SerializePacket)]
pub struct NewBattleClassRewardEntry {
    pub base: BaseRewardEntry,
    pub battle_class_guid: u32,
}

#[derive(SerializePacket)]
pub struct NewAbilityRewardEntry {
    pub base: BaseRewardEntry,
    pub ability_guid: u32,
}

#[derive(SerializePacket)]
pub struct NewCollectionRewardEntry {
    pub base: BaseRewardEntry,
    pub collection_guid: u32,
}

#[derive(SerializePacket)]
pub struct NewCollectionItemRewardEntry {
    pub base: BaseRewardEntry,
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

#[derive(SerializePacket)]
pub struct PetTrickXpRewardEntry {
    pub base: BaseRewardEntry,
    pub unknown1: u32,
    pub unknown2: u32,
}

#[derive(SerializePacket)]
pub struct NewRecipeRewardEntry {
    pub base: BaseRewardEntry,
    pub recipe_guid: u32,
}

#[derive(SerializePacket)]
pub struct ZoneFlagRewardEntry {
    pub base: BaseRewardEntry,
    pub unknown1: String,
    pub unknown2: u32,
    pub unknown3: u32,
}

#[derive(SerializePacket)]
pub struct CharacterFlagRewardEntry {
    pub base: BaseRewardEntry,
    pub unknown1: String,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: bool,
    pub unknown5: u32,
    pub unknown6: bool,
}

#[allow(dead_code)]
pub enum RewardEntry {
    NewItem(NewItemRewardEntry),
    Xp(BaseRewardEntry),
    NewQuest(NewQuestRewardEntry),
    NewBattleClass(NewBattleClassRewardEntry),
    NewAbility(NewAbilityRewardEntry),
    NewCollection(NewCollectionRewardEntry),
    NewCollectionItem(NewCollectionItemRewardEntry),
    Token(BaseRewardEntry),
    PetTrickXp(PetTrickXpRewardEntry),
    NewRecipe(NewRecipeRewardEntry),
    ZoneFlag(ZoneFlagRewardEntry),
    CharacterFlag(CharacterFlagRewardEntry),
    WheelSpin(BaseRewardEntry),
    NewTrophy(BaseRewardEntry),
    ClientExitUrl(BaseRewardEntry),
}

impl SerializePacket for RewardEntry {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        match self {
            RewardEntry::NewItem(item_reward_entry) => {
                buffer.write_u32::<LittleEndian>(1)?;
                item_reward_entry.serialize(buffer)
            }
            RewardEntry::Xp(xp_reward_entry) => {
                buffer.write_u32::<LittleEndian>(3)?;
                xp_reward_entry.serialize(buffer)
            }
            RewardEntry::NewQuest(new_quest_reward_entry) => {
                buffer.write_u32::<LittleEndian>(6)?;
                new_quest_reward_entry.serialize(buffer)
            }
            RewardEntry::NewBattleClass(new_battle_class_reward_entry) => {
                buffer.write_u32::<LittleEndian>(7)?;
                new_battle_class_reward_entry.serialize(buffer)
            }
            RewardEntry::NewAbility(new_ability_reward_entry) => {
                buffer.write_u32::<LittleEndian>(8)?;
                new_ability_reward_entry.serialize(buffer)
            }
            RewardEntry::NewCollection(new_collection_reward_entry) => {
                buffer.write_u32::<LittleEndian>(10)?;
                new_collection_reward_entry.serialize(buffer)
            }
            RewardEntry::NewCollectionItem(new_collection_item_reward_entry) => {
                buffer.write_u32::<LittleEndian>(11)?;
                new_collection_item_reward_entry.serialize(buffer)
            }
            RewardEntry::Token(token_reward_entry) => {
                buffer.write_u32::<LittleEndian>(12)?;
                token_reward_entry.serialize(buffer)
            }
            RewardEntry::PetTrickXp(pet_trick_xp_entry) => {
                buffer.write_u32::<LittleEndian>(13)?;
                pet_trick_xp_entry.serialize(buffer)
            }
            RewardEntry::NewRecipe(new_recipe_entry) => {
                buffer.write_u32::<LittleEndian>(14)?;
                new_recipe_entry.serialize(buffer)
            }
            RewardEntry::ZoneFlag(zone_flag_entry) => {
                buffer.write_u32::<LittleEndian>(15)?;
                zone_flag_entry.serialize(buffer)
            }
            RewardEntry::CharacterFlag(character_flag_entry) => {
                buffer.write_u32::<LittleEndian>(17)?;
                character_flag_entry.serialize(buffer)
            }
            RewardEntry::WheelSpin(wheel_spin_entry) => {
                buffer.write_u32::<LittleEndian>(18)?;
                wheel_spin_entry.serialize(buffer)
            }
            RewardEntry::NewTrophy(new_trophy_entry) => {
                buffer.write_u32::<LittleEndian>(19)?;
                new_trophy_entry.serialize(buffer)
            }
            RewardEntry::ClientExitUrl(client_exit_url_entry) => {
                buffer.write_u32::<LittleEndian>(20)?;
                client_exit_url_entry.serialize(buffer)
            }
        }
    }
}

#[derive(Default, SerializePacket)]
pub struct RewardBundle {
    pub unknown1: bool,
    pub credits: u32,
    pub battle_class_xp: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub icon_set_id: u32,
    pub name_id: u32,
    pub entries: Vec<RewardEntry>,
    pub unknown17: u32,
}
