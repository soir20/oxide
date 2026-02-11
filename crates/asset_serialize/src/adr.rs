use num_enum::TryFromPrimitive;
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};

use crate::{
    deserialize, deserialize_exact, deserialize_string, is_eof, tell, DeserializeAsset, Error,
    ErrorKind,
};

async fn deserialize_len_with_bytes_read(
    file: &mut BufReader<&mut File>,
) -> Result<(i32, i32), Error> {
    let len_marker = deserialize(file, BufReader::read_u8).await?;
    let mut len: i32 = len_marker.into();
    let mut bytes_read = 1;
    if len_marker >= 128 {
        if len_marker == 0xff {
            len = deserialize(file, BufReader::read_i32_le).await?;
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

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SkeletonEntryType {
    AssetName = 0x1,
    Unknown = 0x2,
}

pub enum SkeletonData {
    AssetName { name: String },
    Unknown { data: Vec<u8> },
}

impl DeserializeEntryData<SkeletonEntryType> for SkeletonData {
    async fn deserialize(
        entry_type: &SkeletonEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SkeletonEntryType::AssetName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((SkeletonData::AssetName { name }, bytes_read as i32))
            }
            SkeletonEntryType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((SkeletonData::Unknown { data }, bytes_read as i32))
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
}

pub enum ModelData {
    ModelAssetName { name: String },
    MaterialAssetName { name: String },
    Radius { radius: f32 },
}

impl DeserializeEntryData<ModelEntryType> for ModelData {
    async fn deserialize(
        entry_type: &ModelEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ModelEntryType::ModelAssetName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((ModelData::ModelAssetName { name }, bytes_read as i32))
            }
            ModelEntryType::MaterialAssetName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((ModelData::MaterialAssetName { name }, bytes_read as i32))
            }
            ModelEntryType::Radius => {
                let radius = deserialize(file, BufReader::read_f32).await?;
                Ok((ModelData::Radius { radius }, 4))
            }
        }
    }
}

pub type ModelEntry = Entry<ModelEntryType, ModelData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEntryType {
    EffectId = 0x1,
    EmitterName = 0x2,
    BoneName = 0x3,
    Unknown1 = 0x5,
    Unknown2 = 0x6,
    Unknown3 = 0x7,
    Unknown4 = 0x8,
    Unknown5 = 0x9,
    EffectAssetName = 0xa,
    Unknown6 = 0xb,
    Unknown7 = 0xfe,
}

pub enum ParticleData {
    EffectId { effect_id: i32 },
    EmitterName { name: String },
    BoneName { name: String },
    Unknown1 { data: Vec<u8> },
    Unknown2 { data: Vec<u8> },
    Unknown3 { data: Vec<u8> },
    Unknown4 { data: Vec<u8> },
    Unknown5 { data: Vec<u8> },
    EffectAssetName { name: String },
    Unknown6 { data: Vec<u8> },
    Unknown7 { data: Vec<u8> },
}

impl DeserializeEntryData<ParticleEntryType> for ParticleData {
    async fn deserialize(
        entry_type: &ParticleEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleEntryType::EffectId => {
                let effect_id = deserialize(file, BufReader::read_i32_le).await?;
                Ok((ParticleData::EffectId { effect_id }, 4))
            }
            ParticleEntryType::EmitterName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((ParticleData::EmitterName { name }, bytes_read as i32))
            }
            ParticleEntryType::BoneName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((ParticleData::BoneName { name }, bytes_read as i32))
            }
            ParticleEntryType::Unknown1 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown1 { data }, bytes_read as i32))
            }
            ParticleEntryType::Unknown2 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown2 { data }, bytes_read as i32))
            }
            ParticleEntryType::Unknown3 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown3 { data }, bytes_read as i32))
            }
            ParticleEntryType::Unknown4 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown4 { data }, bytes_read as i32))
            }
            ParticleEntryType::Unknown5 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown5 { data }, bytes_read as i32))
            }
            ParticleEntryType::EffectAssetName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((ParticleData::EffectAssetName { name }, bytes_read as i32))
            }
            ParticleEntryType::Unknown6 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown6 { data }, bytes_read as i32))
            }
            ParticleEntryType::Unknown7 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleData::Unknown7 { data }, bytes_read as i32))
            }
        }
    }
}

pub type ParticleEntry = Entry<ParticleEntryType, ParticleData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleArrayType {
    Unknown = 0x1,
    ParticleEntry = 0x2,
}

pub enum ParticleArrayData {
    Unknown { data: Vec<u8> },
    ParticleEntry { entries: Vec<ParticleEntry> },
}

impl DeserializeEntryData<ParticleArrayType> for ParticleArrayData {
    async fn deserialize(
        entry_type: &ParticleArrayType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ParticleArrayType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((ParticleArrayData::Unknown { data }, bytes_read as i32))
            }
            ParticleArrayType::ParticleEntry => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((ParticleArrayData::ParticleEntry { entries }, bytes_read))
            }
        }
    }
}

pub type ParticleArray = Entry<ParticleArrayType, ParticleArrayData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationEntryType {
    AnimationName = 0x1,
    AssetName = 0x2,
    Duration = 0x4,
    LoadType = 0x5,
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationLoadType {
    Unknown1 = 0x0,
    Unknown2 = 0x1,
    Unknown3 = 0x2,
}

pub enum AnimationData {
    AnimationName { name: String },
    AssetName { name: String },
    Duration { duration_seconds: f32 },
    LoadType { load_type: AnimationLoadType },
}

impl DeserializeEntryData<AnimationEntryType> for AnimationData {
    async fn deserialize(
        entry_type: &AnimationEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AnimationEntryType::AnimationName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((AnimationData::AnimationName { name }, bytes_read as i32))
            }
            AnimationEntryType::AssetName => {
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((AnimationData::AssetName { name }, bytes_read as i32))
            }
            AnimationEntryType::Duration => {
                let duration_seconds = deserialize(file, BufReader::read_f32).await?;
                Ok((AnimationData::Duration { duration_seconds }, 4))
            }
            AnimationEntryType::LoadType => {
                let (load_type, bytes_read) = AnimationLoadType::deserialize(file).await?;
                Ok((AnimationData::LoadType { load_type }, bytes_read))
            }
        }
    }
}

pub type AnimationEntry = Entry<AnimationEntryType, AnimationData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationArrayType {
    AnimationEntry = 0x1,
    Unknown = 0xfe,
}

pub enum AnimationArrayData {
    AnimationEntry { entries: Vec<AnimationEntry> },
    Unknown { data: Vec<u8> },
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
            AnimationArrayType::Unknown => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AnimationArrayData::Unknown { data }, bytes_read as i32))
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
                let (name, bytes_read) = deserialize_string(file, len as usize).await?;
                Ok((CollisionData::AssetName { name }, bytes_read as i32))
            }
        }
    }
}

pub type CollisionEntry = Entry<CollisionEntryType, CollisionData>;

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AdrEntryType {
    Unknown1 = 0x0,
    Skeleton = 0x1,
    Model = 0x2,
    Particle = 0x3,
    Unknown2 = 0x5,
    Unknown3 = 0x6,
    Unknown4 = 0x7,
    Animation = 0x9,
    Unknown5 = 0xa,
    AnimatedParticle = 0xb,
    Unknown6 = 0xc,
    Collision = 0xd,
    Unknown7 = 0xf,
    Unknown8 = 0x12,
    Unknown9 = 0x15,
    Unknown10 = 0x16,
}

pub enum AdrData {
    Unknown1 { data: Vec<u8> },
    Skeleton { entries: Vec<SkeletonEntry> },
    Model { entries: Vec<ModelEntry> },
    Particle { entries: Vec<ParticleArray> },
    Unknown2 { data: Vec<u8> },
    Unknown3 { data: Vec<u8> },
    Unknown4 { data: Vec<u8> },
    Animation { entries: Vec<AnimationArray> },
    Unknown5 { data: Vec<u8> },
    AnimatedParticle { data: Vec<u8> },
    Unknown6 { data: Vec<u8> },
    Collision { entries: Vec<CollisionEntry> },
    Unknown7 { data: Vec<u8> },
    Unknown8 { data: Vec<u8> },
    Unknown9 { data: Vec<u8> },
    Unknown10 { data: Vec<u8> },
}

impl DeserializeEntryData<AdrEntryType> for AdrData {
    async fn deserialize(
        entry_type: &AdrEntryType,
        len: i32,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            AdrEntryType::Unknown1 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown1 { data }, bytes_read as i32))
            }
            AdrEntryType::Skeleton => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Skeleton { entries }, bytes_read))
            }
            AdrEntryType::Model => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Model { entries }, bytes_read))
            }
            AdrEntryType::Particle => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Particle { entries }, bytes_read))
            }
            AdrEntryType::Unknown2 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown2 { data }, bytes_read as i32))
            }
            AdrEntryType::Unknown3 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown3 { data }, bytes_read as i32))
            }
            AdrEntryType::Unknown4 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown4 { data }, bytes_read as i32))
            }
            AdrEntryType::Animation => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Animation { entries }, bytes_read))
            }
            AdrEntryType::Unknown5 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown5 { data }, bytes_read as i32))
            }
            AdrEntryType::AnimatedParticle => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::AnimatedParticle { data }, bytes_read as i32))
            }
            AdrEntryType::Unknown6 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown6 { data }, bytes_read as i32))
            }
            AdrEntryType::Collision => {
                let (entries, bytes_read) = deserialize_entries(file, len).await?;
                Ok((AdrData::Collision { entries }, bytes_read))
            }
            AdrEntryType::Unknown7 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown7 { data }, bytes_read as i32))
            }
            AdrEntryType::Unknown8 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown8 { data }, bytes_read as i32))
            }
            AdrEntryType::Unknown9 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown9 { data }, bytes_read as i32))
            }
            AdrEntryType::Unknown10 => {
                let (data, bytes_read) = deserialize_exact(file, len as usize).await?;
                Ok((AdrData::Unknown10 { data }, bytes_read as i32))
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
    //#[ignore]
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
