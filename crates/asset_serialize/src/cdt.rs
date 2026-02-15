use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
};

use crate::{
    bvh::BoundingVolumeHierarchy, deserialize, deserialize_exact, deserialize_string, skip,
    DeserializeAsset, Error, ErrorKind,
};

pub struct CollisionEntry {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u16; 3]>,
    pub bvh: BoundingVolumeHierarchy,
}

impl CollisionEntry {
    async fn deserialize(file: &mut BufReader<&mut File>) -> Result<Option<Self>, Error> {
        let should_skip_entry = deserialize(file, BufReader::read_i32_le).await?;
        if should_skip_entry > 0 {
            return Ok(None);
        }

        let vertex_count = deserialize(file, BufReader::read_i32_le).await?;
        let (vertex_buffer, _) =
            deserialize_exact(file, vertex_count.saturating_mul(12) as usize).await?;
        let ungrouped_vertices: Vec<f32> = vertex_buffer.chunks_exact(4)
            .map(TryInto::try_into)
            .map(Result::unwrap)
            .map(f32::from_le_bytes)
            .collect();
        let vertices = ungrouped_vertices.chunks_exact(3)
            .map(TryInto::try_into)
            .map(Result::unwrap)
            .collect();

        let triangle_count = deserialize(file, BufReader::read_i32_le).await?;
        let (triangle_buffer, _) =
            deserialize_exact(file, triangle_count.saturating_mul(6) as usize).await?;
        let ungrouped_triangles: Vec<u16> = triangle_buffer.chunks_exact(2)
            .map(TryInto::try_into)
            .map(Result::unwrap)
            .map(u16::from_le_bytes)
            .collect();
        let triangles = ungrouped_triangles.chunks_exact(3)
            .map(TryInto::try_into)
            .map(Result::unwrap)
            .collect();

        let _ = deserialize(file, BufReader::read_i32_le).await?;
        skip(file, 16).await?;
        let bvh = BoundingVolumeHierarchy::deserialize(file).await?;

        Ok(Some(CollisionEntry {
            vertices,
            triangles,
            bvh,
        }))
    }
}

pub struct Cdt {
    pub version: i32,
    pub collision_type: u32,
    pub enable_cursor: bool,
    pub enable_camera_collision: bool,
    pub entries: Vec<CollisionEntry>,
}

impl DeserializeAsset for Cdt {
    async fn deserialize<P: AsRef<std::path::Path> + Send>(
        _: P,
        file: &mut File,
    ) -> Result<Self, Error> {
        let mut reader = BufReader::new(file);

        let (magic, _) = deserialize_string(&mut reader, 4).await?;
        if magic != "CDTA" {
            return Err(Error {
                kind: ErrorKind::UnknownMagic(magic),
                offset: Some(0),
            });
        }

        let version = deserialize(&mut reader, BufReader::read_i32_le).await?;
        let packed_collision_type = deserialize(&mut reader, BufReader::read_u32_le).await?;
        let collision_type = packed_collision_type & 0b_0011_1111_1111_1111_1111_1111_1111_1111;
        let disable_cursor =
            packed_collision_type & 0b_0100_0000_0000_0000_0000_0000_0000_0000 != 0;
        let disable_camera_collision =
            packed_collision_type & 0b_1000_0000_0000_0000_0000_0000_0000_0000 != 0;

        let entry_count = deserialize(&mut reader, BufReader::read_i32_le).await?;
        let mut entries = Vec::new();
        for _ in 0..entry_count {
            if let Some(entry) = CollisionEntry::deserialize(&mut reader).await? {
                entries.push(entry);
            }
        }

        Ok(Cdt {
            version,
            collision_type,
            enable_cursor: !disable_cursor,
            enable_camera_collision: !disable_camera_collision,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use tokio::task::JoinSet;
    use walkdir::WalkDir;

    #[tokio::test]
    #[ignore]
    async fn test_deserialize_cdt() {
        let target_extension = "cdt";
        let search_path = env::var("CDT_ROOT").unwrap();

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
                let mut file = File::open(entry.path())
                    .await
                    .expect(&format!("Failed to open {}", entry.path().display()));
                Cdt::deserialize(entry.path(), &mut file)
                    .await
                    .expect(&format!("Failed to deserialize {}", entry.path().display()));
            });
        }

        jobs.join_all().await;
    }
}
