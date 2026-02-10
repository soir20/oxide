use num_enum::TryFromPrimitive;
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};

use crate::{
    deserialize, deserialize_exact, deserialize_null_terminated_string, is_eof, tell,
    DeserializeAsset, Error, ErrorKind,
};

async fn deserialize_len_with_bytes_read(
    file: &mut BufReader<&mut File>,
) -> Result<(i32, i32), Error> {
    let len_marker = deserialize(file, BufReader::read_i8).await?;
    let mut len: i32 = len_marker.into();
    let mut bytes_read = 1;
    if len_marker < 0 {
        if len_marker == -1 {
            len = deserialize(file, BufReader::read_i32_le).await?;
            bytes_read += 4;
        } else {
            let len_byte2 = deserialize(file, BufReader::read_i8).await?;
            len = ((i32::from(len_marker) & 0b0111_1111) << 8) | i32::from(len_byte2);
            bytes_read += 1;
        }
    }

    Ok((len, bytes_read))
}

async fn deserialize_len(file: &mut BufReader<&mut File>) -> Result<i32, Error> {
    deserialize_len_with_bytes_read(file)
        .await
        .map(|(len, _)| len)
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
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })?;

        Ok((entry_type, 1))
    }
}

trait DeserializeEntryData<T>: Sized {
    fn deserialize(
        entry_type: &T,
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
        let (data, data_bytes_read) = D::deserialize(&entry_type, file).await?;

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

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SkeletonEntryType {
    AssetName = 0x1,
}

pub enum SkeletonData {
    AssetName { name: String },
}

impl DeserializeEntryData<SkeletonEntryType> for SkeletonData {
    async fn deserialize(
        entry_type: &SkeletonEntryType,
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            SkeletonEntryType::AssetName => {
                let (name, bytes_read) = deserialize_null_terminated_string(file).await?;
                Ok((SkeletonData::AssetName { name }, bytes_read as i32))
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
        file: &mut BufReader<&mut File>,
    ) -> Result<(Self, i32), Error> {
        match entry_type {
            ModelEntryType::ModelAssetName => {
                let (name, bytes_read) = deserialize_null_terminated_string(file).await?;
                Ok((ModelData::ModelAssetName { name }, bytes_read as i32))
            }
            ModelEntryType::MaterialAssetName => {
                let (name, bytes_read) = deserialize_null_terminated_string(file).await?;
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
    EffectAssetName = 0xa,
}

impl ParticleEntryType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        ParticleEntryType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum ParticleData {
    EffectId { effect_id: i32 },
    EmitterName { name: String },
    BoneName { name: String },
    EffectAssetName { name: String },
}

pub struct ParticleEntry {
    pub entry_type: ParticleEntryType,
    pub len: i32,
    pub data: ParticleData,
    size: i32,
}

impl ParticleEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = ParticleEntryType::deserialize(file).await?;
        let (len, bytes_read) = deserialize_len_with_bytes_read(file).await?;
        let data = match entry_type {
            ParticleEntryType::EffectId => ParticleData::EffectId {
                effect_id: deserialize(file, BufReader::read_i32_le).await?,
            },
            ParticleEntryType::EmitterName => ParticleData::EmitterName {
                name: deserialize_null_terminated_string(file).await?,
            },
            ParticleEntryType::BoneName => ParticleData::BoneName {
                name: deserialize_null_terminated_string(file).await?,
            },
            ParticleEntryType::EffectAssetName => ParticleData::EffectAssetName {
                name: deserialize_null_terminated_string(file).await?,
            },
        };

        Ok(ParticleEntry {
            entry_type,
            len,
            data,
            size: bytes_read.saturating_add(len).saturating_add(1),
        })
    }

    fn size(&self) -> i32 {
        self.size
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleArrayType {
    Unknown = 0x1,
    ParticleEntry = 0x2,
}

impl ParticleArrayType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        ParticleArrayType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum ParticleArrayData {
    Unknown { data: Vec<u8> },
    ParticleEntry { entries: Vec<ParticleEntry> },
}

pub struct ParticleArray {
    pub entry_type: ParticleArrayType,
    pub len: i32,
    pub data: ParticleArrayData,
}

impl ParticleArray {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = ParticleArrayType::deserialize(file).await?;
        let len = deserialize_len(file).await?;
        let data = match entry_type {
            ParticleArrayType::Unknown => ParticleArrayData::Unknown {
                data: deserialize_exact(file, len as usize).await?,
            },
            ParticleArrayType::ParticleEntry => {
                let mut entries = Vec::new();
                let mut bytes_read = 0;
                while bytes_read < len {
                    let entry = ParticleEntry::deserialize(file).await?;
                    bytes_read = bytes_read.saturating_add(entry.size());
                    entries.push(entry);
                }

                ParticleArrayData::ParticleEntry { entries }
            }
        };

        Ok(ParticleArray {
            entry_type,
            len,
            data,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationEntryType {
    AnimationName = 0x1,
    AssetName = 0x2,
    Duration = 0x4,
    LoadType = 0x5,
}

impl AnimationEntryType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        AnimationEntryType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationLoadType {
    Unknown1 = 0x1,
    Unknown2 = 0x2,
}

impl AnimationLoadType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        AnimationLoadType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum AnimationData {
    AnimationName { name: String },
    AssetName { name: String },
    Duration { duration_seconds: f32 },
    LoadType { load_type: AnimationLoadType },
}

pub struct AnimationEntry {
    pub entry_type: AnimationEntryType,
    pub len: i32,
    pub data: AnimationData,
    size: i32,
}

impl AnimationEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = AnimationEntryType::deserialize(file).await?;
        let (len, bytes_read) = deserialize_len_with_bytes_read(file).await?;
        let data = match entry_type {
            AnimationEntryType::AnimationName => AnimationData::AnimationName {
                name: deserialize_null_terminated_string(file).await?,
            },
            AnimationEntryType::AssetName => AnimationData::AssetName {
                name: deserialize_null_terminated_string(file).await?,
            },
            AnimationEntryType::Duration => AnimationData::Duration {
                duration_seconds: deserialize(file, BufReader::read_f32).await?,
            },
            AnimationEntryType::LoadType => AnimationData::LoadType {
                load_type: AnimationLoadType::deserialize(file).await?,
            },
        };

        Ok(AnimationEntry {
            entry_type,
            len,
            data,
            size: bytes_read.saturating_add(len).saturating_add(1),
        })
    }

    fn size(&self) -> i32 {
        self.size
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AnimationArrayType {
    AnimationEntry = 0x1,
    Unknown = 0xfe,
}

impl AnimationArrayType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        AnimationArrayType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum AnimationArrayData {
    AnimationEntry { entries: Vec<AnimationEntry> },
    Unknown { data: Vec<u8> },
}

pub struct AnimationArray {
    pub entry_type: AnimationArrayType,
    pub len: i32,
    pub data: AnimationArrayData,
}

impl AnimationArray {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = AnimationArrayType::deserialize(file).await?;
        let len = deserialize_len(file).await?;
        let data = match entry_type {
            AnimationArrayType::AnimationEntry => {
                let mut entries = Vec::new();
                let mut bytes_read = 0;
                while bytes_read < len {
                    let entry = AnimationEntry::deserialize(file).await?;
                    bytes_read = bytes_read.saturating_add(entry.size());
                    entries.push(entry);
                }

                AnimationArrayData::AnimationEntry { entries }
            }
            AnimationArrayType::Unknown => AnimationArrayData::Unknown {
                data: deserialize_exact(file, len as usize).await?,
            },
        };

        Ok(AnimationArray {
            entry_type,
            len,
            data,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum CollisionEntryType {
    AssetName = 0x1,
}

impl CollisionEntryType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        CollisionEntryType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum CollisionData {
    AssetName { name: String },
}

pub struct CollisionEntry {
    pub entry_type: CollisionEntryType,
    pub len: i32,
    pub data: CollisionData,
}

impl CollisionEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = CollisionEntryType::deserialize(file).await?;
        let len = deserialize_len(file).await?;
        let data = match entry_type {
            CollisionEntryType::AssetName => CollisionData::AssetName {
                name: deserialize_null_terminated_string(file).await?,
            },
        };

        Ok(CollisionEntry {
            entry_type,
            len,
            data,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum AdrEntryType {
    Unknown = 0x0,
    Skeleton = 0x1,
    Model = 0x2,
    Particles = 0x3,
    Animations = 0x9,
    AnimatedParticles = 0xb,
    Collision = 0xd,
}

impl AdrEntryType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        AdrEntryType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum AdrData {
    Unknown { data: Vec<u8> },
    Skeleton { entry: SkeletonEntry },
    Model { entry: ModelEntry },
    Particles { entries: ParticleArray },
    Animations { entries: AnimationArray },
    AnimatedParticles { data: Vec<u8> },
    Collision { entry: CollisionEntry },
}

pub struct AdrEntry {
    pub entry_type: AdrEntryType,
    pub len: i32,
    pub data: AdrData,
}

impl AdrEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = AdrEntryType::deserialize(file).await?;
        let len = deserialize_len(file).await?;
        let data = match entry_type {
            AdrEntryType::Unknown => AdrData::Unknown {
                data: deserialize_exact(file, len as usize).await?,
            },
            AdrEntryType::Skeleton => AdrData::Skeleton {
                entry: SkeletonEntry::deserialize(file).await?,
            },
            AdrEntryType::Model => AdrData::Model {
                entry: ModelEntry::deserialize(file).await?,
            },
            AdrEntryType::Particles => AdrData::Particles {
                entries: ParticleArray::deserialize(file).await?,
            },
            AdrEntryType::Animations => AdrData::Animations {
                entries: AnimationArray::deserialize(file).await?,
            },
            AdrEntryType::AnimatedParticles => AdrData::AnimatedParticles {
                data: deserialize_exact(file, len as usize).await?,
            },
            AdrEntryType::Collision => AdrData::Collision {
                entry: CollisionEntry::deserialize(file).await?,
            },
        };

        Ok(AdrEntry {
            entry_type,
            len,
            data,
        })
    }
}

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
            entries.push(AdrEntry::deserialize(&mut reader).await?);
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
