use num_enum::TryFromPrimitive;
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};

use crate::{
    deserialize, deserialize_exact, deserialize_null_terminated_string, tell, Error, ErrorKind,
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

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SkeletonEntryType {
    AssetName = 1,
}

impl SkeletonEntryType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        SkeletonEntryType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum SkeletonData {
    AssetName { name: String },
}

pub struct SkeletonEntry {
    pub entry_type: SkeletonEntryType,
    pub len: i32,
    pub data: SkeletonData,
}

impl SkeletonEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = SkeletonEntryType::deserialize(file).await?;
        let len = deserialize_len(file).await?;
        let data = match entry_type {
            SkeletonEntryType::AssetName => SkeletonData::AssetName {
                name: deserialize_null_terminated_string(file).await?,
            },
        };

        Ok(SkeletonEntry {
            entry_type,
            len,
            data,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ModelEntryType {
    ModelAssetName = 1,
    MaterialAssetName = 2,
    Radius = 3,
}

impl ModelEntryType {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, BufReader::read_u8).await?;
        ModelEntryType::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into()),
            offset,
        })
    }
}

pub enum ModelData {
    ModelAssetName { name: String },
    MaterialAssetName { name: String },
    Radius { radius: f32 },
}

pub struct ModelEntry {
    pub entry_type: ModelEntryType,
    pub len: i32,
    pub data: ModelData,
}

impl ModelEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Self, Error> {
        let entry_type = ModelEntryType::deserialize(file).await?;
        let len = deserialize_len(file).await?;
        let data = match entry_type {
            ModelEntryType::ModelAssetName => ModelData::ModelAssetName {
                name: deserialize_null_terminated_string(file).await?,
            },
            ModelEntryType::MaterialAssetName => ModelData::MaterialAssetName {
                name: deserialize_null_terminated_string(file).await?,
            },
            ModelEntryType::Radius => ModelData::Radius {
                radius: deserialize(file, BufReader::read_f32).await?,
            },
        };

        Ok(ModelEntry {
            entry_type,
            len,
            data,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ParticleEntryType {
    EffectId = 1,
    EmitterName = 2,
    BoneName = 3,
    EffectAssetName = 10,
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
    Unknown = 1,
    ParticleEntry = 2,
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

pub struct Adr {}
