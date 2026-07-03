use std::io::Cursor;

use num_enum::TryFromPrimitive;

use packet_serialize::{DeserializePacket, DeserializePacketError, SerializePacket};

use super::{ActionBarType, GamePacket, OpCode, Pos, Target};

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum AbilityOpCode {
    LaunchAndLand = 0x4,
    RequestStartCast = 0xa,
    RequestDefinition = 0xc,
    DetonateProjectile = 0xe,
    ToggleLocationTargeting = 0xf,
    ReceiveTargetedLocation = 0x10,
}

impl SerializePacket for AbilityOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        OpCode::Ability.serialize(buffer);
        (*self as u16).serialize(buffer);
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct GuidAbilityTarget {
    pub target_guid: u64,
    pub target_guid2: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AoeAbilityTarget {
    pub pos: Pos,
    pub guid: u64,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct WithSelfAbilityTarget {
    pub guid: u64,
    pub target_guid: u64,
}

#[allow(dead_code)]
pub enum AbilityTargetType {
    Guid(GuidAbilityTarget),
    Aoe(AoeAbilityTarget),
    WithSelf(WithSelfAbilityTarget),
}

impl SerializePacket for AbilityTargetType {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        match self {
            AbilityTargetType::Guid(guid_target) => {
                0u32.serialize(buffer);
                guid_target.serialize(buffer);
            }
            AbilityTargetType::Aoe(aoe_target) => {
                1u32.serialize(buffer);
                aoe_target.serialize(buffer);
            }
            AbilityTargetType::WithSelf(with_self_target) => {
                2u32.serialize(buffer);
                with_self_target.serialize(buffer);
            }
        }
    }
}

impl DeserializePacket for AbilityTargetType {
    fn deserialize(cursor: &mut Cursor<&[u8]>) -> Result<Self, DeserializePacketError> {
        let raw_target_type: u32 = DeserializePacket::deserialize(cursor)?;

        match raw_target_type {
            0 => Ok(AbilityTargetType::Guid(GuidAbilityTarget::deserialize(
                cursor,
            )?)),
            1 => Ok(AbilityTargetType::Aoe(AoeAbilityTarget::deserialize(
                cursor,
            )?)),
            2 => Ok(AbilityTargetType::WithSelf(
                WithSelfAbilityTarget::deserialize(cursor)?,
            )),
            _ => Err(DeserializePacketError::UnknownDiscriminator),
        }
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct RequestStartCast {
    pub action_bar_type: ActionBarType,
    pub slot_index: u32,
    pub target: AbilityTargetType,
}

impl GamePacket for RequestStartCast {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::RequestStartCast;
}

#[derive(SerializePacket)]
pub struct LaunchAndLand {
    pub launcher_guid: u64,
    pub targets: Vec<Target>,
    pub unknown1: i32,
    pub unknown2: u32,
    pub launcher_animation_id: u32,
    pub launcher_composite_effect_id: u32,
    pub slot_cooldown_millis: u32,
    pub disable_slot_cooldown: bool,
    pub unknown7: bool,
    pub landed_animation_id: u32,
    pub landed_composite_effect_id1: u32,
    pub unknown10: u32,
    pub unknown11: Pos,
    pub launcher_composite_effect_duration: f32,
    pub unknown13: f32,
    pub unknown14: u32,
    pub action_bar_type: ActionBarType,
    pub slot_index: i32,
    pub unknown17: u32,
    pub override_launcher_guid: u64,
    pub unknown19: bool,
    pub unknown20: u32,
    pub unknown21: u32,
    pub progressive_start_speed: f32,
    pub progressive_end_speed: f32,
    pub unknown24: u32,
    pub unknown25: u32,
    pub unknown26: Pos,
    pub unknown27: Pos,
    pub projectile_adr_name: String,
    pub projectile_origin: Target,
    pub unknown_target: Target,
    pub unknown29: Pos,
    pub projectile_angular_speed: f32,
    pub unknown31: bool,
    pub projectile_size: f32,
    pub progressive_inflation_size: f32,
    pub trail_composite_effect_id: u32,
    pub landed_composite_effect_id2: u32,
    pub unknown36: u32,
    pub unknown37: u32,
    pub unknown38: f32,
    pub unknown39: f32,
    pub unknown40: f32,
    pub unknown41: f32,
    pub unknown42: f32,
    pub unknown43: f32,
    pub unknown44: f32,
    pub missfire_travel_units: f32,
    pub unknown46: String,
    pub unknown47: u32,
}

impl GamePacket for LaunchAndLand {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::LaunchAndLand;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DetonateProjectile {
    pub guid: u64,
    pub animation_id: u32,
    pub composite_effect_id: u32,
    pub unknown4: f32,
}

impl GamePacket for DetonateProjectile {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::DetonateProjectile;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ToggleLocationTargeting {
    pub enabled: bool,
    pub target_size: f32,
}

impl GamePacket for ToggleLocationTargeting {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::ToggleLocationTargeting;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ReceiveTargetedLocation {
    pub pos: Pos,
}

impl GamePacket for ReceiveTargetedLocation {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::ReceiveTargetedLocation;
}
