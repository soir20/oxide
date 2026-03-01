use std::io::Cursor;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::{
    deserialize, deserialize_exact, deserialize_string, i32_to_usize, is_eof, serialize,
    serialize_exact, serialize_string, tell, usize_to_i32, AsyncReader, AsyncWriter,
    DeserializeAsset, Error, ErrorKind,
};

async fn deserialize_len_with_bytes_read<W: AsyncSeekExt + AsyncReadExt + Unpin>(
    file: &mut W,
) -> Result<(i32, i32), Error> {
    let len_marker = deserialize(file, W::read_u8).await?;
    let mut len: i32 = len_marker.into();
    let mut bytes_read = 1;
    if len_marker >= 128 {
        if len_marker == 0xff {
            len = deserialize(file, W::read_i32).await?;
            bytes_read += 4;
        } else {
            let len_byte2 = deserialize(file, W::read_u8).await?;
            len = ((i32::from(len_marker) & 0b0111_1111) << 8) | i32::from(len_byte2);
            bytes_read += 1;
        }
    }

    if len < 0 {
        return Err(Error {
            kind: ErrorKind::NegativeLen(len),
            offset: tell(file)
                .await
                .map(|offset| offset.saturating_sub(bytes_read.try_into().unwrap_or_default())),
        });
    }

    Ok((len, bytes_read))
}

async fn serialize_len<W: AsyncWriter>(file: &mut W, len: i32) -> Result<i32, Error> {
    if len < 0 {
        return Err(Error {
            kind: ErrorKind::NegativeLen(len),
            offset: tell(file).await,
        });
    }

    if len >= 0x80 {
        let upper_byte = ((len & 0x7f00) >> 8) as u8 | 0x80;
        if upper_byte == 0xff {
            serialize(file, W::write_u8, 0xff).await?;
            serialize(file, W::write_i32, len).await?;
            Ok(5)
        } else {
            let lower_byte = (len & 0xff) as u8;
            serialize(file, W::write_u8, upper_byte).await?;
            serialize(file, W::write_u8, lower_byte).await?;
            Ok(2)
        }
    } else {
        serialize(file, W::write_u8, (len & 0xff) as u8).await?;
        Ok(1)
    }
}

trait DeserializeEntryType: Sized {
    fn deserialize<R: AsyncReader>(
        file: &mut R,
    ) -> impl std::future::Future<Output = Result<(Self, i32), Error>> + Send;
}

impl<T: TryFromPrimitive<Primitive = u8>> DeserializeEntryType for T {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<(Self, i32), Error> {
        let offset = tell(file).await;
        let value = deserialize(file, R::read_u8).await?;
        let entry_type = Self::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into(), T::NAME),
            offset,
        })?;

        Ok((entry_type, 1))
    }
}

trait SerializeEntryType: Sized {
    fn serialize<W: AsyncWriter>(
        &self,
        file: &mut W,
    ) -> impl std::future::Future<Output = Result<i32, Error>>;
}

impl<T: Copy + Into<u8> + Sync> SerializeEntryType for T {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        serialize(file, W::write_u8, (*self).into()).await?;
        Ok(1)
    }
}

trait DeserializeEntryData<T>: Sized {
    fn deserialize<R: AsyncReader>(
        entry_type: &T,
        entry_len: i32,
        file: &mut R,
    ) -> impl std::future::Future<Output = Result<(Self, i32), Error>> + Send;
}

trait SerializeEntryData: Sized {
    fn serialize<W: AsyncWriter>(
        &self,
        file: &mut W,
    ) -> impl std::future::Future<Output = Result<i32, Error>>;
}

trait DeserializeEntry: Sized {
    fn deserialize<R: AsyncReader>(
        file: &mut R,
    ) -> impl std::future::Future<Output = Result<(Self, i32), Error>> + Send;
}

trait SerializeEntry: Sized {
    fn serialize<W: AsyncWriter>(
        &self,
        file: &mut W,
    ) -> impl std::future::Future<Output = Result<i32, Error>>;
}
pub struct Entry<T, D> {
    pub entry_type: T,
    pub data: D,
}

fn checked_add_i32(values: &[i32], offset: Option<u64>) -> Result<i32, Error> {
    let mut sum: i32 = 0;
    for value in values.iter() {
        match sum.checked_add(*value) {
            Some(new_sum) => sum = new_sum,
            None => {
                return Err(Error {
                    kind: ErrorKind::IntegerOverflow {
                        expected_bytes: 4,
                        actual_bytes: 5,
                    },
                    offset,
                })
            }
        }
    }

    Ok(sum)
}

impl<T: DeserializeEntryType + Send, D: DeserializeEntryData<T> + Send> DeserializeEntry
    for Entry<T, D>
{
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<(Self, i32), Error> {
        let (entry_type, type_bytes_read) = T::deserialize(file).await?;
        let (len, len_bytes_read) = deserialize_len_with_bytes_read(file).await?;
        let (data, data_bytes_read) = D::deserialize(&entry_type, len, file).await?;

        let total_bytes_read = checked_add_i32(
            &[type_bytes_read, len_bytes_read, data_bytes_read],
            tell(file).await,
        )?;

        Ok((Entry { entry_type, data }, total_bytes_read))
    }
}

impl<T: SerializeEntryType, D: SerializeEntryData> SerializeEntry for Entry<T, D> {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        let mut bytes_written = self.entry_type.serialize(file).await?;

        let mut entry_data = Vec::new();
        let data_len = self
            .data
            .serialize(&mut Cursor::new(&mut entry_data))
            .await?;

        match bytes_written
            .checked_add(serialize_len(file, data_len).await?)
            .and_then(|bytes_written| bytes_written.checked_add(data_len))
        {
            Some(new_bytes_written) => bytes_written = new_bytes_written,
            None => {
                return Err(Error {
                    kind: ErrorKind::IntegerOverflow {
                        expected_bytes: 4,
                        actual_bytes: 5,
                    },
                    offset: tell(file).await,
                })
            }
        }

        serialize(file, W::write_all, &entry_data).await?;

        Ok(bytes_written)
    }
}

async fn deserialize_entries<R: AsyncReader, E: DeserializeEntry>(
    file: &mut R,
    len: i32,
) -> Result<(Vec<E>, i32), Error> {
    let mut entries = Vec::new();
    let mut bytes_read = 0;
    while bytes_read < len {
        let (entry, entry_bytes_read) = E::deserialize(file).await?;
        bytes_read = checked_add_i32(&[bytes_read, entry_bytes_read], tell(file).await)?;
        entries.push(entry);
    }

    Ok((entries, bytes_read))
}

async fn serialize_entries<W: AsyncWriter, E: SerializeEntry>(
    file: &mut W,
    entries: &[E],
) -> Result<i32, Error> {
    let mut bytes_written = 0;
    for entry in entries.iter() {
        bytes_written = checked_add_i32(
            &[bytes_written, entry.serialize(file).await?],
            tell(file).await,
        )?;
    }

    Ok(bytes_written)
}

async fn deserialize_f32_be<R: AsyncReader>(file: &mut R, len: i32) -> Result<(f32, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    data.resize(4, 0);
    Ok((
        f32::from_be_bytes(data.try_into().expect("data should contain 4 bytes")),
        usize_to_i32(bytes_read)?,
    ))
}

async fn serialize_f32_be<W: AsyncWriter>(file: &mut W, value: f32) -> Result<i32, Error> {
    if value == 0.0 {
        serialize(file, W::write_u8, 0).await?;
        Ok(1)
    } else {
        serialize(file, W::write_f32, value).await?;
        Ok(4)
    }
}

async fn check_int_overflow<R: AsyncReader>(
    file: &mut R,
    expected_bytes: usize,
    actual_bytes: usize,
) -> Result<(), Error> {
    if actual_bytes > expected_bytes {
        return Err(Error {
            kind: ErrorKind::IntegerOverflow {
                expected_bytes,
                actual_bytes,
            },
            offset: tell(file)
                .await
                .map(|offset| offset.saturating_sub(actual_bytes.try_into().unwrap_or_default())),
        });
    }

    Ok(())
}

async fn deserialize_u8<R: AsyncReader>(file: &mut R, len: i32) -> Result<(u8, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    check_int_overflow(file, 1, bytes_read).await?;
    data.resize(1, 0);
    Ok((data[0], usize_to_i32(bytes_read)?))
}

async fn serialize_u8<W: AsyncWriter>(file: &mut W, value: u8) -> Result<i32, Error> {
    serialize(file, W::write_u8, value).await?;
    Ok(1)
}

async fn deserialize_u32_le<R: AsyncReader>(file: &mut R, len: i32) -> Result<(u32, i32), Error> {
    let (mut data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
    check_int_overflow(file, 4, bytes_read).await?;
    data.resize(4, 0);
    Ok((
        u32::from_le_bytes(data.try_into().expect("data should contain 4 bytes")),
        usize_to_i32(bytes_read)?,
    ))
}

async fn serialize_u32_le<W: AsyncWriter>(file: &mut W, value: u32) -> Result<i32, Error> {
    let bytes = value.to_le_bytes();
    let bytes_to_write = if value > 0xffffff {
        4
    } else if value > 0xffff {
        3
    } else if value > 0xff {
        2
    } else {
        1
    };

    serialize(file, W::write_all, &bytes[0..bytes_to_write]).await?;

    Ok(bytes_to_write as i32)
}

async fn serialize_exact_i32<W: AsyncWriter>(file: &mut W, value: &[u8]) -> Result<i32, Error> {
    usize_to_i32(serialize_exact(file, value).await?)
}

async fn serialize_string_i32<W: AsyncWriter>(file: &mut W, value: &str) -> Result<i32, Error> {
    usize_to_i32(serialize_string(file, value).await?)
}

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &EntryCountEntryType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for EntryCountEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            EntryCountEntryData::EntryCount { entry_count } => {
                serialize_u32_le(file, *entry_count).await
            }
            EntryCountEntryData::EntryCount3 { entry_count } => {
                serialize_u32_le(file, *entry_count).await
            }
            EntryCountEntryData::EntryCount4 { entry_count } => {
                serialize_u32_le(file, *entry_count).await
            }
        }
    }
}

pub type EntryCountEntry = Entry<EntryCountEntryType, EntryCountEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &SkeletonEntryType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for SkeletonData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            SkeletonData::AssetName { name } => serialize_string_i32(file, name).await,
            SkeletonData::Scale { scale } => serialize_f32_be(file, *scale).await,
        }
    }
}

pub type SkeletonEntry = Entry<SkeletonEntryType, SkeletonData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    ObjectTerrainData { object_terrain_data_id: u32 },
}

impl DeserializeEntryData<ModelEntryType> for ModelData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &ModelEntryType,
        len: i32,
        file: &mut R,
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
                let (object_terrain_data_id, bytes_read) = deserialize_u32_le(file, len).await?;
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

impl SerializeEntryData for ModelData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            ModelData::ModelAssetName { name } => serialize_string_i32(file, name).await,
            ModelData::MaterialAssetName { name } => serialize_string_i32(file, name).await,
            ModelData::UpdateRadius { radius } => serialize_f32_be(file, *radius).await,
            ModelData::WaterDisplacementHeight { height } => serialize_f32_be(file, *height).await,
            ModelData::ObjectTerrainData {
                object_terrain_data_id,
            } => serialize_u32_le(file, *object_terrain_data_id).await,
        }
    }
}

pub type ModelEntry = Entry<ModelEntryType, ModelData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &SoundEmitterAssetEntryType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for SoundEmitterAssetEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            SoundEmitterAssetEntryData::AssetName { asset_name } => {
                serialize_string_i32(file, asset_name).await
            }
            SoundEmitterAssetEntryData::TimeOffset { time_offset_millis } => {
                serialize_f32_be(file, *time_offset_millis).await
            }
            SoundEmitterAssetEntryData::Weight { weight } => serialize_f32_be(file, *weight).await,
        }
    }
}

pub type SoundEmitterAssetEntry = Entry<SoundEmitterAssetEntryType, SoundEmitterAssetEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum SoundEmitterEntryType {
    Asset = 0x1,
    Id = 0x2,
    EmitterName = 0x3,
    BoneName = 0x4,
    Heading = 0x5,
    Pitch = 0x6,
    Scale = 0x7,
    OffsetX = 0x8,
    OffsetY = 0x9,
    OffsetZ = 0xa,
    ControlType = 0xb,
    PlayListType = 0xc,
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
    AttenuateReverbWithDistance = 0x18,
    MaxConcurrentSounds = 0x19,
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
        id: u32,
    },
    EmitterName {
        asset_name: String,
    },
    BoneName {
        bone_name: String,
    },
    Heading {
        heading: f32,
    },
    Pitch {
        pitch: f32,
    },
    Scale {
        scale: f32,
    },
    OffsetX {
        offset_x: f32,
    },
    OffsetY {
        offset_y: f32,
    },
    OffsetZ {
        offset_z: f32,
    },
    ControlType {
        control_type: u8,
    },
    PlayListType {
        play_list_type: u8,
    },
    PlayBackType {
        play_back_type: u8,
    },
    Category {
        category: u32,
    },
    SubCategory {
        sub_category: u32,
    },
    FadeTime {
        fade_time_millis: f32,
    },
    FadeOutTime {
        fade_out_time_millis: f32,
    },
    LoadType {
        load_type: u32,
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
    AttenuateReverbWithDistance {
        should_attenuate: bool,
    },
    MaxConcurrentSounds {
        max_concurrent_sounds: u32,
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &SoundEmitterEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SoundEmitterEntryType::Asset => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((SoundEmitterEntryData::Asset { entries }, bytes_read))
            }
            SoundEmitterEntryType::Id => {
                let (id, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((SoundEmitterEntryData::Id { id }, bytes_read))
            }
            SoundEmitterEntryType::EmitterName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    SoundEmitterEntryData::EmitterName { asset_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            SoundEmitterEntryType::BoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    SoundEmitterEntryData::BoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            SoundEmitterEntryType::Heading => {
                let (heading, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::Heading { heading }, bytes_read))
            }
            SoundEmitterEntryType::Pitch => {
                let (pitch, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::Pitch { pitch }, bytes_read))
            }
            SoundEmitterEntryType::Scale => {
                let (scale, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::Scale { scale }, bytes_read))
            }
            SoundEmitterEntryType::OffsetX => {
                let (offset_x, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::OffsetX { offset_x }, bytes_read))
            }
            SoundEmitterEntryType::OffsetY => {
                let (offset_y, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::OffsetY { offset_y }, bytes_read))
            }
            SoundEmitterEntryType::OffsetZ => {
                let (offset_z, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((SoundEmitterEntryData::OffsetZ { offset_z }, bytes_read))
            }
            SoundEmitterEntryType::ControlType => {
                let (control_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    SoundEmitterEntryData::ControlType { control_type },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::PlayListType => {
                let (play_list_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    SoundEmitterEntryData::PlayListType { play_list_type },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::PlayBackType => {
                let (play_back_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    SoundEmitterEntryData::PlayBackType { play_back_type },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::Category => {
                let (category, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((SoundEmitterEntryData::Category { category }, bytes_read))
            }
            SoundEmitterEntryType::SubCategory => {
                let (sub_category, bytes_read) = deserialize_u32_le(file, len).await?;
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
                let (load_type, bytes_read) = deserialize_u32_le(file, len).await?;
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
            SoundEmitterEntryType::AttenuateReverbWithDistance => {
                let (should_attenuate, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    SoundEmitterEntryData::AttenuateReverbWithDistance {
                        should_attenuate: should_attenuate != 0,
                    },
                    bytes_read,
                ))
            }
            SoundEmitterEntryType::MaxConcurrentSounds => {
                let (max_concurrent_sounds, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((
                    SoundEmitterEntryData::MaxConcurrentSounds {
                        max_concurrent_sounds,
                    },
                    bytes_read,
                ))
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

impl SerializeEntryData for SoundEmitterEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            SoundEmitterEntryData::Asset { entries } => serialize_entries(file, entries).await,
            SoundEmitterEntryData::Id { id } => serialize_u32_le(file, *id).await,
            SoundEmitterEntryData::EmitterName { asset_name } => {
                serialize_string_i32(file, asset_name).await
            }
            SoundEmitterEntryData::BoneName { bone_name } => {
                serialize_string_i32(file, bone_name).await
            }
            SoundEmitterEntryData::Heading { heading } => serialize_f32_be(file, *heading).await,
            SoundEmitterEntryData::Pitch { pitch } => serialize_f32_be(file, *pitch).await,
            SoundEmitterEntryData::Scale { scale } => serialize_f32_be(file, *scale).await,
            SoundEmitterEntryData::OffsetX { offset_x } => serialize_f32_be(file, *offset_x).await,
            SoundEmitterEntryData::OffsetY { offset_y } => serialize_f32_be(file, *offset_y).await,
            SoundEmitterEntryData::OffsetZ { offset_z } => serialize_f32_be(file, *offset_z).await,
            SoundEmitterEntryData::ControlType { control_type } => {
                serialize_u8(file, *control_type).await
            }
            SoundEmitterEntryData::PlayListType { play_list_type } => {
                serialize_u8(file, *play_list_type).await
            }
            SoundEmitterEntryData::PlayBackType { play_back_type } => {
                serialize_u8(file, *play_back_type).await
            }
            SoundEmitterEntryData::Category { category } => serialize_u32_le(file, *category).await,
            SoundEmitterEntryData::SubCategory { sub_category } => {
                serialize_u32_le(file, *sub_category).await
            }
            SoundEmitterEntryData::FadeTime { fade_time_millis } => {
                serialize_f32_be(file, *fade_time_millis).await
            }
            SoundEmitterEntryData::FadeOutTime {
                fade_out_time_millis,
            } => serialize_f32_be(file, *fade_out_time_millis).await,
            SoundEmitterEntryData::LoadType { load_type } => {
                serialize_u32_le(file, *load_type).await
            }
            SoundEmitterEntryData::Volume { volume } => serialize_f32_be(file, *volume).await,
            SoundEmitterEntryData::VolumeOffset { volume_offset } => {
                serialize_f32_be(file, *volume_offset).await
            }
            SoundEmitterEntryData::RateMultiplier { rate_multiplier } => {
                serialize_f32_be(file, *rate_multiplier).await
            }
            SoundEmitterEntryData::RateMultiplierOffset {
                rate_multiplier_offset,
            } => serialize_f32_be(file, *rate_multiplier_offset).await,
            SoundEmitterEntryData::RoomTypeScalar { room_type_scalar } => {
                serialize_f32_be(file, *room_type_scalar).await
            }
            SoundEmitterEntryData::AttenuateReverbWithDistance { should_attenuate } => {
                serialize_u8(file, (*should_attenuate).into()).await
            }
            SoundEmitterEntryData::MaxConcurrentSounds {
                max_concurrent_sounds,
            } => serialize_u32_le(file, *max_concurrent_sounds).await,
            SoundEmitterEntryData::AttenuationDistance { distance } => {
                serialize_f32_be(file, *distance).await
            }
            SoundEmitterEntryData::ClipDistance { clip_distance } => {
                serialize_f32_be(file, *clip_distance).await
            }
            SoundEmitterEntryData::DelayBetweenSounds {
                delay_between_sounds_millis,
            } => serialize_f32_be(file, *delay_between_sounds_millis).await,
            SoundEmitterEntryData::DelayBetweenSoundsOffset {
                delay_between_sounds_offset_millis,
            } => serialize_f32_be(file, *delay_between_sounds_offset_millis).await,
            SoundEmitterEntryData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type SoundEmitterEntry = Entry<SoundEmitterEntryType, SoundEmitterEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &SoundEmitterType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for SoundEmitterData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            SoundEmitterData::SoundEmitter { entries } => serialize_entries(file, entries).await,
            SoundEmitterData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type SoundEmitter = Entry<SoundEmitterType, SoundEmitterData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    Id { id: u32 },
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &ParticleEmitterEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleEmitterEntryType::Id => {
                let (id, bytes_read) = deserialize_u32_le(file, len).await?;
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

impl SerializeEntryData for ParticleEmitterEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            ParticleEmitterEntryData::Id { id } => serialize_u32_le(file, *id).await,
            ParticleEmitterEntryData::EmitterName { emitter_name } => {
                serialize_string_i32(file, emitter_name).await
            }
            ParticleEmitterEntryData::BoneName { bone_name } => {
                serialize_string_i32(file, bone_name).await
            }
            ParticleEmitterEntryData::Heading { heading } => serialize_f32_be(file, *heading).await,
            ParticleEmitterEntryData::Pitch { pitch } => serialize_f32_be(file, *pitch).await,
            ParticleEmitterEntryData::Scale { scale } => serialize_f32_be(file, *scale).await,
            ParticleEmitterEntryData::OffsetX { offset_x } => {
                serialize_f32_be(file, *offset_x).await
            }
            ParticleEmitterEntryData::OffsetY { offset_y } => {
                serialize_f32_be(file, *offset_y).await
            }
            ParticleEmitterEntryData::OffsetZ { offset_z } => {
                serialize_f32_be(file, *offset_z).await
            }
            ParticleEmitterEntryData::AssetName { asset_name } => {
                serialize_string_i32(file, asset_name).await
            }
            ParticleEmitterEntryData::SourceBoneName { bone_name } => {
                serialize_string_i32(file, bone_name).await
            }
            ParticleEmitterEntryData::LocalSpaceDerived {
                is_local_space_derived,
            } => serialize_u8(file, (*is_local_space_derived).into()).await,
            ParticleEmitterEntryData::WorldOrientation {
                use_world_orientation,
            } => serialize_u8(file, (*use_world_orientation).into()).await,
            ParticleEmitterEntryData::HardStop { is_hard_stop } => {
                serialize_u8(file, (*is_hard_stop).into()).await
            }
        }
    }
}

pub type ParticleEmitterEntry = Entry<ParticleEmitterEntryType, ParticleEmitterEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &ParticleEmitterType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for ParticleEmitterData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            ParticleEmitterData::ParticleEmitter { entries } => {
                serialize_entries(file, entries).await
            }
            ParticleEmitterData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type ParticleEmitter = Entry<ParticleEmitterType, ParticleEmitterData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum EffectDefinitionArrayType {
    SoundEmitterArray = 0x1,
    ParticleEmitterArray = 0x2,
}

pub enum EffectDefinitionArrayData {
    SoundEmitterArray {
        sound_emitters: Vec<SoundEmitter>,
    },
    ParticleEmitterArray {
        particle_emitters: Vec<ParticleEmitter>,
    },
}

impl DeserializeEntryData<EffectDefinitionArrayType> for EffectDefinitionArrayData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &EffectDefinitionArrayType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            EffectDefinitionArrayType::SoundEmitterArray => {
                let (sound_emitters, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    EffectDefinitionArrayData::SoundEmitterArray { sound_emitters },
                    bytes_read,
                ))
            }
            EffectDefinitionArrayType::ParticleEmitterArray => {
                let (particle_emitters, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    EffectDefinitionArrayData::ParticleEmitterArray { particle_emitters },
                    bytes_read,
                ))
            }
        }
    }
}

impl SerializeEntryData for EffectDefinitionArrayData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            EffectDefinitionArrayData::SoundEmitterArray { sound_emitters } => {
                serialize_entries(file, sound_emitters).await
            }
            EffectDefinitionArrayData::ParticleEmitterArray { particle_emitters } => {
                serialize_entries(file, particle_emitters).await
            }
        }
    }
}

pub type EffectDefinitionArray = Entry<EffectDefinitionArrayType, EffectDefinitionArrayData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum MaterialTagEntryType {
    Name = 0x1,
    MaterialIndex = 0x2,
    SemanticHash = 0x3,
    TintSetId = 0x4,
    DefaultTintId = 0x5,
}

pub enum MaterialTagEntryData {
    Name { name: String },
    MaterialIndex { material_index: u32 },
    SemanticHash { hash: u32 },
    TintSetId { tint_set_id: u32 },
    DefaultTintId { tint_id: u32 },
}

impl DeserializeEntryData<MaterialTagEntryType> for MaterialTagEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &MaterialTagEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MaterialTagEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MaterialTagEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MaterialTagEntryType::MaterialIndex => {
                let (material_index, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((
                    MaterialTagEntryData::MaterialIndex { material_index },
                    bytes_read,
                ))
            }
            MaterialTagEntryType::SemanticHash => {
                let (hash, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MaterialTagEntryData::SemanticHash { hash }, bytes_read))
            }
            MaterialTagEntryType::TintSetId => {
                let (tint_set_id, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MaterialTagEntryData::TintSetId { tint_set_id }, bytes_read))
            }
            MaterialTagEntryType::DefaultTintId => {
                let (tint_id, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MaterialTagEntryData::DefaultTintId { tint_id }, bytes_read))
            }
        }
    }
}

impl SerializeEntryData for MaterialTagEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            MaterialTagEntryData::Name { name } => serialize_string_i32(file, name).await,
            MaterialTagEntryData::MaterialIndex { material_index } => {
                serialize_u32_le(file, *material_index).await
            }
            MaterialTagEntryData::SemanticHash { hash } => serialize_u32_le(file, *hash).await,
            MaterialTagEntryData::TintSetId { tint_set_id } => {
                serialize_u32_le(file, *tint_set_id).await
            }
            MaterialTagEntryData::DefaultTintId { tint_id } => {
                serialize_u32_le(file, *tint_id).await
            }
        }
    }
}

pub type MaterialTagEntry = Entry<MaterialTagEntryType, MaterialTagEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &MaterialTagType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for MaterialTagData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            MaterialTagData::Material { entries } => serialize_entries(file, entries).await,
            MaterialTagData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type MaterialTag = Entry<MaterialTagType, MaterialTagData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum TextureAliasEntryType {
    ModelType = 0x1,
    MaterialIndex = 0x2,
    SemanticHash = 0x3,
    Name = 0x4,
    AssetName = 0x5,
    OcclusionBitMask = 0x6,
    IsDefault = 0x7,
}

pub enum TextureAliasEntryData {
    ModelType { model_type: u32 },
    MaterialIndex { material_index: u32 },
    SemanticHash { hash: u32 },
    Name { name: String },
    AssetName { asset_name: String },
    OcclusionBitMask { bit_mask: u32 },
    IsDefault { is_default: bool },
}

impl DeserializeEntryData<TextureAliasEntryType> for TextureAliasEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &TextureAliasEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            TextureAliasEntryType::ModelType => {
                let (model_type, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((TextureAliasEntryData::ModelType { model_type }, bytes_read))
            }
            TextureAliasEntryType::MaterialIndex => {
                let (material_index, bytes_read) = deserialize_u32_le(file, len).await?;
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
                let (bit_mask, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((
                    TextureAliasEntryData::OcclusionBitMask { bit_mask },
                    bytes_read,
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

impl SerializeEntryData for TextureAliasEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            TextureAliasEntryData::ModelType { model_type } => {
                serialize_u32_le(file, *model_type).await
            }
            TextureAliasEntryData::MaterialIndex { material_index } => {
                serialize_u32_le(file, *material_index).await
            }
            TextureAliasEntryData::SemanticHash { hash } => serialize_u32_le(file, *hash).await,
            TextureAliasEntryData::Name { name } => serialize_string_i32(file, name).await,
            TextureAliasEntryData::AssetName { asset_name } => {
                serialize_string_i32(file, asset_name).await
            }
            TextureAliasEntryData::OcclusionBitMask { bit_mask } => {
                serialize_u32_le(file, *bit_mask).await
            }
            TextureAliasEntryData::IsDefault { is_default } => {
                serialize_u8(file, (*is_default).into()).await
            }
        }
    }
}

pub type TextureAliasEntry = Entry<TextureAliasEntryType, TextureAliasEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &TextureAliasType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for TextureAliasData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            TextureAliasData::TextureAlias { entries } => serialize_entries(file, entries).await,
            TextureAliasData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type TextureAlias = Entry<TextureAliasType, TextureAliasData>;
#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum TintAliasEntryType {
    ModelType = 0x1,
    MaterialIndex = 0x2,
    SemanticHash = 0x3,
    Name = 0x4,
    Red = 0x5,
    Green = 0x6,
    Blue = 0x7,
    IsDefault = 0x8,
}

pub enum TintAliasEntryData {
    ModelType { model_type: u32 },
    MaterialIndex { material_index: u32 },
    SemanticHash { hash: u32 },
    Name { name: String },
    Red { red: f32 },
    Green { green: f32 },
    Blue { blue: f32 },
    IsDefault { is_default: bool },
}

impl DeserializeEntryData<TintAliasEntryType> for TintAliasEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &TintAliasEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            TintAliasEntryType::ModelType => {
                let (model_type, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((TintAliasEntryData::ModelType { model_type }, bytes_read))
            }
            TintAliasEntryType::MaterialIndex => {
                let (material_index, bytes_read) = deserialize_u32_le(file, len).await?;
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

impl SerializeEntryData for TintAliasEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            TintAliasEntryData::ModelType { model_type } => {
                serialize_u32_le(file, *model_type).await
            }
            TintAliasEntryData::MaterialIndex { material_index } => {
                serialize_u32_le(file, *material_index).await
            }
            TintAliasEntryData::SemanticHash { hash } => serialize_u32_le(file, *hash).await,
            TintAliasEntryData::Name { name } => serialize_string_i32(file, name).await,
            TintAliasEntryData::Red { red } => serialize_f32_be(file, *red).await,
            TintAliasEntryData::Green { green } => serialize_f32_be(file, *green).await,
            TintAliasEntryData::Blue { blue } => serialize_f32_be(file, *blue).await,
            TintAliasEntryData::IsDefault { is_default } => {
                serialize_u8(file, (*is_default).into()).await
            }
        }
    }
}

pub type TintAliasEntry = Entry<TintAliasEntryType, TintAliasEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &TintAliasType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for TintAliasData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            TintAliasData::TintAlias { entries } => serialize_entries(file, entries).await,
            TintAliasData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type TintAlias = Entry<TintAliasType, TintAliasData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    Id { id: u32 },
}

impl DeserializeEntryData<EffectEntryType> for EffectEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &EffectEntryType,
        len: i32,
        file: &mut R,
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
                let (id, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((EffectEntryData::Id { id }, bytes_read))
            }
        }
    }
}

impl SerializeEntryData for EffectEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            EffectEntryData::Type { effect_type } => serialize_u8(file, *effect_type).await,
            EffectEntryData::Name { name } => serialize_string_i32(file, name).await,
            EffectEntryData::ToolName { tool_name } => serialize_string_i32(file, tool_name).await,
            EffectEntryData::Id { id } => serialize_u32_le(file, *id).await,
        }
    }
}

pub type EffectEntry = Entry<EffectEntryType, EffectEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &EffectType,
        len: i32,
        file: &mut R,
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

impl SerializeEntryData for EffectData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            EffectData::Effect { entries } => serialize_entries(file, entries).await,
            EffectData::EntryCount { entries } => serialize_entries(file, entries).await,
        }
    }
}

pub type Effect = Entry<EffectType, EffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum LevelOfDetailAssetEntryType {
    AssetName = 0x1,
    Distance = 0x2,
}

pub enum LevelOfDetailAssetEntryData {
    AssetName { asset_name: String },
    Distance { distance: f32 },
}

impl DeserializeEntryData<LevelOfDetailAssetEntryType> for LevelOfDetailAssetEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &LevelOfDetailAssetEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            LevelOfDetailAssetEntryType::AssetName => {
                let (asset_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    LevelOfDetailAssetEntryData::AssetName { asset_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            LevelOfDetailAssetEntryType::Distance => {
                let (distance, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    LevelOfDetailAssetEntryData::Distance { distance },
                    bytes_read,
                ))
            }
        }
    }
}

impl SerializeEntryData for LevelOfDetailAssetEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            LevelOfDetailAssetEntryData::AssetName { asset_name } => {
                serialize_string_i32(file, asset_name).await
            }
            LevelOfDetailAssetEntryData::Distance { distance } => {
                serialize_f32_be(file, *distance).await
            }
        }
    }
}

pub type LevelOfDetailAssetEntry = Entry<LevelOfDetailAssetEntryType, LevelOfDetailAssetEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum LevelOfDetailEntryType {
    Asset = 0x1,
    Lod0aMaxDistanceFromCamera = 0x2,
}

pub enum LevelOfDetailEntryData {
    Asset {
        entries: Vec<LevelOfDetailAssetEntry>,
    },
    Lod0aMaxDistanceFromCamera {
        distance: f32,
    },
}

impl DeserializeEntryData<LevelOfDetailEntryType> for LevelOfDetailEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &LevelOfDetailEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            LevelOfDetailEntryType::Asset => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((LevelOfDetailEntryData::Asset { entries }, bytes_read))
            }
            LevelOfDetailEntryType::Lod0aMaxDistanceFromCamera => {
                let (distance, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((
                    LevelOfDetailEntryData::Lod0aMaxDistanceFromCamera { distance },
                    bytes_read,
                ))
            }
        }
    }
}

impl SerializeEntryData for LevelOfDetailEntryData {
    async fn serialize<W: AsyncWriter>(&self, file: &mut W) -> Result<i32, Error> {
        match self {
            LevelOfDetailEntryData::Asset { entries } => serialize_entries(file, entries).await,
            LevelOfDetailEntryData::Lod0aMaxDistanceFromCamera { distance } => {
                serialize_f32_be(file, *distance).await
            }
        }
    }
}

pub type LevelOfDetailEntry = Entry<LevelOfDetailEntryType, LevelOfDetailEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum LevelOfDetailType {
    LevelOfDetail = 0x1,
    EntryCount = 0xfe,
}

pub enum LevelOfDetailData {
    LevelOfDetail { entries: Vec<LevelOfDetailEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<LevelOfDetailType> for LevelOfDetailData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &LevelOfDetailType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            LevelOfDetailType::LevelOfDetail => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((LevelOfDetailData::LevelOfDetail { entries }, bytes_read))
            }
            LevelOfDetailType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((LevelOfDetailData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type LevelOfDetail = Entry<LevelOfDetailType, LevelOfDetailData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationLoadType {
    Required = 0x0,
    Preload = 0x1,
    OnDemand = 0x2,
    InheritFromParent = 0x3,
    RequiredFirst = 0x4,
}

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationEntryType {
    Name = 0x1,
    AssetName = 0x2,
    PlayBackScale = 0x3,
    Duration = 0x4,
    LoadType = 0x5,
    Required = 0x6,
    EffectsPersist = 0x7,
}

pub enum AnimationEntryData {
    Name { name: String },
    AssetName { name: String },
    PlayBackScale { scale: f32 },
    Duration { duration_seconds: f32 },
    LoadType { load_type: AnimationLoadType },
    Required { required: bool },
    EffectsPersist { do_effects_persist: bool },
}

impl DeserializeEntryData<AnimationEntryType> for AnimationEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationEntryType,
        len: i32,
        file: &mut R,
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
            AnimationEntryType::Required => {
                let (required, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationEntryData::Required {
                        required: required != 0,
                    },
                    bytes_read,
                ))
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationEffectTriggerEventType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationEffectEntryType {
    TriggerEventArray = 0x1,
    Type = 0x2,
    Name = 0x3,
    ToolName = 0x4,
    Id = 0x5,
    PlayOnce = 0x6,
    LoadType = 0x7,
    EntryCount = 0xfe,
}

pub enum AnimationEffectEntryData {
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
        id: u32,
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

impl DeserializeEntryData<AnimationEffectEntryType> for AnimationEffectEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationEffectEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationEffectEntryType::TriggerEventArray => {
                let (trigger_events, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationEffectEntryData::TriggerEventArray { trigger_events },
                    bytes_read,
                ))
            }
            AnimationEffectEntryType::Type => {
                let (effect_type, bytes_read) = deserialize_u8(file, len).await?;
                Ok((AnimationEffectEntryData::Type { effect_type }, bytes_read))
            }
            AnimationEffectEntryType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationEffectEntryData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationEffectEntryType::ToolName => {
                let (tool_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationEffectEntryData::ToolName { tool_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationEffectEntryType::Id => {
                let (id, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((AnimationEffectEntryData::Id { id }, bytes_read))
            }
            AnimationEffectEntryType::PlayOnce => {
                let (should_play_once, bytes_read) = deserialize_u8(file, len).await?;
                Ok((
                    AnimationEffectEntryData::PlayOnce {
                        should_play_once: should_play_once != 0,
                    },
                    bytes_read,
                ))
            }
            AnimationEffectEntryType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((AnimationEffectEntryData::LoadType { load_type }, bytes_read))
            }
            AnimationEffectEntryType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationEffectEntryData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationEffectEntry = Entry<AnimationEffectEntryType, AnimationEffectEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationEffectType {
    Effect = 0x1,
    Name = 0x2,
    EntryCount = 0xfe,
}

pub enum AnimationEffectData {
    Effect { entries: Vec<AnimationEffectEntry> },
    Name { name: String },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<AnimationEffectType> for AnimationEffectData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationEffectType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationEffectType::Effect => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationEffectData::Effect { entries }, bytes_read))
            }
            AnimationEffectType::Name => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationEffectData::Name { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationEffectType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationEffectData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationEffect = Entry<AnimationEffectType, AnimationEffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationSoundEffectType {
    EffectArray = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationSoundEffectData {
    EffectArray { effects: Vec<AnimationEffect> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<AnimationSoundEffectType> for AnimationSoundEffectData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationSoundEffectType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationSoundEffectType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationSoundEffectData::EffectArray { effects },
                    bytes_read,
                ))
            }
            AnimationSoundEffectType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationSoundEffectData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type AnimationSoundEffect = Entry<AnimationSoundEffectType, AnimationSoundEffectData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationParticleEffectType {
    EffectArray = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationParticleEffectData {
    EffectArray { effects: Vec<AnimationEffect> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<AnimationParticleEffectType> for AnimationParticleEffectData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationParticleEffectType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationParticleEffectType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationParticleEffectData::EffectArray { effects },
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &ActionPointEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &ActionPointType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationActionPointEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationActionPointType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum CollisionEntryType {
    AssetName = 0x1,
}

pub enum CollisionData {
    AssetName { name: String },
}

impl DeserializeEntryData<CollisionEntryType> for CollisionData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &CollisionEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum CoveredSlotEntryType {
    SlotId = 0x1,
}

pub enum CoveredSlotEntryData {
    SlotId { slot_id: u32 },
}

impl DeserializeEntryData<CoveredSlotEntryType> for CoveredSlotEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &CoveredSlotEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            CoveredSlotEntryType::SlotId => {
                let (bone_id, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((
                    CoveredSlotEntryData::SlotId { slot_id: bone_id },
                    bytes_read,
                ))
            }
        }
    }
}

pub type CoveredSlotEntry = Entry<CoveredSlotEntryType, CoveredSlotEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum OcclusionEntryType {
    SlotBitMask = 0x1,
    BitMask = 0x2,
    CoveredSlot = 0x4,
    EntryCount = 0xfe,
}

pub enum OcclusionData {
    SlotBitMask { bit_mask: u32 },
    BitMask { bit_mask: u32 },
    CoveredSlot { entries: Vec<CoveredSlotEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<OcclusionEntryType> for OcclusionData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &OcclusionEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            OcclusionEntryType::SlotBitMask => {
                let (bit_mask, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((OcclusionData::SlotBitMask { bit_mask }, bytes_read))
            }
            OcclusionEntryType::BitMask => {
                let (bit_mask, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((OcclusionData::BitMask { bit_mask }, bytes_read))
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &UsageEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &HatHairEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &ShadowEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    SlotId { slot_id: u32 },
    ParentAttachSlot { slot_name: String },
    ChildAttachSlot { slot_name: String },
    SlotName { slot_name: String },
}

impl DeserializeEntryData<EquippedSlotEntryType> for EquippedSlotEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &EquippedSlotEntryType,
        len: i32,
        file: &mut R,
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
                let (slot_id, bytes_read) = deserialize_u32_le(file, len).await?;
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BoneMetadataEntryType {
    BoneName = 0x1,
    CollisionType = 0x2,
    Joint1 = 0x3,
    Weight1 = 0x4,
    Joint2 = 0x5,
    Weight2 = 0x6,
}

pub enum BoneMetadataEntryData {
    BoneName { bone_name: String },
    CollisionType { collision_type: u32 },
    Joint1 { joint_name: String },
    Weight1 { weight: f32 },
    Joint2 { joint_name: String },
    Weight2 { weight: f32 },
}

impl DeserializeEntryData<BoneMetadataEntryType> for BoneMetadataEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &BoneMetadataEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            BoneMetadataEntryType::BoneName => {
                let (bone_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    BoneMetadataEntryData::BoneName { bone_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            BoneMetadataEntryType::CollisionType => {
                let (collision_type, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((
                    BoneMetadataEntryData::CollisionType { collision_type },
                    bytes_read,
                ))
            }
            BoneMetadataEntryType::Joint1 => {
                let (joint_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    BoneMetadataEntryData::Joint1 { joint_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            BoneMetadataEntryType::Weight1 => {
                let (weight, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((BoneMetadataEntryData::Weight1 { weight }, bytes_read))
            }
            BoneMetadataEntryType::Joint2 => {
                let (joint_name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    BoneMetadataEntryData::Joint2 { joint_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            BoneMetadataEntryType::Weight2 => {
                let (weight, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((BoneMetadataEntryData::Weight2 { weight }, bytes_read))
            }
        }
    }
}

pub type BoneMetadataEntry = Entry<BoneMetadataEntryType, BoneMetadataEntryData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BoneMetadataType {
    BoneMetadata = 0x1,
    EntryCount = 0xfe,
}

pub enum BoneMetadataData {
    BoneMetadata { entries: Vec<BoneMetadataEntry> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<BoneMetadataType> for BoneMetadataData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &BoneMetadataType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            BoneMetadataType::BoneMetadata => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((BoneMetadataData::BoneMetadata { entries }, bytes_read))
            }
            BoneMetadataType::EntryCount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((BoneMetadataData::EntryCount { entries }, bytes_read))
            }
        }
    }
}

pub type BoneMetadata = Entry<BoneMetadataType, BoneMetadataData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &MountSeatEntranceExitEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &MountSeatEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum MountEntryType {
    Seat = 0x1,
    MinOccupancy = 0x4,
    StandAnimation = 0x5,
    StandToSprintAnimation = 0x6,
    SprintAnimation = 0x7,
    SprintToStandAnimation = 0x8,
    AnimationPrefix = 0x9,
    EntryCount = 0xfe,
}

pub enum MountEntryData {
    Seat { entries: Vec<MountSeatEntry> },
    MinOccupancy { min_occupancy: u32 },
    StandAnimation { animation_name: String },
    StandToSprintAnimation { animation_name: String },
    SprintAnimation { animation_name: String },
    SprintToStandAnimation { animation_name: String },
    AnimationPrefix { animation_prefix: String },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<MountEntryType> for MountEntryData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &MountEntryType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            MountEntryType::Seat => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((MountEntryData::Seat { entries }, bytes_read))
            }
            MountEntryType::MinOccupancy => {
                let (min_occupancy, bytes_read) = deserialize_u32_le(file, len).await?;
                Ok((MountEntryData::MinOccupancy { min_occupancy }, bytes_read))
            }
            MountEntryType::StandAnimation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountEntryData::StandAnimation { animation_name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            MountEntryType::StandToSprintAnimation => {
                let (animation_name, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountEntryData::StandToSprintAnimation { animation_name },
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
            MountEntryType::AnimationPrefix => {
                let (animation_prefix, bytes_read) =
                    deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    MountEntryData::AnimationPrefix { animation_prefix },
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AnimationCompositeEffectType {
    EffectArray = 0x1,
    EntryCount = 0xfe,
}

pub enum AnimationCompositeEffectData {
    EffectArray { effects: Vec<AnimationEffect> },
    EntryCount { entries: Vec<EntryCountEntry> },
}

impl DeserializeEntryData<AnimationCompositeEffectType> for AnimationCompositeEffectData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AnimationCompositeEffectType,
        len: i32,
        file: &mut R,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationCompositeEffectType::EffectArray => {
                let (effects, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AnimationCompositeEffectData::EffectArray { effects },
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &JointEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
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
    async fn deserialize<R: AsyncReader>(
        entry_type: &LookControlEntryType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum LookControlType {
    LookControl = 0x1,
}

pub enum LookControlData {
    LookControl { entries: Vec<LookControlEntry> },
}

impl DeserializeEntryData<LookControlType> for LookControlData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &LookControlType,
        len: i32,
        file: &mut R,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AdrEntryType {
    Skeleton = 0x1,
    Model = 0x2,
    EffectDefinitionArrayArray = 0x3,
    MaterialTagArray = 0x4,
    TextureAliasArray = 0x5,
    TintAliasArray = 0x6,
    EffectArray = 0x7,
    LevelOfDetailArray = 0x8,
    AnimationArray = 0x9,
    AnimationSoundEffectArray = 0xa,
    AnimationParticleEffectArray = 0xb,
    AnimationActionPointArray = 0xc,
    Collision = 0xd,
    Occlusion = 0xe,
    Usage = 0xf,
    HatHair = 0x10,
    Shadow = 0x11,
    EquippedSlot = 0x12,
    BoneMetadataArray = 0x13,
    Mount = 0x14,
    AnimationCompositeEffectArray = 0x15,
    LookControlArray = 0x16,
}

pub enum AdrData {
    Skeleton {
        entries: Vec<SkeletonEntry>,
    },
    Model {
        entries: Vec<ModelEntry>,
    },
    EffectDefinitionArrayArray {
        arrays: Vec<EffectDefinitionArray>,
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
    LevelOfDetailArray {
        levels_of_detail: Vec<LevelOfDetail>,
    },
    AnimationArray {
        animations: Vec<Animation>,
    },
    AnimationSoundEffectArray {
        sounds: Vec<AnimationSoundEffect>,
    },
    AnimationParticleEffectArray {
        particles: Vec<AnimationParticleEffect>,
    },
    AnimationActionPointArray {
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
    BoneMetadataArray {
        bone_metadata: Vec<BoneMetadata>,
    },
    Mount {
        entries: Vec<MountEntry>,
    },
    AnimationCompositeEffectArray {
        composites: Vec<AnimationCompositeEffect>,
    },
    LookControlArray {
        look_controls: Vec<LookControl>,
    },
}

impl DeserializeEntryData<AdrEntryType> for AdrData {
    async fn deserialize<R: AsyncReader>(
        entry_type: &AdrEntryType,
        len: i32,
        file: &mut R,
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
            AdrEntryType::EffectDefinitionArrayArray => {
                let (arrays, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::EffectDefinitionArrayArray { arrays }, bytes_read))
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
            AdrEntryType::LevelOfDetailArray => {
                let (levels_of_detail, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::LevelOfDetailArray { levels_of_detail }, bytes_read))
            }
            AdrEntryType::AnimationArray => {
                let (animations, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationArray { animations }, bytes_read))
            }
            AdrEntryType::AnimationSoundEffectArray => {
                let (sounds, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::AnimationSoundEffectArray { sounds }, bytes_read))
            }
            AdrEntryType::AnimationParticleEffectArray => {
                let (particles, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AdrData::AnimationParticleEffectArray { particles },
                    bytes_read,
                ))
            }
            AdrEntryType::AnimationActionPointArray => {
                let (action_points, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AdrData::AnimationActionPointArray { action_points },
                    bytes_read,
                ))
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
            AdrEntryType::BoneMetadataArray => {
                let (bone_metadata, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::BoneMetadataArray { bone_metadata }, bytes_read))
            }
            AdrEntryType::Mount => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Mount { entries }, bytes_read))
            }
            AdrEntryType::AnimationCompositeEffectArray => {
                let (composites, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    AdrData::AnimationCompositeEffectArray { composites },
                    bytes_read,
                ))
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
    async fn deserialize<R: AsyncReader, P: AsRef<std::path::Path> + Send>(
        _: P,
        file: &mut R,
    ) -> Result<Self, Error> {
        let mut entries = Vec::new();
        while !is_eof(file).await? {
            let (entry, _) = AdrEntry::deserialize(file).await?;
            entries.push(entry);
        }

        Ok(Adr { entries })
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::io::Cursor;

    use super::*;
    use tokio::fs::File;
    use tokio::io::BufReader;
    use walkdir::WalkDir;

    #[tokio::test]
    async fn test_serialize_adr_len() {
        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 0)
            .await
            .unwrap();
        assert_eq!(vec![0x0], buffer);
        assert_eq!(
            (0, 1),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );

        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 1)
            .await
            .unwrap();
        assert_eq!(vec![0x1], buffer);
        assert_eq!(
            (1, 1),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );

        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 0x7f)
            .await
            .unwrap();
        assert_eq!(vec![0x7f], buffer);
        assert_eq!(
            (0x7f, 1),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );

        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 0xff)
            .await
            .unwrap();
        assert_eq!(vec![0x80, 0xff], buffer);
        assert_eq!(
            (0xff, 2),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );

        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 0x7eff)
            .await
            .unwrap();
        assert_eq!(vec![0xfe, 0xff], buffer);
        assert_eq!(
            (0x7eff, 2),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );

        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 0x7ffe)
            .await
            .unwrap();
        assert_eq!(vec![0xff, 0x0, 0x0, 0x7f, 0xfe], buffer);
        assert_eq!(
            (0x7ffe, 5),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );

        let mut buffer = Vec::new();
        serialize_len(&mut Cursor::new(&mut buffer), 0x7fff)
            .await
            .unwrap();
        assert_eq!(vec![0xff, 0x0, 0x0, 0x7f, 0xff], buffer);
        assert_eq!(
            (0x7fff, 5),
            deserialize_len_with_bytes_read(&mut Cursor::new(buffer))
                .await
                .unwrap()
        );
    }

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
            let file = File::open(entry.path())
                .await
                .expect(&format!("Failed to open {}", entry.path().display()));
            Adr::deserialize(entry.path(), &mut BufReader::new(file))
                .await
                .expect(&format!("Failed to deserialize {}", entry.path().display()));
        }
    }
}
