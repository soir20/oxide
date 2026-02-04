use std::{
    collections::HashMap,
    io::{ErrorKind, SeekFrom},
    path::PathBuf,
};

use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

use crate::{Asset, DeserializeAsset};

struct PackAsset {
    offset: u64,
    size: u32,
    crc: u32,
}

pub struct Pack {
    path: PathBuf,
    assets: HashMap<String, PackAsset>,
}

impl DeserializeAsset for Pack {
    async fn deserialize(path: PathBuf, file: &mut File) -> Result<Self, tokio::io::Error> {
        let mut assets = HashMap::new();
        loop {
            let next_group_offset = file.read_u32().await? as u64;
            let files_in_group = file.read_u32().await?;

            for _ in 0..files_in_group {
                let name_len = file.read_u32().await?;
                let mut name_buffer = vec![0; name_len as usize];
                file.read_exact(&mut name_buffer).await?;
                let name = String::from_utf8(name_buffer).map_err(|_| ErrorKind::InvalidData)?;

                let offset = file.read_u32().await? as u64;
                let size = file.read_u32().await?;
                let crc = file.read_u32().await?;

                assets.insert(name, PackAsset { offset, size, crc });
            }

            if next_group_offset == 0 {
                break;
            }

            file.seek(SeekFrom::Start(next_group_offset)).await?;
        }

        Ok(Pack { path, assets })
    }
}

impl Pack {
    pub fn flatten(self) -> HashMap<String, Asset> {
        self.assets
            .into_iter()
            .map(|(name, asset)| {
                (
                    name,
                    Asset {
                        path: self.path.clone(),
                        offset: asset.offset,
                    },
                )
            })
            .collect()
    }
}
