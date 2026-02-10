use std::{
    collections::{hash_map::IntoIter, HashMap},
    io::SeekFrom,
    path::{Path, PathBuf},
};

use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, BufReader},
};

use crate::{deserialize, deserialize_string, tell, Asset, DeserializeAsset, Error};

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
    async fn deserialize<P: AsRef<Path> + Send>(path: P, file: &mut File) -> Result<Self, Error> {
        let mut reader = BufReader::new(file);
        let mut assets = HashMap::new();
        loop {
            let next_group_offset = deserialize(&mut reader, BufReader::read_u32).await? as u64;
            let files_in_group = deserialize(&mut reader, BufReader::read_u32).await?;

            for _ in 0..files_in_group {
                let name_len = deserialize(&mut reader, BufReader::read_u32).await?;
                let (name, _) = deserialize_string(&mut reader, name_len as usize).await?;

                let offset = deserialize(&mut reader, BufReader::read_u32).await? as u64;
                let size = deserialize(&mut reader, BufReader::read_u32).await?;
                let crc = deserialize(&mut reader, BufReader::read_u32).await?;

                assets.insert(name, PackAsset { offset, size, crc });
            }

            if next_group_offset == 0 {
                break;
            }

            let offset = tell(&mut reader).await;
            if let Err(err) = reader.seek(SeekFrom::Start(next_group_offset)).await {
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
