use std::io::Cursor;

use async_compression::tokio::bufread::ZlibDecoder;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::{
    deserialize, deserialize_string, i32_to_usize, tell, AsyncReader, DeserializeAsset, Error,
    ErrorKind,
};

#[derive(Serialize, Deserialize)]
pub struct TerrainChunk {}

impl TerrainChunk {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        Ok(TerrainChunk {})
    }
}

async fn decompress_section<R: AsyncReader>(file: &mut R) -> Result<Vec<u8>, Error> {
    let decompressed_len = deserialize(file, R::read_i32_le).await?;
    let compressed_len = deserialize(file, R::read_i32_le).await?;

    let offset = tell(file).await;
    let mut buffer = Vec::with_capacity(i32_to_usize(compressed_len)?);
    let mut decoder = ZlibDecoder::new(&mut *file);

    decoder.read_exact(&mut buffer).await.map_err(|err| Error {
        kind: err.into(),
        offset,
    })?;

    Ok(buffer)
}

#[derive(Serialize, Deserialize)]
pub struct Gcnk {
    pub version: i32,
    pub chunk: TerrainChunk,
}

impl DeserializeAsset for Gcnk {
    async fn deserialize<R: AsyncReader, P: AsRef<std::path::Path> + Send>(
        _: P,
        file: &mut R,
    ) -> Result<Self, Error> {
        let (magic, _) = deserialize_string(file, 4).await?;
        if magic != "GCNK" {
            return Err(Error {
                kind: ErrorKind::UnknownMagic(magic),
                offset: Some(0),
            });
        }

        let version = deserialize(file, R::read_i32_le).await?;

        let chunk_buffer = decompress_section(file).await?;
        let chunk = TerrainChunk::deserialize(&mut Cursor::new(chunk_buffer)).await?;

        let collision_buffer = decompress_section(file).await?;
        //let chunk = TerrainChunk::deserialize(&mut Cursor::new(collision_buffer)).await?;

        Ok(Gcnk { version, chunk })
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use tokio::fs::File;
    use tokio::io::BufReader;
    use tokio::task::JoinSet;
    use walkdir::WalkDir;

    #[tokio::test]
    #[ignore]
    async fn test_deserialize_gcnk() {
        let target_extension = "gcnk";
        let search_path = env::var("GCNK_ROOT").unwrap();

        let mut jobs = JoinSet::new();
        for entry in WalkDir::new(search_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == target_extension)
            })
        {
            jobs.spawn(async move {
                let file = File::open(entry.path())
                    .await
                    .expect(&format!("Failed to open {}", entry.path().display()));
                <Gcnk as DeserializeAsset>::deserialize(entry.path(), &mut BufReader::new(file))
                    .await
                    .expect(&format!("Failed to deserialize {}", entry.path().display()));
            });
        }

        jobs.join_all().await;
    }
}
