use num_enum::TryFromPrimitive;
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};

use crate::{deserialize, deserialize_null_terminated_string, tell, Error, ErrorKind};

async fn deserialize_len(file: &mut BufReader<&mut File>) -> Result<i32, Error> {
    let len_marker = deserialize(file, BufReader::read_i8).await?;
    let mut len: i32 = len_marker.into();
    if len_marker < 0 {
        if len_marker == -1 {
            len = deserialize(file, BufReader::read_i32_le).await?;
        } else {
            let len_byte2 = deserialize(file, BufReader::read_i8).await?;
            len = ((i32::from(len_marker) & 0b0111_1111) << 8) | i32::from(len_byte2);
        }
    }

    Ok(len)
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

pub struct Adr {}
