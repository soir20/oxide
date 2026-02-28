use std::{
    collections::{hash_map::IntoIter, HashMap},
    io::SeekFrom,
    path::{Path, PathBuf},
};

use tokio::io::AsyncReadExt;

use crate::{
    deserialize, deserialize_string, tell, u32_to_usize, Asset, AsyncReader, DeserializeAsset,
    Error,
};

pub struct PackAsset {
    pub offset: u64,
    pub size: u32,
    pub crc: u32,
}

pub struct Pack {
    path: PathBuf,
    assets: HashMap<String, PackAsset>,
}

impl DeserializeAsset for Pack {
    async fn deserialize<R: AsyncReader, P: AsRef<Path> + Send>(
        path: P,
        file: &mut R,
    ) -> Result<Self, Error> {
        let mut assets = HashMap::new();
        loop {
            let next_group_offset = deserialize(file, R::read_u32).await? as u64;
            let files_in_group = deserialize(file, R::read_u32).await?;

            for _ in 0..files_in_group {
                let name_len = deserialize(file, R::read_u32).await?;
                let (name, _) = deserialize_string(file, u32_to_usize(name_len)?).await?;

                let offset = deserialize(file, R::read_u32).await? as u64;
                let size = deserialize(file, R::read_u32).await?;
                let crc = deserialize(file, R::read_u32).await?;

                assets.insert(name, PackAsset { offset, size, crc });
            }

            if next_group_offset == 0 {
                break;
            }

            let offset = tell(file).await;
            if let Err(err) = file.seek(SeekFrom::Start(next_group_offset)).await {
                return Err(Error {
                    kind: err.into(),
                    offset,
                });
            }
        }

        Ok(Pack {
            path: path.as_ref().to_path_buf(),
            assets,
        })
    }
}

impl IntoIterator for Pack {
    type Item = (String, PackAsset);

    type IntoIter = IntoIter<String, PackAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.assets.into_iter()
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

    pub fn iter(&self) -> impl Iterator<Item = (&String, &PackAsset)> + use<'_> {
        self.assets.iter()
    }
}
