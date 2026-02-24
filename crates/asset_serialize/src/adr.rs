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
pub enum EntryCountEntryType {
    EntryCount = 0x1,
    EntryCount3 = 0x3,
    EntryCount4 = 0x4,
}

pub enum EntryCountEntryData {
    EntryCount { entry_count: u32 },
    EntryCount3 { entry_count: u32 },
    EntryCount4 { entry_count: u32 },
}

impl DeserializeEntryData<EntryCountEntryType> for EntryCountEntryData {
    async fn deserialize(
        entry_type: &EntryCountEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            EntryCountEntryType::EntryCount => {
                let (entry_count, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((EntryCountEntryData::EntryCount { entry_count }, bytes_read))
            }
            EntryCountEntryType::EntryCount3 => {
                let (entry_count, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((EntryCountEntryData::EntryCount3 { entry_count }, bytes_read))
            }
            EntryCountEntryType::EntryCount4 => {
                let (entry_count, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((EntryCountEntryData::EntryCount4 { entry_count }, bytes_read))
            }
        }
    }
}

pub type EntryCountEntry = Entry<EntryCountEntryType, EntryCountEntryData>;

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
    UpdateRadius = 0x3,
    WaterDisplacementHeight = 0x4,
    ObjectTerrainData = 0x5,
}

pub enum ModelData {
    ModelAssetName { name: String },
    MaterialAssetName { name: String },
    UpdateRadius { radius: f32 },
    WaterDisplacementHeight { height: f32 },
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
            ModelEntryType::UpdateRadius => {
                let (radius, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ModelData::UpdateRadius { radius }, bytes_read))
            }
            ModelEntryType::WaterDisplacementHeight => {
                let (height, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ModelData::WaterDisplacementHeight { height }, bytes_read))
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
pub enum SoundEmitterAssetEntryType {
    AssetName = 0x1,
    TimeOffset = 0x2,
    Weight = 0x3,
}

pub enum SoundEmitterAssetEntryData {
    AssetName { asset_name: String },
    TimeOffset { time_offset_millis: f32 },
    Weight { weight: f32 },
}

impl DeserializeEntryData<SoundEmitterAssetEntryType> for SoundEmitterAssetEntryData {
    async fn deserialize(
        entry_type: &SoundEmitterAssetEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SoundEmitterAssetEntryType::AssetName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    SoundEmitterAssetEntryData::AssetName { asset_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            SoundEmitterAssetEntryType::TimeOffset => {
                let (time_offset_millis, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterAssetEntryData::TimeOffset { time_offset_millis },
                    bytes_read,
                ))
            }
            SoundEmitterAssetEntryType::Weight => {
                let (weight, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterAssetEntryData::Weight { weight }, bytes_read))
            }
        }
    }
}

pub type SoundEmitterAssetEntry = Entry<SoundEmitterAssetEntryType, SoundEmitterAssetEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SoundEmitterEntryType {
    Asset = 0x1,
    Id = 0x2,
    EmitterName = 0x3,
    Unknown4 = 0x4,
    Unknown10 = 0xa,
    Unknown12 = 0xc,
    PlayBackType = 0xd,
    Category = 0xe,
    SubCategory = 0xf,
    FadeTime = 0x10,
    FadeOutTime = 0x11,
    LoadType = 0x12,
    Volume = 0x13,
    VolumeOffset = 0x14,
    RateMultiplier = 0x15,
    RateMultiplierOffset = 0x16,
    RoomTypeScalar = 0x17,
    Unknown24 = 0x18,
    AttenuationDistance = 0x1a,
    ClipDistance = 0x1b,
    DelayBetweenSounds = 0x1c,
    DelayBetweenSoundsOffset = 0x1d,
    EntryCount = 0xfe,
}

pub enum SoundEmitterEntryData {
    Asset {
        entries: Vec<SoundEmitterAssetEntry>,
    },
    Id {
        id: u16,
    },
    EmitterName {
        asset_name: String,
    },
    Unknown4 {
        unknown: f32,
    },
    Unknown10 {
        unknown: f32,
    },
    Unknown12 {
        unknown: u8,
    },
    PlayBackType {
        play_back_type: u8,
    },
    Category {
        category: u8,
    },
    SubCategory {
        sub_category: u8,
    },
    FadeTime {
        fade_time_millis: f32,
    },
    FadeOutTime {
        fade_out_time_millis: f32,
    },
    LoadType {
        load_type: u8,
    },
    Volume {
        volume: f32,
    },
    VolumeOffset {
        volume_offset: f32,
    },
    RateMultiplier {
        rate_multiplier: f32,
    },
    RateMultiplierOffset {
        rate_multiplier_offset: f32,
    },
    RoomTypeScalar {
        room_type_scalar: f32,
    },
    Unknown24 {
        unknown: u8,
    },
    AttenuationDistance {
        distance: f32,
    },
    ClipDistance {
        clip_distance: f32,
    },
    DelayBetweenSounds {
        delay_between_sounds_millis: f32,
    },
    DelayBetweenSoundsOffset {
        delay_between_sounds_offset_millis: f32,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<SoundEmitterEntryType> for SoundEmitterEntryData {
    async fn deserialize(
        entry_type: &SoundEmitterEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SoundEmitterEntryType::Asset => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((SoundEmitterEntryData::Asset { entries }, bytes_read))
            }
            SoundEmitterEntryType::Id => {
                let (id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((SoundEmitterEntryData::Id { id }, bytes_read))
            }
            SoundEmitterEntryType::EmitterName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    SoundEmitterEntryData::EmitterName { asset_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            SoundEmitterEntryType::Unknown4 => {
                let (unknown, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::Unknown4 { unknown }, bytes_read))
            }
            SoundEmitterEntryType::Unknown10 => {
                let (unknown, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::Unknown10 { unknown }, bytes_read))
            }
            SoundEmitterEntryType::Unknown12 => {
                let (unknown, bytes_read) = deserialize_u8(file, len).await?;
                Ok((SoundEmitterEntryData::Unknown12 { unknown }, bytes_read))
            }
            SoundEmitterEntryType::PlayBackType => {
                let (play_back_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    SoundEmitterEntryData::PlayBackType { play_back_type },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::Category => {
                let (category, bytes_read) = deserialize_u8(file, len).await?;
                Ok((SoundEmitterEntryData::Category { category }, bytes_read))
            }
            SoundEmitterEntryType::SubCategory => {
                let (sub_category, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    SoundEmitterEntryData::SubCategory { sub_category },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::FadeTime => {
                let (fade_time_millis, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::FadeTime { fade_time_millis },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::FadeOutTime => {
                let (fade_out_time_millis, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::FadeOutTime {
                        fade_out_time_millis,
                    },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::LoadType => {
                let (load_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((SoundEmitterEntryData::LoadType { load_type }, bytes_read))
            }
            SoundEmitterEntryType::Volume => {
                let (volume, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::Volume { volume }, bytes_read))
            }
            SoundEmitterEntryType::VolumeOffset => {
                let (volume_offset, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::VolumeOffset { volume_offset },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::RateMultiplier => {
                let (rate_multiplier, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::RateMultiplier { rate_multiplier },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::RateMultiplierOffset => {
                let (rate_multiplier_offset, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::RateMultiplierOffset {
                        rate_multiplier_offset,
                    },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::RoomTypeScalar => {
                let (room_type_scalar, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::RoomTypeScalar { room_type_scalar },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::Unknown24 => {
                let (unknown, bytes_read) = deserialize_u8(file, len).await?;
                Ok((SoundEmitterEntryData::Unknown24 { unknown }, bytes_read))
            }
            SoundEmitterEntryType::AttenuationDistance => {
                let (distance, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::AttenuationDistance { distance },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::ClipDistance => {
                let (clip_distance, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::ClipDistance { clip_distance },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::DelayBetweenSounds => {
                let (delay_between_sounds_millis, bytes_read) =
                    deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::DelayBetweenSounds {
                        delay_between_sounds_millis,
                    },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::DelayBetweenSoundsOffset => {
                let (delay_between_sounds_offset_millis, bytes_read) =
                    deserialize_f32_be(file, len).await?;
                Ok((
                    SoundEmitterEntryData::DelayBetweenSoundsOffset {
                        delay_between_sounds_offset_millis,
                    },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((SoundEmitterEntryData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type SoundEmitterEntry = Entry<SoundEmitterEntryType, SoundEmitterEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SoundEmitterType {
    SoundEmitter = 0x1,
    EntryCount = 0xfe,
}

pub enum SoundEmitterData {
    SoundEmitter { entries: Vec<SoundEmitterEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<SoundEmitterType> for SoundEmitterData {
    async fn deserialize(
        entry_type: &SoundEmitterType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SoundEmitterType::SoundEmitter => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((SoundEmitterData::SoundEmitter { entries }, bytes_read))
            }
            SoundEmitterType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((SoundEmitterData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type SoundEmitter = Entry<SoundEmitterType, SoundEmitterData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEmitterEntryType {
    Id = 0x1,
    EmitterName = 0x2,
    BoneName = 0x3,
    Heading = 0x4,
    Pitch = 0x5,
    Scale = 0x6,
    OffsetX = 0x7,
    OffsetY = 0x8,
    OffsetZ = 0x9,
    AssetName = 0xa,
    SourceBoneName = 0xb,
    LocalSpaceDerived = 0xc,
    WorldOrientation = 0xd,
    HardStop = 0xe,
}

pub enum ParticleEmitterEntryData {
    Id { id: u16 },
    EmitterName { emitter_name: String },
    BoneName { bone_name: String },
    Heading { heading: f32 },
    Pitch { pitch: f32 },
    Scale { scale: f32 },
    OffsetX { offset_x: f32 },
    OffsetY { offset_y: f32 },
    OffsetZ { offset_z: f32 },
    AssetName { asset_name: String },
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
            ParticleEmitterEntryType::Id => {
                let (id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((ParticleEmitterEntryData::Id { id }, bytes_read))
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
            ParticleEmitterEntryType::AssetName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterEntryData::AssetName { asset_name },
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
    EntryCount = 0xfe,
}

pub enum ParticleEmitterData {
    ParticleEmitter { entries: Vec<ParticleEmitterEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
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
            ParticleEmitterType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((ParticleEmitterData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type ParticleEmitter = Entry<ParticleEmitterType, ParticleEmitterData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum EmitterArrayType {
    SoundEmitterArray = 0x1,
    ParticleEmitterArray = 0x2,
}

pub enum EmitterArrayData {
    SoundEmitterArray {
        sound_emitters: Vec<SoundEmitter>,
    },
    ParticleEmitterArray {
        particle_emitters: Vec<ParticleEmitter>,
    },
}

impl DeserializeEntryData<EmitterArrayType> for EmitterArrayData {
    async fn deserialize(
        entry_type: &EmitterArrayType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            EmitterArrayType::SoundEmitterArray => {
                let (sound_emitters, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    EmitterArrayData::SoundEmitterArray { sound_emitters },
                    bytes_read,
                ))
            }
            EmitterArrayType::ParticleEmitterArray => {
                let (particle_emitters, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    EmitterArrayData::ParticleEmitterArray { particle_emitters },
                    bytes_read,
                ))
            }
        }
    }
}

pub type EmitterArray = Entry<EmitterArrayType, EmitterArrayData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MaterialTagEntryType {
    Name = 0x1,
    SemanticHash = 0x3,
}

pub enum MaterialTagEntryData {
    Name { name: String },
    SemanticHash { hash: u32 },
}

impl DeserializeEntryData<MaterialTagEntryType> for MaterialTagEntryData {
    async fn deserialize(
        entry_type: &MaterialTagEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MaterialTagEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MaterialTagEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MaterialTagEntryType::SemanticHash => {
                let (hash, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MaterialTagEntryData::SemanticHash { hash }, bytes_read))
            }
        }
    }
}

pub type MaterialTagEntry = Entry<MaterialTagEntryType, MaterialTagEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MaterialTagType {
    MaterialTag = 0x1,
    EntryCount = 0xfe,
}

pub enum MaterialTagData {
    Material { entries: Vec<MaterialTagEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<MaterialTagType> for MaterialTagData {
    async fn deserialize(
        entry_type: &MaterialTagType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MaterialTagType::MaterialTag => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MaterialTagData::Material { entries }, bytes_read))
            }
            MaterialTagType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MaterialTagData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type MaterialTag = Entry<MaterialTagType, MaterialTagData>;

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
    EntryCount = 0xfe,
}

pub enum TextureAliasData {
    TextureAlias { entries: Vec<TextureAliasEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
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
            TextureAliasType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((TextureAliasData::EntryCount { entries }, bytes_read))
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
    EntryCount = 0xfe,
}

pub enum TintAliasData {
    TintAlias { entries: Vec<TintAliasEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
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
            TintAliasType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((TintAliasData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type TintAlias = Entry<TintAliasType, TintAliasData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum EffectEntryType {
    Type = 0x2,
    Name = 0x3,
    ToolName = 0x4,
    Id = 0x5,
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
    EntryCount = 0xfe,
}

pub enum EffectData {
    Effect { entries: Vec<EffectEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
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
            EffectType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((EffectData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type Effect = Entry<EffectType, EffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum RenderSettingEntryType {
    MaxDistanceFromCamera = 0x2,
}

pub enum RenderSettingEntryData {
    MaxDistanceFromCamera { distance: f32 },
}

impl DeserializeEntryData<RenderSettingEntryType> for RenderSettingEntryData {
    async fn deserialize(
        entry_type: &RenderSettingEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            RenderSettingEntryType::MaxDistanceFromCamera => {
                let (distance, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    RenderSettingEntryData::MaxDistanceFromCamera { distance },
                    bytes_read,
                ))
            }
        }
    }
}

pub type RenderSettingEntry = Entry<RenderSettingEntryType, RenderSettingEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum RenderSettingType {
    RenderSetting = 0x1,
    EntryCount = 0xfe,
}

pub enum RenderSettingData {
    RenderSetting { entries: Vec<RenderSettingEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<RenderSettingType> for RenderSettingData {
    async fn deserialize(
        entry_type: &RenderSettingType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            RenderSettingType::RenderSetting => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((RenderSettingData::RenderSetting { entries }, bytes_read))
            }
            RenderSettingType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((RenderSettingData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type RenderSetting = Entry<RenderSettingType, RenderSettingData>;

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
    Required = 0x0,
    Preload = 0x1,
    OnDemand = 0x2,
    InheritFromParent = 0x3,
    RequiredFirst = 0x4,
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
    EntryCount = 0xfe,
}

pub enum AnimationData {
    Animation { entries: Vec<AnimationEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
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
            AnimationType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type Animation = Entry<AnimationType, AnimationData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationEffectTriggerEventType {
    Start = 0x1,
    End = 0x2,
}

pub enum AnimationEffectTriggerEventData {
    Start { start_seconds: f32 },
    End { end_seconds: f32 },
}

impl DeserializeEntryData<AnimationEffectTriggerEventType> for AnimationEffectTriggerEventData {
    async fn deserialize(
        entry_type: &AnimationEffectTriggerEventType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationEffectTriggerEventType::Start => {
                let (start_seconds, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    AnimationEffectTriggerEventData::Start { start_seconds },
                    bytes_read,
                ))
            }
            AnimationEffectTriggerEventType::End => {
                let (end_seconds, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    AnimationEffectTriggerEventData::End { end_seconds },
                    bytes_read,
                ))
            }
        }
    }
}

pub type AnimationEffectTriggerEvent =
    Entry<AnimationEffectTriggerEventType, AnimationEffectTriggerEventData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationSoundEffectType {
    TriggerEventArray = 0x1,
    Type = 0x2,
    Name = 0x3,
    ToolName = 0x4,
    Id = 0x5,
    PlayOnce = 0x6,
    LoadType = 0x7,
    EntryCount = 0xfe,
}

pub enum AnimationSoundEffectData {
    TriggerEventArray {
        trigger_events: Vec<AnimationEffectTriggerEvent>,
    },
    Type {
        effect_type: u8,
    },
    Name {
        name: String,
    },
    ToolName {
        tool_name: String,
    },
    Id {
        id: u16,
    },
    PlayOnce {
        should_play_once: bool,
    },
    LoadType {
        load_type: AnimationLoadType,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationSoundEffectType> for AnimationSoundEffectData {
    async fn deserialize(
        entry_type: &AnimationSoundEffectType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationSoundEffectType::TriggerEventArray => {
                let (trigger_events, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationSoundEffectData::TriggerEventArray { trigger_events },
                    bytes_read,
                ))
            }
            AnimationSoundEffectType::Type => {
                let (effect_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((AnimationSoundEffectData::Type { effect_type }, bytes_read))
            }
            AnimationSoundEffectType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationSoundEffectData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationSoundEffectType::ToolName => {
                let (tool_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationSoundEffectData::ToolName { tool_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationSoundEffectType::Id => {
                let (id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((AnimationSoundEffectData::Id { id }, bytes_read))
            }
            AnimationSoundEffectType::PlayOnce => {
                let (should_play_once, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationSoundEffectData::PlayOnce {
                        should_play_once: should_play_once != 0,
                    },
                    bytes_read,
                ))
            }
            AnimationSoundEffectType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((AnimationSoundEffectData::LoadType { load_type }, bytes_read))
            }
            AnimationSoundEffectType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationSoundEffectData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationSoundEffect = Entry<AnimationSoundEffectType, AnimationSoundEffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationSoundEntryType {
    EffectArray = 0x1,
    Name = 0x2,
    EntryCount = 0xfe,
}

pub enum AnimationSoundEntryData {
    EffectArray { effects: Vec<AnimationSoundEffect> },
    Name { name: String },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<AnimationSoundEntryType> for AnimationSoundEntryData {
    async fn deserialize(
        entry_type: &AnimationSoundEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationSoundEntryType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationSoundEntryData::EffectArray { effects }, bytes_read))
            }
            AnimationSoundEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationSoundEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationSoundEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationSoundEntryData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationSoundEntry = Entry<AnimationSoundEntryType, AnimationSoundEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationSoundType {
    AnimationSound = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationSoundData {
    AnimationSound { entries: Vec<AnimationSoundEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<AnimationSoundType> for AnimationSoundData {
    async fn deserialize(
        entry_type: &AnimationSoundType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationSoundType::AnimationSound => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationSoundData::AnimationSound { entries }, bytes_read))
            }
            AnimationSoundType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationSoundData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationSound = Entry<AnimationSoundType, AnimationSoundData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationParticleEffectType {
    TriggerEventArray = 0x1,
    Type = 0x2,
    Name = 0x3,
    ToolName = 0x4,
    Id = 0x5,
    PlayOnce = 0x6,
    LoadType = 0x7,
    EntryCount = 0xfe,
}

pub enum AnimationParticleEffectData {
    TriggerEventArray {
        trigger_events: Vec<AnimationEffectTriggerEvent>,
    },
    Type {
        effect_type: u8,
    },
    Name {
        name: String,
    },
    ToolName {
        tool_name: String,
    },
    Id {
        id: u16,
    },
    PlayOnce {
        should_play_once: bool,
    },
    LoadType {
        load_type: AnimationLoadType,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationParticleEffectType> for AnimationParticleEffectData {
    async fn deserialize(
        entry_type: &AnimationParticleEffectType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationParticleEffectType::TriggerEventArray => {
                let (trigger_events, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationParticleEffectData::TriggerEventArray { trigger_events },
                    bytes_read,
                ))
            }
            AnimationParticleEffectType::Type => {
                let (effect_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationParticleEffectData::Type { effect_type },
                    bytes_read,
                ))
            }
            AnimationParticleEffectType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationParticleEffectData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationParticleEffectType::ToolName => {
                let (tool_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationParticleEffectData::ToolName { tool_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationParticleEffectType::Id => {
                let (id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((AnimationParticleEffectData::Id { id }, bytes_read))
            }
            AnimationParticleEffectType::PlayOnce => {
                let (should_play_once, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationParticleEffectData::PlayOnce {
                        should_play_once: should_play_once != 0,
                    },
                    bytes_read,
                ))
            }
            AnimationParticleEffectType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((
                    AnimationParticleEffectData::LoadType { load_type },
                    bytes_read,
                ))
            }
            AnimationParticleEffectType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationParticleEffectData::EntryCount { entries },
                    bytes_read,
                ))
            }
        }
    }
}

pub type AnimationParticleEffect = Entry<AnimationParticleEffectType, AnimationParticleEffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationParticleEntryType {
    EffectArray = 0x1,
    Name = 0x2,
    EntryCount = 0xfe,
}

pub enum AnimationParticleEntryData {
    EffectArray {
        effects: Vec<AnimationParticleEffect>,
    },
    Name {
        name: String,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationParticleEntryType> for AnimationParticleEntryData {
    async fn deserialize(
        entry_type: &AnimationParticleEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationParticleEntryType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationParticleEntryData::EffectArray { effects },
                    bytes_read,
                ))
            }
            AnimationParticleEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationParticleEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationParticleEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationParticleEntryData::EntryCount { entries },
                    bytes_read,
                ))
            }
        }
    }
}

pub type AnimationParticleEntry = Entry<AnimationParticleEntryType, AnimationParticleEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationParticleType {
    AnimationParticle = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationParticleData {
    AnimationParticle {
        entries: Vec<AnimationParticleEntry>,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationParticleType> for AnimationParticleData {
    async fn deserialize(
        entry_type: &AnimationParticleType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationParticleType::AnimationParticle => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationParticleData::AnimationParticle { entries },
                    bytes_read,
                ))
            }
            AnimationParticleType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationParticleData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationParticle = Entry<AnimationParticleType, AnimationParticleData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ActionPointEntryType {
    Name = 0x1,
    Time = 0x2,
}

pub enum ActionPointEntryData {
    Name { name: String },
    Time { time_seconds: f32 },
}

impl DeserializeEntryData<ActionPointEntryType> for ActionPointEntryData {
    async fn deserialize(
        entry_type: &ActionPointEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ActionPointEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    ActionPointEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ActionPointEntryType::Time => {
                let (time_seconds, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((ActionPointEntryData::Time { time_seconds }, bytes_read))
            }
        }
    }
}

pub type ActionPointEntry = Entry<ActionPointEntryType, ActionPointEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ActionPointType {
    ActionPoint = 0x1,
    EntryCount = 0xfe,
}

pub enum ActionPointData {
    ActionPoint { entries: Vec<ActionPointEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<ActionPointType> for ActionPointData {
    async fn deserialize(
        entry_type: &ActionPointType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ActionPointType::ActionPoint => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((ActionPointData::ActionPoint { entries }, bytes_read))
            }
            ActionPointType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((ActionPointData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type ActionPoint = Entry<ActionPointType, ActionPointData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationActionPointEntryType {
    ActionPointArray = 0x1,
    Name = 0x2,
}

pub enum AnimationActionPointEntryData {
    ActionPointArray { action_points: Vec<ActionPoint> },
    Name { name: String },
}

impl DeserializeEntryData<AnimationActionPointEntryType> for AnimationActionPointEntryData {
    async fn deserialize(
        entry_type: &AnimationActionPointEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationActionPointEntryType::ActionPointArray => {
                let (action_points, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationActionPointEntryData::ActionPointArray { action_points },
                    bytes_read,
                ))
            }
            AnimationActionPointEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationActionPointEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type AnimationActionPointEntry =
    Entry<AnimationActionPointEntryType, AnimationActionPointEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationActionPointType {
    AnimationActionPoint = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationActionPointData {
    AnimationActionPoint {
        entries: Vec<AnimationActionPointEntry>,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationActionPointType> for AnimationActionPointData {
    async fn deserialize(
        entry_type: &AnimationActionPointType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationActionPointType::AnimationActionPoint => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationActionPointData::AnimationActionPoint { entries },
                    bytes_read,
                ))
            }
            AnimationActionPointType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationActionPointData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationActionPoint = Entry<AnimationActionPointType, AnimationActionPointData>;

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
pub enum CoveredSlotEntryType {
    SlotId = 0x1,
}

pub enum CoveredSlotEntryData {
    SlotId { slot_id: u8 },
}

impl DeserializeEntryData<CoveredSlotEntryType> for CoveredSlotEntryData {
    async fn deserialize(
        entry_type: &CoveredSlotEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            CoveredSlotEntryType::SlotId => {
                let (bone_id, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    CoveredSlotEntryData::SlotId { slot_id: bone_id },
                    bytes_read,
                ))
            }
        }
    }
}

pub type CoveredSlotEntry = Entry<CoveredSlotEntryType, CoveredSlotEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum OcclusionEntryType {
    SlotBitMask = 0x1,
    BitMask = 0x2,
    CoveredSlot = 0x4,
    EntryCount = 0xfe,
}

pub enum OcclusionData {
    SlotBitMask { bit_mask: Vec<u8> },
    BitMask { bit_mask: Vec<u8> },
    CoveredSlot { entries: Vec<CoveredSlotEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<OcclusionEntryType> for OcclusionData {
    async fn deserialize(
        entry_type: &OcclusionEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            OcclusionEntryType::SlotBitMask => {
                let (bit_mask, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    OcclusionData::SlotBitMask { bit_mask },
                    usize_to_i32(bytes_read)?,
                ))
            }
            OcclusionEntryType::BitMask => {
                let (bit_mask, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    OcclusionData::BitMask { bit_mask },
                    usize_to_i32(bytes_read)?,
                ))
            }
            OcclusionEntryType::CoveredSlot => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((OcclusionData::CoveredSlot { entries }, bytes_read))
            }
            OcclusionEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((OcclusionData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type OcclusionEntry = Entry<OcclusionEntryType, OcclusionData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum UsageEntryType {
    Usage = 0x1,
    AttachmentBoneName = 0x2,
    ValidatePcNpc = 0x3,
    InheritAnimations = 0x4,
    ReplicationBoneName = 0x5,
}

pub enum UsageEntryData {
    Usage { usage: u8 },
    AttachmentBoneName { bone_name: String },
    ValidatePcNpc { validate: bool },
    InheritAnimations { should_inherit_animations: bool },
    ReplicationBoneName { bone_name: String },
}

impl DeserializeEntryData<UsageEntryType> for UsageEntryData {
    async fn deserialize(
        entry_type: &UsageEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            UsageEntryType::Usage => {
                let (usage, bytes_read) = deserialize_u8(file, len).await?;
                Ok((UsageEntryData::Usage { usage }, bytes_read))
            }
            UsageEntryType::AttachmentBoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    UsageEntryData::AttachmentBoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            UsageEntryType::ValidatePcNpc => {
                let (validate, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    UsageEntryData::ValidatePcNpc {
                        validate: validate != 0,
                    },
                    bytes_read,
                ))
            }
            UsageEntryType::InheritAnimations => {
                let (should_inherit_animations, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    UsageEntryData::InheritAnimations {
                        should_inherit_animations: should_inherit_animations != 0,
                    },
                    bytes_read,
                ))
            }
            UsageEntryType::ReplicationBoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    UsageEntryData::ReplicationBoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type UsageEntry = Entry<UsageEntryType, UsageEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum HatHairEntryType {
    CoverFacialHair = 0x1,
    Type = 0x2,
}

pub enum HatHairEntryData {
    CoverFacialHair { should_cover_facial_hair: bool },
    Type { hat_hair_type: u8 },
}

impl DeserializeEntryData<HatHairEntryType> for HatHairEntryData {
    async fn deserialize(
        entry_type: &HatHairEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            HatHairEntryType::CoverFacialHair => {
                let (should_cover_facial_hair, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    HatHairEntryData::CoverFacialHair {
                        should_cover_facial_hair: should_cover_facial_hair != 0,
                    },
                    bytes_read,
                ))
            }
            HatHairEntryType::Type => {
                let (hat_hair_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((HatHairEntryData::Type { hat_hair_type }, bytes_read))
            }
        }
    }
}

pub type HatHairEntry = Entry<HatHairEntryType, HatHairEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ShadowEntryType {
    CheckShadowVisibility = 0x1,
}

pub enum ShadowEntryData {
    CheckShadowVisibility {
        should_check_shadow_visibility: bool,
    },
}

impl DeserializeEntryData<ShadowEntryType> for ShadowEntryData {
    async fn deserialize(
        entry_type: &ShadowEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ShadowEntryType::CheckShadowVisibility => {
                let (should_check_shadow_visibility, bytes_read) =
                    deserialize_u8(file, len).await?;
                Ok((
                    ShadowEntryData::CheckShadowVisibility {
                        should_check_shadow_visibility: should_check_shadow_visibility != 0,
                    },
                    bytes_read,
                ))
            }
        }
    }
}

pub type ShadowEntry = Entry<ShadowEntryType, ShadowEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum EquippedSlotEntryType {
    Type = 0x1,
    SlotId = 0x2,
    ParentAttachSlot = 0x3,
    ChildAttachSlot = 0x4,
    SlotName = 0x5,
}

pub enum EquippedSlotEntryData {
    Type { equipped_slot_type: u8 },
    SlotId { slot_id: u8 },
    ParentAttachSlot { slot_name: String },
    ChildAttachSlot { slot_name: String },
    SlotName { slot_name: String },
}

impl DeserializeEntryData<EquippedSlotEntryType> for EquippedSlotEntryData {
    async fn deserialize(
        entry_type: &EquippedSlotEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            EquippedSlotEntryType::Type => {
                let (equipped_slot_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    EquippedSlotEntryData::Type { equipped_slot_type },
                    bytes_read,
                ))
            }
            EquippedSlotEntryType::SlotId => {
                let (slot_id, bytes_read) = deserialize_u8(file, len).await?;
                Ok((EquippedSlotEntryData::SlotId { slot_id }, bytes_read))
            }
            EquippedSlotEntryType::ParentAttachSlot => {
                let (slot_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    EquippedSlotEntryData::ParentAttachSlot { slot_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            EquippedSlotEntryType::ChildAttachSlot => {
                let (slot_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    EquippedSlotEntryData::ChildAttachSlot { slot_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            EquippedSlotEntryType::SlotName => {
                let (slot_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    EquippedSlotEntryData::SlotName { slot_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type EquippedSlotEntry = Entry<EquippedSlotEntryType, EquippedSlotEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MountSeatEntranceExitEntryType {
    BoneName = 0x1,
    Animation = 0x2,
    Location = 0x3,
}

pub enum MountSeatEntranceExitEntryData {
    BoneName { bone_name: String },
    Animation { animation_name: String },
    Location { location: String },
}

impl DeserializeEntryData<MountSeatEntranceExitEntryType> for MountSeatEntranceExitEntryData {
    async fn deserialize(
        entry_type: &MountSeatEntranceExitEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MountSeatEntranceExitEntryType::BoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountSeatEntranceExitEntryData::BoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountSeatEntranceExitEntryType::Animation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountSeatEntranceExitEntryData::Animation { animation_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountSeatEntranceExitEntryType::Location => {
                let (location, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountSeatEntranceExitEntryData::Location { location },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type MountSeatEntranceExitEntry =
    Entry<MountSeatEntranceExitEntryType, MountSeatEntranceExitEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MountSeatEntryType {
    Entrance = 0x1,
    Exit = 0x2,
    BoneName = 0x3,
    Animation = 0x4,
}

pub enum MountSeatEntryData {
    Entrance {
        entries: Vec<MountSeatEntranceExitEntry>,
    },
    Exit {
        entries: Vec<MountSeatEntranceExitEntry>,
    },
    Bone {
        bone_name: String,
    },
    Animation {
        animation_name: String,
    },
}

impl DeserializeEntryData<MountSeatEntryType> for MountSeatEntryData {
    async fn deserialize(
        entry_type: &MountSeatEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MountSeatEntryType::Entrance => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MountSeatEntryData::Entrance { entries }, bytes_read))
            }
            MountSeatEntryType::Exit => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MountSeatEntryData::Exit { entries }, bytes_read))
            }
            MountSeatEntryType::BoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountSeatEntryData::Bone { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountSeatEntryType::Animation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountSeatEntryData::Animation { animation_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type MountSeatEntry = Entry<MountSeatEntryType, MountSeatEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MountEntryType {
    Seat = 0x1,
    StandAnimation = 0x5,
    SprintAnimation = 0x7,
    SprintToStandAnimation = 0x9,
    EntryCount = 0xfe,
}

pub enum MountEntryData {
    Seat { entries: Vec<MountSeatEntry> },
    StandAnimation { animation_name: String },
    SprintAnimation { animation_name: String },
    SprintToStandAnimation { animation_name: String },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<MountEntryType> for MountEntryData {
    async fn deserialize(
        entry_type: &MountEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MountEntryType::Seat => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MountEntryData::Seat { entries }, bytes_read))
            }
            MountEntryType::StandAnimation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountEntryData::StandAnimation { animation_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountEntryType::SprintAnimation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountEntryData::SprintAnimation { animation_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountEntryType::SprintToStandAnimation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountEntryData::SprintToStandAnimation { animation_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MountEntryData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type MountEntry = Entry<MountEntryType, MountEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationCompositeEffectType {
    TriggerEventArray = 0x1,
    Type = 0x2,
    Name = 0x3,
    ToolName = 0x4,
    Id = 0x5,
    PlayOnce = 0x6,
    LoadType = 0x7,
    EntryCount = 0xfe,
}

pub enum AnimationCompositeEffectData {
    TriggerEventArray {
        trigger_events: Vec<AnimationEffectTriggerEvent>,
    },
    Type {
        effect_type: u8,
    },
    Name {
        name: String,
    },
    ToolName {
        tool_name: String,
    },
    Id {
        id: u16,
    },
    PlayOnce {
        should_play_once: bool,
    },
    LoadType {
        load_type: AnimationLoadType,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationCompositeEffectType> for AnimationCompositeEffectData {
    async fn deserialize(
        entry_type: &AnimationCompositeEffectType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationCompositeEffectType::TriggerEventArray => {
                let (trigger_events, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationCompositeEffectData::TriggerEventArray { trigger_events },
                    bytes_read,
                ))
            }
            AnimationCompositeEffectType::Type => {
                let (effect_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationCompositeEffectData::Type { effect_type },
                    bytes_read,
                ))
            }
            AnimationCompositeEffectType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationCompositeEffectData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationCompositeEffectType::ToolName => {
                let (tool_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationCompositeEffectData::ToolName { tool_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationCompositeEffectType::Id => {
                let (id, bytes_read) = deserialize_u16_le(file, len).await?;
                Ok((AnimationCompositeEffectData::Id { id }, bytes_read))
            }
            AnimationCompositeEffectType::PlayOnce => {
                let (should_play_once, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationCompositeEffectData::PlayOnce {
                        should_play_once: should_play_once != 0,
                    },
                    bytes_read,
                ))
            }
            AnimationCompositeEffectType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((
                    AnimationCompositeEffectData::LoadType { load_type },
                    bytes_read,
                ))
            }
            AnimationCompositeEffectType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationCompositeEffectData::EntryCount { entries },
                    bytes_read,
                ))
            }
        }
    }
}

pub type AnimationCompositeEffect =
    Entry<AnimationCompositeEffectType, AnimationCompositeEffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationCompositeEntryType {
    EffectArray = 0x1,
    Name = 0x2,
    EntryCount = 0xfe,
}

pub enum AnimationCompositeEntryData {
    EffectArray {
        effects: Vec<AnimationCompositeEffect>,
    },
    Name {
        name: String,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationCompositeEntryType> for AnimationCompositeEntryData {
    async fn deserialize(
        entry_type: &AnimationCompositeEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationCompositeEntryType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationCompositeEntryData::EffectArray { effects },
                    bytes_read,
                ))
            }
            AnimationCompositeEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationCompositeEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationCompositeEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationCompositeEntryData::EntryCount { entries },
                    bytes_read,
                ))
            }
        }
    }
}

pub type AnimationCompositeEntry = Entry<AnimationCompositeEntryType, AnimationCompositeEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationCompositeType {
    AnimationComposite = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationCompositeData {
    AnimationComposite {
        entries: Vec<AnimationCompositeEntry>,
    },
    EntryCount {
        entries: Vec<EntryCountEntry>,
    },
}

impl DeserializeEntryData<AnimationCompositeType> for AnimationCompositeData {
    async fn deserialize(
        entry_type: &AnimationCompositeType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationCompositeType::AnimationComposite => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationCompositeData::AnimationComposite { entries },
                    bytes_read,
                ))
            }
            AnimationCompositeType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationCompositeData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationComposite = Entry<AnimationCompositeType, AnimationCompositeData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum JointEntryType {
    BoneName = 0x1,
    PitchLimit = 0x2,
    YawLimit = 0x3,
    TurnRate = 0x4,
}

pub enum JointEntryData {
    BoneName { bone_name: String },
    PitchLimit { pitch_limit: f32 },
    YawLimit { yaw_limit: f32 },
    TurnRate { turn_rate: f32 },
}

impl DeserializeEntryData<JointEntryType> for JointEntryData {
    async fn deserialize(
        entry_type: &JointEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            JointEntryType::BoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    JointEntryData::BoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            JointEntryType::PitchLimit => {
                let (pitch_limit, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((JointEntryData::PitchLimit { pitch_limit }, bytes_read))
            }
            JointEntryType::YawLimit => {
                let (yaw_limit, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((JointEntryData::YawLimit { yaw_limit }, bytes_read))
            }
            JointEntryType::TurnRate => {
                let (turn_rate, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((JointEntryData::TurnRate { turn_rate }, bytes_read))
            }
        }
    }
}

pub type JointEntry = Entry<JointEntryType, JointEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum LookControlEntryType {
    Name = 0x1,
    Type = 0x2,
    Joint = 0x3,
    EffectorBone = 0x4,
    EntryCount = 0xfe,
}

pub enum LookControlEntryData {
    Name { name: String },
    Type { look_control_type: u8 },
    Joint { entries: Vec<JointEntry> },
    EffectorBone { bone_name: String },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<LookControlEntryType> for LookControlEntryData {
    async fn deserialize(
        entry_type: &LookControlEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            LookControlEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    LookControlEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            LookControlEntryType::Type => {
                let (look_control_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((LookControlEntryData::Type { look_control_type }, bytes_read))
            }
            LookControlEntryType::Joint => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((LookControlEntryData::Joint { entries }, bytes_read))
            }
            LookControlEntryType::EffectorBone => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    LookControlEntryData::EffectorBone { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            LookControlEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((LookControlEntryData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type LookControlEntry = Entry<LookControlEntryType, LookControlEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum LookControlType {
    LookControl = 0x1,
}

pub enum LookControlData {
    LookControl { entries: Vec<LookControlEntry> },
}

impl DeserializeEntryData<LookControlType> for LookControlData {
    async fn deserialize(
        entry_type: &LookControlType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            LookControlType::LookControl => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((LookControlData::LookControl { entries }, bytes_read))
            }
        }
    }
}

pub type LookControl = Entry<LookControlType, LookControlData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AdrEntryType {
    Skeleton = 0x1,
    Model = 0x2,
    EmitterArrayArray = 0x3,
    MaterialTagArray = 0x4,
    TextureAliasArray = 0x5,
    TintAliasArray = 0x6,
    EffectArray = 0x7,
    RenderSettingArray = 0x8,
    AnimationArray = 0x9,
    AnimationSoundArray = 0xa,
    AnimationParticleArray = 0xb,
    AnimationActionPoint = 0xc,
    Collision = 0xd,
    Occlusion = 0xe,
    Usage = 0xf,
    HatHair = 0x10,
    Shadow = 0x11,
    EquippedSlot = 0x12,
    BorrowedSkeleton = 0x13,
    Mount = 0x14,
    AnimationCompositeArray = 0x15,
    LookControlArray = 0x16,
}

pub enum AdrData {
    Skeleton {
        entries: Vec<SkeletonEntry>,
    },
    Model {
        entries: Vec<ModelEntry>,
    },
    EmitterArrayArray {
        arrays: Vec<EmitterArray>,
    },
    MaterialTagArray {
        material_tags: Vec<MaterialTag>,
    },
    TextureAliasArray {
        texture_aliases: Vec<TextureAlias>,
    },
    TintAliasArray {
        tint_aliases: Vec<TintAlias>,
    },
    EffectArray {
        effects: Vec<Effect>,
    },
    RenderSettingArray {
        render_settings: Vec<RenderSetting>,
    },
    AnimationArray {
        animations: Vec<Animation>,
    },
    AnimationSoundArray {
        sounds: Vec<AnimationSound>,
    },
    AnimationParticleArray {
        particles: Vec<AnimationParticle>,
    },
    AnimationActionPoint {
        action_points: Vec<AnimationActionPoint>,
    },
    Collision {
        entries: Vec<CollisionEntry>,
    },
    Occlusion {
        entries: Vec<OcclusionEntry>,
    },
    Usage {
        entries: Vec<UsageEntry>,
    },
    HatHair {
        entries: Vec<HatHairEntry>,
    },
    Shadow {
        entries: Vec<ShadowEntry>,
    },
    EquippedSlot {
        entries: Vec<EquippedSlotEntry>,
    },
    BorrowedSkeleton {
        data: Vec<u8>,
    },
    Mount {
        entries: Vec<MountEntry>,
    },
    AnimationCompositeArray {
        composites: Vec<AnimationComposite>,
    },
    LookControlArray {
        look_controls: Vec<LookControl>,
    },
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
            AdrEntryType::EmitterArrayArray => {
                let (arrays, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::EmitterArrayArray { arrays }, bytes_read))
            }
            AdrEntryType::MaterialTagArray => {
                let (materials, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AdrData::MaterialTagArray {
                        material_tags: materials,
                    },
                    bytes_read,
                ))
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
            AdrEntryType::RenderSettingArray => {
                let (render_settings, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::RenderSettingArray { render_settings }, bytes_read))
            }
            AdrEntryType::AnimationArray => {
                let (animations, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationArray { animations }, bytes_read))
            }
            AdrEntryType::AnimationSoundArray => {
                let (sounds, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationSoundArray { sounds }, bytes_read))
            }
            AdrEntryType::AnimationParticleArray => {
                let (particles, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationParticleArray { particles }, bytes_read))
            }
            AdrEntryType::AnimationActionPoint => {
                let (action_points, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationActionPoint { action_points }, bytes_read))
            }
            AdrEntryType::Collision => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Collision { entries }, bytes_read))
            }
            AdrEntryType::Occlusion => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Occlusion { entries }, bytes_read))
            }
            AdrEntryType::Usage => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Usage { entries }, bytes_read))
            }
            AdrEntryType::HatHair => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::HatHair { entries }, bytes_read))
            }
            AdrEntryType::Shadow => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Shadow { entries }, bytes_read))
            }
            AdrEntryType::EquippedSlot => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::EquippedSlot { entries }, bytes_read))
            }
            AdrEntryType::BorrowedSkeleton => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    AdrData::BorrowedSkeleton { data },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AdrEntryType::Mount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Mount { entries }, bytes_read))
            }
            AdrEntryType::AnimationCompositeArray => {
                let (composites, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationCompositeArray { composites }, bytes_read))
            }
            AdrEntryType::LookControlArray => {
                let (look_controls, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::LookControlArray { look_controls }, bytes_read))
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
