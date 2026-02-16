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
    pub len: i32,
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

        Ok((
            Entry {
                entry_type,
                len,
                data,
            },
            total_bytes_read,
        ))
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
    Unknown7 = 0xfe,
}

pub enum ParticleEmitterEntryData {
    EffectId { effect_id: i32 },
    Unknown7 { data: Vec<u8> },
}

impl DeserializeEntryData<ParticleEmitterEntryType> for ParticleEmitterEntryData {
    async fn deserialize(
        entry_type: &ParticleEmitterEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleEmitterEntryType::EffectId => {
                let (mut effect_id_bytes, bytes_read) =
                    deserialize_exact(file, i32_to_usize(len)?).await?;
                effect_id_bytes.resize(4, 0);
                let effect_id = i32::from_le_bytes(
                    effect_id_bytes
                        .try_into()
                        .expect("effect_id_bytes should contain 4 bytes"),
                );
                Ok((
                    ParticleEmitterEntryData::EffectId { effect_id },
                    usize_to_i32(bytes_read)?,
                ))
            }
            ParticleEmitterEntryType::Unknown7 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    ParticleEmitterEntryData::Unknown7 { data },
                    usize_to_i32(bytes_read)?,
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
}

pub enum ParticleEmitterData {
    ParticleEmitter { entries: Vec<ParticleEmitterEntry> },
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
        }
    }
}

pub type ParticleEmitter = Entry<ParticleEmitterType, ParticleEmitterData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEmitterArrayType {
    Unknown = 0x1,
    ParticleEntry = 0x2,
}

pub enum ParticleEmitterArrayData {
    Unknown { data: Vec<u8> },
    ParticleEntry { entries: Vec<ParticleEmitter> },
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
            ParticleEmitterArrayType::ParticleEntry => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((
                    ParticleEmitterArrayData::ParticleEntry { entries },
                    bytes_read,
                ))
            }
        }
    }
}

pub type ParticleEmitterArray = Entry<ParticleEmitterArrayType, ParticleEmitterArrayData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationEntryType {
    AnimationName = 0x1,
    AssetName = 0x2,
    Unknown1 = 0x3,
    Duration = 0x4,
    LoadType = 0x5,
    Unknown2 = 0x7,
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationLoadType {
    Unknown1 = 0x0,
    Unknown2 = 0x1,
    Unknown3 = 0x2,
    Unknown4 = 0x4,
}

pub enum AnimationData {
    AnimationName { name: String },
    AssetName { name: String },
    Unknown1 { data: Vec<u8> },
    Duration { duration_seconds: f32 },
    LoadType { load_type: AnimationLoadType },
    Unknown2 { data: Vec<u8> },
}

impl DeserializeEntryData<AnimationEntryType> for AnimationData {
    async fn deserialize(
        entry_type: &AnimationEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationEntryType::AnimationName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationData::AnimationName { name },
                    usize_to_i32(bytes_read)?,
                ))
            }
            AnimationEntryType::AssetName => {
                let (name, bytes_read) = deserialize_string(file, i32_to_usize(len)?).await?;
                Ok((AnimationData::AssetName { name }, usize_to_i32(bytes_read)?))
            }
            AnimationEntryType::Unknown1 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AnimationData::Unknown1 { data }, usize_to_i32(bytes_read)?))
            }
            AnimationEntryType::Duration => {
                let (duration_seconds, bytes_read) = deserialize_f32_be(file, len).await?;
                Ok((AnimationData::Duration { duration_seconds }, bytes_read))
            }
            AnimationEntryType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((AnimationData::LoadType { load_type }, bytes_read))
            }
            AnimationEntryType::Unknown2 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AnimationData::Unknown2 { data }, usize_to_i32(bytes_read)?))
            }
        }
    }
}

pub type AnimationEntry = Entry<AnimationEntryType, AnimationData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationArrayType {
    AnimationEntry = 0x1,
    Unknown2 = 0xfe,
}

pub enum AnimationArrayData {
    AnimationEntry { entries: Vec<AnimationEntry> },
    Unknown2 { data: Vec<u8> },
}

impl DeserializeEntryData<AnimationArrayType> for AnimationArrayData {
    async fn deserialize(
        entry_type: &AnimationArrayType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationArrayType::AnimationEntry => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AnimationArrayData::AnimationEntry { entries }, bytes_read))
            }
            AnimationArrayType::Unknown2 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((
                    AnimationArrayData::Unknown2 { data },
                    usize_to_i32(bytes_read)?,
                ))
            }
        }
    }
}

pub type AnimationArray = Entry<AnimationArrayType, AnimationArrayData>;

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
    ParticleEmitterArray = 0x3,
    Unknown2 = 0x4,
    Unknown3 = 0x5,
    Unknown4 = 0x6,
    Unknown5 = 0x7,
    Unknown6 = 0x8,
    Animation = 0x9,
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
    ParticleEmitterArray { entries: Vec<ParticleEmitterArray> },
    Unknown2 { data: Vec<u8> },
    Unknown3 { data: Vec<u8> },
    Unknown4 { data: Vec<u8> },
    Unknown5 { data: Vec<u8> },
    Unknown6 { data: Vec<u8> },
    Animation { entries: Vec<AnimationArray> },
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
            AdrEntryType::ParticleEmitterArray => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::ParticleEmitterArray { entries }, bytes_read))
            }
            AdrEntryType::Unknown2 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown2 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown3 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown3 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown4 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown4 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown5 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown5 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Unknown6 => {
                let (data, bytes_read) = deserialize_exact(file, i32_to_usize(len)?).await?;
                Ok((AdrData::Unknown6 { data }, usize_to_i32(bytes_read)?))
            }
            AdrEntryType::Animation => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Animation { entries }, bytes_read))
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
