use num_enum::TryFromPrimitive;
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};

use crate::{
    deserialize, deserialize_exact, deserialize_string, i32_to_usize, is_eof, tell, usize_to_i32,
    DeserializeAsset, Error, ErrorKind,
};

async fn deserialize_len_with_bytes_read(
    file: &mut BufReader<&mut File>,
) -> Result<(i32, i32), Error> {
    let len_marker = deserialize(file, BufReader::read_u8).await?;
    let mut len: i32 = len_marker.into();
    let mut bytes_read = 1;
    if len_marker >= 128 {
        if len_marker == 0xff {
            len = deserialize(file, BufReader::read_i32).await?;
            bytes_read += 4;
        } else {
            let len_byte2 = deserialize(file, BufReader::read_u8).await?;
            len = ((i32::from(len_marker) & 0b0111_1111) << 8) | i32::from(len_byte2);
            bytes_read += 1;
        }
    }

    Ok((len, bytes_read))
}

trait DeserializeEntryType: Sized {
    fn deserialize(
        file: &mut BufReader<&mut File>,
    ) -> impl std::future::Future<Output = Result<(Self, i32), Error>> + Send;
}

impl<T: TryFromPrimitive<Primitive = u8>> DeserializeEntryType for T {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<(Self, i32), Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        let entry_type = Self::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into(), T::NAME),
            offset,
        })?;

        Ok((entry_type, 1))
    }
}

trait DeserializeEntryData<T>: Sized {
    fn deserialize(
        entry_type: &T,
        entry_len: i32,
        file: &mut BufReader<&mut File>,
    ) -> impl std::future::Future<Output = Result<(Self, i32), Error>> + Send;
}

trait DeserializeEntry<T, D>: Sized {
    fn deserialize(
        file: &mut BufReader<&mut File>,
    ) -> impl std::future::Future<Output = Result<(Self, i32), Error>> + Send;
}

pub struct Entry<T, D> {
    pub entry_type: T,
    pub data: D,
}

impl<T: DeserializeEntryType + Send, D: DeserializeEntryData<T> + Send> DeserializeEntry<T, D>
    for Entry<T, D>
{
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<(Self, i32), Error> {
        let (entry_type, type_bytes_read) = T::deserialize(file).await?;
        let (len, len_bytes_read) = deserialize_len_with_bytes_read(file).await?;
        let (data, data_bytes_read) = D::deserialize(&entry_type, len, file).await?;

        let total_bytes_read = type_bytes_read
            .saturating_add(len_bytes_read)
            .saturating_add(data_bytes_read);

        Ok((Entry { entry_type, data }, total_bytes_read))
    }
}

async fn deserialize_entries<T, D, E: DeserializeEntry<T, D>>(
    file: &mut BufReader<&mut File>,
    len: i32,
) -> Result<(Vec<E>, i32), Error> {
    let mut entries = Vec::new();
    let mut bytes_read = 0;
    while bytes_read < len {
        let (entry, entry_bytes_read) = E::deserialize(file).await?;
        bytes_read = bytes_read.saturating_add(entry_bytes_read);
        entries.push(entry);
    }

    Ok((entries, bytes_read))
}

async fn deserialize_f32_be(
    file: &mut BufReader<&mut File>,
    len: i32,
) -> Result<(f32, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    data.resize(4, 0);
    Ok((
        f32::from_be_bytes(data.try_into().expect("data should contain 4 bytes")),
        usize_to_i32(bytes_read)?,
    ))
}

async fn deserialize_u8(file: &mut BufReader<&mut File>, len: i32) -> Result<(u8, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    data.resize(1, 0);
    Ok((data[0], usize_to_i32(bytes_read)?))
}

async fn deserialize_u16_le(
    file: &mut BufReader<&mut File>,
    len: i32,
) -> Result<(u16, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    data.resize(2, 0);
    Ok((
        u16::from_le_bytes(data.try_into().expect("data should contain 2 bytes")),
        usize_to_i32(bytes_read)?,
    ))
}

async fn deserialize_u32_le(
    file: &mut BufReader<&mut File>,
    len: i32,
) -> Result<(u32, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    data.resize(4, 0);
    Ok((
        u32::from_le_bytes(data.try_into().expect("data should contain 4 bytes")),
        usize_to_i32(bytes_read)?,
    ))
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SkeletonEntryType {
    AssetName = 0x1,
    Scale = 0x2,
}

pub enum SkeletonData {
    AssetName { name: String },
    Scale { scale: f32 },
}

impl DeserializeEntryData<SkeletonEntryType> for SkeletonData {
    async fn deserialize(
        entry_type: &SkeletonEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SkeletonEntryType::AssetName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((SkeletonData::AssetName { name }, usize_to_i32(bytes_read)?))
            }
            SkeletonEntryType::Scale => {
                let (scale, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SkeletonData::Scale { scale }, bytes_read))
            }
        }
    }
}

pub type SkeletonEntry = Entry<SkeletonEntryType, SkeletonData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ModelEntryType {
    ModelAssetName = 0x1,
    MaterialAssetName = 0x2,
    Radius = 0x3,
    Unknown1 = 0x4,
    ObjectTerrainData = 0x5,
}

pub enum ModelData {
    ModelAssetName { name: String },
    MaterialAssetName { name: String },
    Radius { radius: f32 },
    Unknown1 { data: Vec<u8> },
    ObjectTerrainData { object_terrain_data_id: u8 },
}

impl DeserializeEntryData<ModelEntryType> for ModelData {
    async fn deserialize(
        entry_type: &ModelEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ModelEntryType::ModelAssetName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ModelData::ModelAssetName { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ModelEntryType::MaterialAssetName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ModelData::MaterialAssetName { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ModelEntryType::Radius => {
                let (radius, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ModelData::Radius { radius }, bytes_read))
            }
            ModelEntryType::Unknown1 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((ModelData::Unknown1 { data }, usize_to_i32(bytes_read)?))
            }
            ModelEntryType::ObjectTerrainData => {
                let (object_terrain_data_id, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    ModelData::ObjectTerrainData {
                        object_terrain_data_id,
                    },
                    bytes_read,
                ))
            }
        }
    }
}

pub type ModelEntry = Entry<ModelEntryType, ModelData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEmitterEntryType {
    EffectId = 0x1,
    EmitterName = 0x2,
    BoneName = 0x3,
    Heading = 0x4,
    Pitch = 0x5,
    Scale = 0x6,
    OffsetX = 0x7,
    OffsetY = 0x8,
    OffsetZ = 0x9,
    EffectAssetName = 0xa,
    SourceBoneName = 0xb,
    LocalSpaceDerived = 0xc,
    WorldOrientation = 0xd,
    HardStop = 0xe,
}

pub enum ParticleEmitterEntryData {
    EffectId { effect_id: u16 },
    EmitterName { emitter_name: String },
    BoneName { bone_name: String },
    Heading { heading: f32 },
    Pitch { pitch: f32 },
    Scale { scale: f32 },
    OffsetX { offset_x: f32 },
    OffsetY { offset_y: f32 },
    OffsetZ { offset_z: f32 },
    EffectAssetName { asset_name: String },
    SourceBoneName { bone_name: String },
    LocalSpaceDerived { is_local_space_derived: bool },
    WorldOrientation { use_world_orientation: bool },
    HardStop { is_hard_stop: bool },
}

impl DeserializeEntryData<ParticleEmitterEntryType> for ParticleEmitterEntryData {
    async fn deserialize(
        entry_type: &ParticleEmitterEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleEmitterEntryType::EffectId => {
                let (effect_id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((ParticleEmitterEntryData::EffectId { effect_id }, bytes_read))
            }
            ParticleEmitterEntryType::EmitterName => {
                let (emitter_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterEntryData::EmitterName { emitter_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ParticleEmitterEntryType::BoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterEntryData::BoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ParticleEmitterEntryType::Heading => {
                let (heading, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ParticleEmitterEntryData::Heading { heading }, bytes_read))
            }
            ParticleEmitterEntryType::Pitch => {
                let (pitch, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ParticleEmitterEntryData::Pitch { pitch }, bytes_read))
            }
            ParticleEmitterEntryType::Scale => {
                let (scale, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ParticleEmitterEntryData::Scale { scale }, bytes_read))
            }
            ParticleEmitterEntryType::OffsetX => {
                let (offset_x, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ParticleEmitterEntryData::OffsetX { offset_x }, bytes_read))
            }
            ParticleEmitterEntryType::OffsetY => {
                let (offset_y, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ParticleEmitterEntryData::OffsetY { offset_y }, bytes_read))
            }
            ParticleEmitterEntryType::OffsetZ => {
                let (offset_z, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ParticleEmitterEntryData::OffsetZ { offset_z }, bytes_read))
            }
            ParticleEmitterEntryType::EffectAssetName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterEntryData::EffectAssetName { asset_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ParticleEmitterEntryType::SourceBoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterEntryData::SourceBoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ParticleEmitterEntryType::LocalSpaceDerived => {
                let (is_local_space_derived, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    ParticleEmitterEntryData::LocalSpaceDerived {
                        is_local_space_derived: is_local_space_derived != 0,
                    },
                    bytes_read,
                ))
            }
            ParticleEmitterEntryType::WorldOrientation => {
                let (use_world_orientation, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    ParticleEmitterEntryData::WorldOrientation {
                        use_world_orientation: use_world_orientation != 0,
                    },
                    bytes_read,
                ))
            }
            ParticleEmitterEntryType::HardStop => {
                let (is_hard_stop, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    ParticleEmitterEntryData::HardStop {
                        is_hard_stop: is_hard_stop != 0,
                    },
                    bytes_read,
                ))
            }
        }
    }
}

pub type ParticleEmitterEntry = Entry<ParticleEmitterEntryType, ParticleEmitterEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEmitterType {
    ParticleEmitter = 0x1,
    Unknown = 0xfe,
}

pub enum ParticleEmitterData {
    ParticleEmitter { entries: Vec<ParticleEmitterEntry> },
    Unknown { data: Vec<u8> },
}

impl DeserializeEntryData<ParticleEmitterType> for ParticleEmitterData {
    async fn deserialize(
        entry_type: &ParticleEmitterType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleEmitterType::ParticleEmitter => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((ParticleEmitterData::ParticleEmitter { entries }, bytes_read))
            }
            ParticleEmitterType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterData::Unknown { data },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type ParticleEmitter = Entry<ParticleEmitterType, ParticleEmitterData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEmitterArrayType {
    Unknown = 0x1,
    ParticleEmitter = 0x2,
}

pub enum ParticleEmitterArrayData {
    Unknown { data: Vec<u8> },
    ParticleEmitter { entries: Vec<ParticleEmitter> },
}

impl DeserializeEntryData<ParticleEmitterArrayType> for ParticleEmitterArrayData {
    async fn deserialize(
        entry_type: &ParticleEmitterArrayType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleEmitterArrayType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterArrayData::Unknown { data },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ParticleEmitterArrayType::ParticleEmitter => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    ParticleEmitterArrayData::ParticleEmitter { entries },
                    bytes_read,
                ))
            }
        }
    }
}

pub type ParticleEmitterArray = Entry<ParticleEmitterArrayType, ParticleEmitterArrayData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MaterialEntryType {
    Name = 0x1,
    SemanticHash = 0x2,
    UnknownHash = 0x3,
}

pub enum MaterialEntryData {
    Name { name: String },
    SemanticHash { hash: u32 },
    UnknownHash { hash: u32 },
}

impl DeserializeEntryData<MaterialEntryType> for MaterialEntryData {
    async fn deserialize(
        entry_type: &MaterialEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MaterialEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((MaterialEntryData::Name { name }, usize_to_i32(bytes_read)?))
            }
            MaterialEntryType::SemanticHash => {
                let (hash, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MaterialEntryData::SemanticHash { hash }, bytes_read))
            }
            MaterialEntryType::UnknownHash => {
                let (hash, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MaterialEntryData::UnknownHash { hash }, bytes_read))
            }
        }
    }
}

pub type MaterialEntry = Entry<MaterialEntryType, MaterialEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MaterialType {
    Material = 0x1,
}

pub enum MaterialData {
    Material { entries: Vec<MaterialEntry> },
}

impl DeserializeEntryData<MaterialType> for MaterialData {
    async fn deserialize(
        entry_type: &MaterialType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MaterialType::Material => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MaterialData::Material { entries }, bytes_read))
            }
        }
    }
}

pub type Material = Entry<MaterialType, MaterialData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum TextureAliasEntryType {
    Unknown = 0x1,
    MaterialIndex = 0x2,
    SemanticHash = 0x3,
    Name = 0x4,
    AssetName = 0x5,
    OcclusionBitMask = 0x6,
    IsDefault = 0x7,
}

pub enum TextureAliasEntryData {
    Unknown { unknown: u8 },
    MaterialIndex { material_index: u8 },
    SemanticHash { hash: u32 },
    Name { name: String },
    AssetName { asset_name: String },
    OcclusionBitMask { bit_mask: Vec<u8> },
    IsDefault { is_default: bool },
}

impl DeserializeEntryData<TextureAliasEntryType> for TextureAliasEntryData {
    async fn deserialize(
        entry_type: &TextureAliasEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            TextureAliasEntryType::Unknown => {
                let (unknown, bytes_read) = deserialize_u8(file, len).await?;
                Ok((TextureAliasEntryData::Unknown { unknown }, bytes_read))
            }
            TextureAliasEntryType::MaterialIndex => {
                let (material_index, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    TextureAliasEntryData::MaterialIndex { material_index },
                    bytes_read,
                ))
            }
            TextureAliasEntryType::SemanticHash => {
                let (hash, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((TextureAliasEntryData::SemanticHash { hash }, bytes_read))
            }
            TextureAliasEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    TextureAliasEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            TextureAliasEntryType::AssetName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    TextureAliasEntryData::AssetName { asset_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            TextureAliasEntryType::OcclusionBitMask => {
                let (bit_mask, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    TextureAliasEntryData::OcclusionBitMask { bit_mask },
                    usize_to_i32(bytes_read)?,
                ))
            }
            TextureAliasEntryType::IsDefault => {
                let (is_default, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    TextureAliasEntryData::IsDefault {
                        is_default: is_default != 0,
                    },
                    bytes_read,
                ))
            }
        }
    }
}

pub type TextureAliasEntry = Entry<TextureAliasEntryType, TextureAliasEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum TextureAliasType {
    TextureAlias = 0x1,
    Unknown = 0xfe,
}

pub enum TextureAliasData {
    TextureAlias { entries: Vec<TextureAliasEntry> },
    Unknown { data: Vec<u8> },
}

impl DeserializeEntryData<TextureAliasType> for TextureAliasData {
    async fn deserialize(
        entry_type: &TextureAliasType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            TextureAliasType::TextureAlias => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((TextureAliasData::TextureAlias { entries }, bytes_read))
            }
            TextureAliasType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    TextureAliasData::Unknown { data },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type TextureAlias = Entry<TextureAliasType, TextureAliasData>;
#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum TintAliasEntryType {
    MaterialIndex = 0x2,
    SemanticHash = 0x3,
    Name = 0x4,
    Red = 0x5,
    Green = 0x6,
    Blue = 0x7,
    IsDefault = 0x8,
}

pub enum TintAliasEntryData {
    MaterialIndex { material_index: u8 },
    SemanticHash { hash: u32 },
    Name { name: String },
    Red { red: f32 },
    Green { green: f32 },
    Blue { blue: f32 },
    IsDefault { is_default: bool },
}

impl DeserializeEntryData<TintAliasEntryType> for TintAliasEntryData {
    async fn deserialize(
        entry_type: &TintAliasEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            TintAliasEntryType::MaterialIndex => {
                let (material_index, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    TintAliasEntryData::MaterialIndex { material_index },
                    bytes_read,
                ))
            }
            TintAliasEntryType::SemanticHash => {
                let (hash, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((TintAliasEntryData::SemanticHash { hash }, bytes_read))
            }
            TintAliasEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((TintAliasEntryData::Name { name }, usize_to_i32(bytes_read)?))
            }
            TintAliasEntryType::Red => {
                let (red, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((TintAliasEntryData::Red { red }, bytes_read))
            }
            TintAliasEntryType::Green => {
                let (green, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((TintAliasEntryData::Green { green }, bytes_read))
            }
            TintAliasEntryType::Blue => {
                let (blue, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((TintAliasEntryData::Blue { blue }, bytes_read))
            }
            TintAliasEntryType::IsDefault => {
                let (is_default, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    TintAliasEntryData::IsDefault {
                        is_default: is_default != 0,
                    },
                    bytes_read,
                ))
            }
        }
    }
}

pub type TintAliasEntry = Entry<TintAliasEntryType, TintAliasEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum TintAliasType {
    TintAlias = 0x1,
    Unknown = 0xfe,
}

pub enum TintAliasData {
    TintAlias { entries: Vec<TintAliasEntry> },
    Unknown { data: Vec<u8> },
}

impl DeserializeEntryData<TintAliasType> for TintAliasData {
    async fn deserialize(
        entry_type: &TintAliasType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            TintAliasType::TintAlias => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((TintAliasData::TintAlias { entries }, bytes_read))
            }
            TintAliasType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((TintAliasData::Unknown { data }, usize_to_i32(bytes_read)?))
            }
        }
    }
}
#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum EffectEntryType {
    Type = 2,
    Name = 3,
    ToolName = 4,
    Id = 5,
}

pub enum EffectEntryData {
    Type { effect_type: u8 },
    Name { name: String },
    ToolName { tool_name: String },
    Id { id: u16 },
}

impl DeserializeEntryData<EffectEntryType> for EffectEntryData {
    async fn deserialize(
        entry_type: &EffectEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            EffectEntryType::Type => {
                let (effect_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((EffectEntryData::Type { effect_type }, bytes_read))
            }
            EffectEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((EffectEntryData::Name { name }, usize_to_i32(bytes_read)?))
            }
            EffectEntryType::ToolName => {
                let (tool_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    EffectEntryData::ToolName { tool_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            EffectEntryType::Id => {
                let (id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((EffectEntryData::Id { id }, bytes_read))
            }
        }
    }
}

pub type EffectEntry = Entry<EffectEntryType, EffectEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum EffectType {
    Effect = 0x1,
    Unknown = 0xfe,
}

pub enum EffectData {
    Effect { entries: Vec<EffectEntry> },
    Unknown { data: Vec<u8> },
}

impl DeserializeEntryData<EffectType> for EffectData {
    async fn deserialize(
        entry_type: &EffectType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            EffectType::Effect => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((EffectData::Effect { entries }, bytes_read))
            }
            EffectType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((EffectData::Unknown { data }, usize_to_i32(bytes_read)?))
            }
        }
    }
}

pub type Effect = Entry<EffectType, EffectData>;

pub type TintAlias = Entry<TintAliasType, TintAliasData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationEntryType {
    Name = 0x1,
    AssetName = 0x2,
    PlayBackScale = 0x3,
    Duration = 0x4,
    LoadType = 0x5,
    EffectsPersist = 0x7,
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationLoadType {
    Unknown1 = 0x0,
    Unknown2 = 0x1,
    Unknown3 = 0x2,
    Unknown4 = 0x4,
}

pub enum AnimationEntryData {
    Name { name: String },
    AssetName { name: String },
    PlayBackScale { scale: f32 },
    Duration { duration_seconds: f32 },
    LoadType { load_type: AnimationLoadType },
    EffectsPersist { do_effects_persist: bool },
}

impl DeserializeEntryData<AnimationEntryType> for AnimationEntryData {
    async fn deserialize(
        entry_type: &AnimationEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((AnimationEntryData::Name { name }, usize_to_i32(bytes_read)?))
            }
            AnimationEntryType::AssetName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationEntryData::AssetName { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationEntryType::PlayBackScale => {
                let (scale, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((AnimationEntryData::PlayBackScale { scale }, bytes_read))
            }
            AnimationEntryType::Duration => {
                let (duration_seconds, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    AnimationEntryData::Duration { duration_seconds },
                    bytes_read,
                ))
            }
            AnimationEntryType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((AnimationEntryData::LoadType { load_type }, bytes_read))
            }
            AnimationEntryType::EffectsPersist => {
                let (do_effects_persist, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationEntryData::EffectsPersist {
                        do_effects_persist: do_effects_persist != 0,
                    },
                    bytes_read,
                ))
            }
        }
    }
}

pub type AnimationEntry = Entry<AnimationEntryType, AnimationEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationType {
    Animation = 0x1,
    Unknown2 = 0xfe,
}

pub enum AnimationData {
    Animation { entries: Vec<AnimationEntry> },
    Unknown2 { data: Vec<u8> },
}

impl DeserializeEntryData<AnimationType> for AnimationData {
    async fn deserialize(
        entry_type: &AnimationType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationType::Animation => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationData::Animation { entries }, bytes_read))
            }
            AnimationType::Unknown2 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AnimationData::Unknown2 { data }, usize_to_i32(bytes_read)?))
            }
        }
    }
}

pub type Animation = Entry<AnimationType, AnimationData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum CollisionEntryType {
    AssetName = 0x1,
}

pub enum CollisionData {
    AssetName { name: String },
}

impl DeserializeEntryData<CollisionEntryType> for CollisionData {
    async fn deserialize(
        entry_type: &CollisionEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            CollisionEntryType::AssetName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((CollisionData::AssetName { name }, usize_to_i32(bytes_read)?))
            }
        }
    }
}

pub type CollisionEntry = Entry<CollisionEntryType, CollisionData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AdrEntryType {
    Skeleton = 0x1,
    Model = 0x2,
    ParticleEmitterArrayArray = 0x3,
    MaterialArray = 0x4,
    TextureAliasArray = 0x5,
    TintAliasArray = 0x6,
    EffectArray = 0x7,
    Unknown6 = 0x8,
    AnimationArray = 0x9,
    Unknown7 = 0xa,
    AnimatedParticle = 0xb,
    Unknown8 = 0xc,
    Collision = 0xd,
    Unknown9 = 0xe,
    Unknown10 = 0xf,
    Unknown11 = 0x10,
    Unknown12 = 0x11,
    Unknown13 = 0x12,
    Unknown14 = 0x13,
    Unknown15 = 0x14,
    Unknown16 = 0x15,
    Unknown17 = 0x16,
}

pub enum AdrData {
    Skeleton { entries: Vec<SkeletonEntry> },
    Model { entries: Vec<ModelEntry> },
    ParticleEmitterArrayArray { arrays: Vec<ParticleEmitterArray> },
    MaterialArray { materials: Vec<Material> },
    TextureAliasArray { texture_aliases: Vec<TextureAlias> },
    TintAliasArray { tint_aliases: Vec<TintAlias> },
    EffectArray { effects: Vec<Effect> },
    Unknown6 { data: Vec<u8> },
    AnimationArray { animations: Vec<Animation> },
    Unknown7 { data: Vec<u8> },
    AnimatedParticle { data: Vec<u8> },
    Unknown8 { data: Vec<u8> },
    Collision { entries: Vec<CollisionEntry> },
    Unknown9 { data: Vec<u8> },
    Unknown10 { data: Vec<u8> },
    Unknown11 { data: Vec<u8> },
    Unknown12 { data: Vec<u8> },
    Unknown13 { data: Vec<u8> },
    Unknown14 { data: Vec<u8> },
    Unknown15 { data: Vec<u8> },
    Unknown16 { data: Vec<u8> },
    Unknown17 { data: Vec<u8> },
}

impl DeserializeEntryData<AdrEntryType> for AdrData {
    async fn deserialize(
        entry_type: &AdrEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AdrEntryType::Skeleton => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Skeleton { entries }, bytes_read))
            }
            AdrEntryType::Model => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Model { entries }, bytes_read))
            }
            AdrEntryType::ParticleEmitterArrayArray => {
                let (arrays, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::ParticleEmitterArrayArray { arrays }, bytes_read))
            }
            AdrEntryType::MaterialArray => {
                let (materials, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::MaterialArray { materials }, bytes_read))
            }
            AdrEntryType::TextureAliasArray => {
                let (texture_aliases, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::TextureAliasArray { texture_aliases }, bytes_read))
            }
            AdrEntryType::TintAliasArray => {
                let (tint_aliases, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::TintAliasArray { tint_aliases }, bytes_read))
            }
            AdrEntryType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::EffectArray { effects }, bytes_read))
            }
            AdrEntryType::Unknown6 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown6 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::AnimationArray => {
                let (animations, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationArray { animations }, bytes_read))
            }
            AdrEntryType::Unknown7 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown7 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::AnimatedParticle => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    AdrData::AnimatedParticle { data },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AdrEntryType::Unknown8 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown8 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Collision => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Collision { entries }, bytes_read))
            }
            AdrEntryType::Unknown9 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown9 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown10 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown10 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown11 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown11 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown12 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown12 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown13 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown13 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown14 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown14 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown15 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown15 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown16 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown16 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown17 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown17 { data }, usize_to_i32(bytes_read)?))
            }
        }
    }
}

pub type AdrEntry = Entry<AdrEntryType, AdrData>;

pub struct Adr {
    pub entries: Vec<AdrEntry>,
}

impl DeserializeAsset for Adr {
    async fn deserialize<P: AsRef<std::path::Path> + Send>(
        _: P,
        file: &mut File,
    ) -> Result<Self, Error> {
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();
        while !is_eof(&mut reader).await? {
            let (entry, _) = AdrEntry::deserialize(&mut reader).await?;
            entries.push(entry);
        }

        Ok(Adr { entries })
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use walkdir::WalkDir;

    #[tokio::test]
    #[ignore]
    async fn test_deserialize_adr() {
        let target_extension = "adr";
        let search_path = env::var("ADR_ROOT").unwrap();

        for entry in WalkDir::new(search_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == target_extension)
            })
        {
            let mut file = File::open(entry.path())
                .await
                .expect(&format!("Failed to open {}", entry.path().display()));
            Adr::deserialize(entry.path(), &mut file)
                .await
                .expect(&format!("Failed to deserialize {}", entry.path().display()));
        }
    }
}
