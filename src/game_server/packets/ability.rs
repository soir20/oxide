use byteorder::{LittleEndian, WriteBytesExt};
use packet_serialize::{DeserializePacket, SerializePacket, SerializePacketError};

use super::{GamePacket, OpCode, Pos, Target};

#[derive(Copy, Clone, Debug)]
pub enum AbilityOpCode {
    AbilityFailed = 0x1,
    StartCasting = 0x3,
    LaunchAndLand = 0x4,
    SetDefinition = 0x5,
    ClientMoveAndCast = 0x6,
    PurchaseAbility = 0x7,
    UpdateAbilityExperience = 0x8,
    StopAura = 0x9,
    RequestStartAbility = 0xa,
    MeleeRefresh = 0xb,
    RequestAbilityDefinition = 0xc,
    AbilityDefinition = 0xd,
    DetonateProjectile = 0xe,
    PulseLocationTargeting = 0xf,
    ReceivePulseLocation = 0x10,
}

impl SerializePacket for AbilityOpCode {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<(), SerializePacketError> {
        OpCode::Ability.serialize(buffer)?;
        buffer.write_u16::<LittleEndian>(*self as u16)?;
        Ok(())
    }
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AbilityFailed {
    pub unknown1: u32,
}

impl GamePacket for AbilityFailed {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::AbilityFailed;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StartCasting {
    pub caster_guid: u64,
    pub target_guid: u64,
    pub caster_composite_effect: u32,
    pub caster_animation_id: u32,
    pub ability_id: u32,
    pub unknown6: Pos,
}

impl GamePacket for StartCasting {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::StartCasting;
}

#[derive(SerializePacket)]
pub struct TargetArray {
    pub target: Target,
}

#[derive(SerializePacket)]
pub struct LaunchAndLand {
    pub launcher_guid: u64,
    pub target1: Vec<TargetArray>,
    pub unknown1: i32,
    pub unknown2: u32,
    pub launcher_animation_id: u32,
    pub launcher_composite_effect: u32,
    pub unknown5: u32,
    pub unknown6: bool,
    pub unknown7: bool,
    pub target_animation_unknown1: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: Pos,
    pub launcher_composite_effect_duration: f32,
    pub unknown13: f32,
    pub unknown14: u32,
    pub unknown15: u32,
    pub unknown16: u32,
    pub unknown17: u32,
    pub override_launcher_guid: u64,
    pub track_target: bool,
    pub unknown20: u32,
    pub unknown21: u32,
    pub progressive_start_speed: f32,
    pub progressive_end_speed: f32,
    pub unknown24: u32,
    pub unknown25: u32,
    pub unknown26: Pos,
    pub unknown27: Pos,
    pub projectile_adr_name: String,
    pub projectile_bone_source: Target,
    pub target3: Target,
    pub unknown29: Pos,
    pub unknown30: f32,
    pub unknown31: bool,
    pub projectile_size: f32,
    pub progressive_inflation_size: f32,
    pub trail_composite_effect: u32,
    pub landed_composite_effect: u32,
    pub unknown36: u32,
    pub unknown37: u32,
    pub unknown38: f32,
    pub projectile_duration_seconds: f32,
    pub unknown40: f32,
    pub unknown41: f32,
    pub unknown42: f32,
    pub unknown43: f32,
    pub unknown44: f32,
    pub unknown45: f32,
    pub unknown46: String,
    pub unknown47: u32,
}

impl GamePacket for LaunchAndLand {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::LaunchAndLand;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct ClientMoveAndCast {
    pub unknown1: Pos,
    pub unknown2: u32,
    pub unknown3: u32,
}

impl GamePacket for ClientMoveAndCast {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::ClientMoveAndCast;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UpdateAbilityExperience {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
}

impl GamePacket for UpdateAbilityExperience {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::UpdateAbilityExperience;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct StopAura {
    pub unknown1: u32,
    pub unknown2: u32,
}

impl GamePacket for StopAura {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::StopAura;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct MeleeRefresh {
    pub refresh_millis: u32,
}

impl GamePacket for MeleeRefresh {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::MeleeRefresh;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct UnknownAbilityDefArray {
    pub unknown1: u32,
    pub unknown2: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: bool,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: f32,
    pub unknown11: f32,
    pub unknown12: f32,
    pub unknown13: u32,
    pub unknown14: u32,
    pub unknown15: u32,
    pub unknown16: u32,
    pub unknown17: u32,
    pub unknown18: u32,
    pub unknown19: bool,
    pub unknown20: bool,
    pub unknown21: String,
    pub unknown22: f32,
    pub unknown23: f32,
    pub unknown24: f32,
    pub unknown25: f32,
    pub unknown26: f32,
    pub unknown27: f32,
    pub unknown28: u32,
    pub unknown29: u32,
    pub unknown30: u32,
    pub unknown31: f32,
    pub unknown32: u32,
    pub unknown33: u32,
    pub unknown34: u32,
    pub unknown35: u32,
    pub unknown36: u32,
    pub unknown37: u32,
    pub unknown38: u32,
    pub unknown39: u32,
    pub unknown40: bool,
    pub unknown41: f32,
    pub unknown42: f32,
    pub unknown43: f32,
    pub unknown44: bool,
}

#[derive(SerializePacket, DeserializePacket)]
pub struct AbilityDefinition {
    pub unknown1: u32,
    pub unknown2: bool,
    pub unknown3: bool,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown6: u32,
    pub unknown7: f32,
    pub unknown8: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub unknown13: u32,
    pub unknown14: u32,
    pub unknown15: u32,
    pub unknown16: u32,
    pub unknown17: f32,
    pub unknown18: f32,
    pub unknown19: u32,
    pub unknown20: u32,
    pub unknown21: u32,
    pub unknown22: bool,
    pub unknown23: u32,
    pub unknown24: u32,
    pub unknown25: u32,
    pub unknown26: f32,
    pub unknown27: u32,
    pub unknown28: u32,
    pub unknown29: f32,
    pub unknown30: f32,
    pub unknown31: u32,
    pub unknown32: u32,
    pub unknown33: u32,
    pub unknown34: f32,
    pub unknown35: f32,
    pub unknown36: bool,
    pub unknown37: u32,
    pub unknown38: u32,
    pub unknown39: bool,
    pub unknown40: bool,
    pub unknown41: bool,
    pub unknown42: u32,
    pub unknown43: f32,
    pub unknown44: Vec<UnknownAbilityDefArray>,
}

impl GamePacket for AbilityDefinition {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::AbilityDefinition;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct DetonateProjectile {
    pub guid: u64,
    pub animation_id: u32,
    pub composite_effect: u32,
    pub unknown4: f32,
}

impl GamePacket for DetonateProjectile {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::DetonateProjectile;
}

#[derive(SerializePacket, DeserializePacket)]
pub struct PulseLocationTargeting {
    pub enable_location_targeting: bool,
    pub size: f32,
}

impl GamePacket for PulseLocationTargeting {
    type Header = AbilityOpCode;
    const HEADER: Self::Header = AbilityOpCode::PulseLocationTargeting;
}
