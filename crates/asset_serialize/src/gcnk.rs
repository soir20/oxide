use std::io::Cursor;

use async_compression::tokio::bufread::ZlibDecoder;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::{
    bvh::BoundingVolumeHierarchy, deserialize, deserialize_exact, deserialize_f32_le_vec3,
    deserialize_f32_le_vec4, deserialize_null_terminated_string, deserialize_string, i32_to_u64,
    i32_to_usize, is_eof, skip, tell, AsyncReader, DeserializeAsset, Error, ErrorKind,
};

async fn deserialize_vec<R: AsyncReader, T>(
    file: &mut R,
    version: i32,
    mut fun: impl AsyncFnMut(&mut R, i32) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    let len: i32 = deserialize(file, R::read_i32_le).await?;
    let mut items = Vec::new();
    for _ in 0..len {
        items.push(fun(file, version).await?);
    }

    Ok(items)
}

async fn deserialize_u16<R: AsyncReader>(file: &mut R, _: i32) -> Result<u16, Error> {
    deserialize(file, R::read_u16_le).await
}

async fn deserialize_i32<R: AsyncReader>(file: &mut R, _: i32) -> Result<i32, Error> {
    deserialize(file, R::read_i32_le).await
}

#[derive(Serialize, Deserialize)]
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba8 {
    async fn deserialize<R: AsyncReader>(file: &mut R, _: i32) -> Result<Self, Error> {
        Ok(Rgba8 {
            red: deserialize(file, R::read_u8).await?,
            green: deserialize(file, R::read_u8).await?,
            blue: deserialize(file, R::read_u8).await?,
            alpha: deserialize(file, R::read_u8).await?,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct SubMeshBakedLighting {
    pub vertex_colors: Vec<Rgba8>,
}

impl SubMeshBakedLighting {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let vertex_colors = deserialize_vec(file, version, Rgba8::deserialize).await?;
        Ok(SubMeshBakedLighting { vertex_colors })
    }
}

#[derive(Serialize, Deserialize)]
pub struct RuntimeObjectTint {
    pub tint_alias: String,
    pub tint: [f32; 4],
}

impl RuntimeObjectTint {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Option<Self>, Error> {
        let (tint_alias, _) = deserialize_null_terminated_string(file).await?;
        if tint_alias.is_empty() {
            return Ok(None);
        }

        let tint = deserialize_f32_le_vec4(file).await?;

        Ok(Some(RuntimeObjectTint { tint_alias, tint }))
    }
}

#[derive(Serialize, Deserialize)]
pub enum TerrainObjectIdentifier {
    Id(u32),
    Name(String),
}

impl TerrainObjectIdentifier {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        if version >= 5 {
            Ok(TerrainObjectIdentifier::Id(
                deserialize(file, R::read_u32_le).await?,
            ))
        } else {
            Ok(TerrainObjectIdentifier::Name(
                deserialize_null_terminated_string(file).await?.0,
            ))
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct RuntimeObject {
    pub guid: i32,
    pub adr_name: String,
    pub unknown: String,
    pub pos: [f32; 4],
    pub rot: [f32; 4],
    pub scale: f32,
    pub texture_alias: Option<String>,
    pub tint: Option<RuntimeObjectTint>,
    pub terrain_object_identifier: TerrainObjectIdentifier,
    pub min_render_radius: f32,
    pub baked_lighting: Vec<SubMeshBakedLighting>,
}

impl RuntimeObject {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let guid = deserialize(file, R::read_i32_le).await?;
        let (adr_name, _) = deserialize_null_terminated_string(file).await?;
        let (unknown, _) = deserialize_null_terminated_string(file).await?;
        let pos = deserialize_f32_le_vec4(file).await?;
        let rot = deserialize_f32_le_vec4(file).await?;
        let scale = deserialize(file, R::read_f32_le).await?;

        let mut texture_alias = None;
        let mut tint = None;
        if version >= 6 {
            let (alias, _) = deserialize_null_terminated_string(file).await?;
            if !alias.is_empty() {
                texture_alias = Some(alias);
            }
            tint = RuntimeObjectTint::deserialize(file).await?;
        }

        skip(file, 4).await?;

        let terrain_object_identifier = TerrainObjectIdentifier::deserialize(file, version).await?;
        let min_render_radius = deserialize(file, R::read_f32_le).await?;

        let mut baked_lighting = Vec::new();
        if version >= 3 {
            baked_lighting =
                deserialize_vec(file, version, SubMeshBakedLighting::deserialize).await?;
        }

        Ok(RuntimeObject {
            guid,
            adr_name,
            unknown,
            pos,
            rot,
            scale,
            texture_alias,
            tint,
            terrain_object_identifier,
            min_render_radius,
            baked_lighting,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct RawLight {
    pub name: String,
    pub color_name: String,
    pub light_type: u8,
    pub pos: [f32; 4],
    pub range: f32,
    pub intensity: f32,
    pub color: Rgba8,
}

impl RawLight {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let (name, _) = deserialize_null_terminated_string(file).await?;
        let (color_name, _) = deserialize_null_terminated_string(file).await?;
        let light_type = deserialize(file, R::read_u8).await?;
        let pos = deserialize_f32_le_vec4(file).await?;
        let range = deserialize(file, R::read_f32_le).await?;
        let intensity = deserialize(file, R::read_f32_le).await?;
        let color = Rgba8::deserialize(file, version).await?;

        Ok(RawLight {
            name,
            color_name,
            light_type,
            pos,
            range,
            intensity,
            color,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct RawArea {
    pub name: String,
    pub unknown1: i32,
    pub unknown2: String,
    pub pos: [f32; 4],
    pub rot: [f32; 4],
    pub scale: f32,
    pub dimensions: [f32; 3],
}

impl RawArea {
    async fn deserialize<R: AsyncReader>(file: &mut R, _: i32) -> Result<Self, Error> {
        let (name, _) = deserialize_null_terminated_string(file).await?;
        let unknown1 = deserialize(file, R::read_i32_le).await?;
        let (unknown2, _) = deserialize_null_terminated_string(file).await?;
        let pos = deserialize_f32_le_vec4(file).await?;
        let rot = deserialize_f32_le_vec4(file).await?;
        let scale = deserialize(file, R::read_f32_le).await?;
        let dimensions = deserialize_f32_le_vec3(file).await?;

        Ok(RawArea {
            name,
            unknown1,
            unknown2,
            pos,
            rot,
            scale,
            dimensions,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct RawGroup {
    pub name: String,
    pub pos: [f32; 4],
    pub rot: [f32; 4],
    pub scale: f32,
}

impl RawGroup {
    async fn deserialize<R: AsyncReader>(file: &mut R, _: i32) -> Result<Self, Error> {
        let (name, _) = deserialize_null_terminated_string(file).await?;
        let pos = deserialize_f32_le_vec4(file).await?;
        let rot = deserialize_f32_le_vec4(file).await?;
        let scale = deserialize(file, R::read_f32_le).await?;

        Ok(RawGroup {
            name,
            pos,
            rot,
            scale,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct TileUnknown {
    pub unknown1: i32,
    pub unknown2: i32,
    pub unknown3: i32,
    pub unknown4: i32,
    pub unknown5: i32,
}

impl TileUnknown {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Option<Self>, Error> {
        let unknown1 = deserialize(file, R::read_i32_le).await?;
        if unknown1 <= 0 {
            return Ok(None);
        }

        let unknown2 = deserialize(file, R::read_i32_le).await?;
        let unknown3 = deserialize(file, R::read_i32_le).await?;
        let unknown4 = deserialize(file, R::read_i32_le).await?;
        let unknown5 = deserialize(file, R::read_i32_le).await?;

        Ok(Some(TileUnknown {
            unknown1,
            unknown2,
            unknown3,
            unknown4,
            unknown5,
        }))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
    pub pos: [f32; 4],
    pub unknown1: Option<TileUnknown>,
    pub unknown2: f32,
    pub eco_data: Vec<i32>,
    pub runtime_objects: Vec<RuntimeObject>,
    pub lights: Vec<RawLight>,
    pub areas: Vec<RawArea>,
    pub groups: Vec<RawGroup>,
    pub index: i32,
}

impl Tile {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let x = deserialize(file, R::read_i32_le).await?;
        let y = deserialize(file, R::read_i32_le).await?;
        let pos = deserialize_f32_le_vec4(file).await?;

        let unknown1 = TileUnknown::deserialize(file).await?;
        let unknown2 = deserialize(file, R::read_f32_le).await?;

        let eco_data = deserialize_vec(file, version, deserialize_i32).await?;
        let runtime_objects = deserialize_vec(file, version, RuntimeObject::deserialize).await?;
        let lights = deserialize_vec(file, version, RawLight::deserialize).await?;
        let areas = deserialize_vec(file, version, RawArea::deserialize).await?;
        let groups = deserialize_vec(file, version, RawGroup::deserialize).await?;

        let index = deserialize(file, R::read_i32_le).await?;
        skip(file, 4).await?;

        Ok(Tile {
            x,
            y,
            pos,
            unknown1,
            unknown2,
            eco_data,
            runtime_objects,
            lights,
            areas,
            groups,
            index,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct RenderBatch {
    pub index_offset: i32,
    pub index_count: i32,
    pub vertex_offset: i32,
    pub vertex_count: i32,
}

impl RenderBatch {
    async fn deserialize<R: AsyncReader>(file: &mut R, _: i32) -> Result<Self, Error> {
        let index_offset = deserialize(file, R::read_i32_le).await?;
        let index_count = deserialize(file, R::read_i32_le).await?;
        let vertex_offset = deserialize(file, R::read_i32_le).await?;
        let vertex_count = deserialize(file, R::read_i32_le).await?;

        Ok(RenderBatch {
            index_offset,
            index_count,
            vertex_offset,
            vertex_count,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct DetailMask {
    pub channel_count: i32,
    pub bits_per_channel: i32,
    pub pixels: Vec<u8>,
}

impl DetailMask {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let channel_count = deserialize(file, R::read_i32_le).await?;
        if channel_count <= 0 {
            return Ok(DetailMask::default());
        }

        let offset = tell(file).await;
        let side_len = deserialize(file, R::read_i32_le).await?;
        if side_len <= 0 {
            return Err(Error {
                kind: ErrorKind::NegativeLen(side_len),
                offset,
            });
        }

        let Some(total_channels) = side_len
            .checked_mul(side_len)
            .map(|pixel_count| pixel_count.saturating_mul(channel_count))
        else {
            return Err(Error {
                kind: ErrorKind::IntegerOverflow {
                    expected_bytes: 8,
                    actual_bytes: 4,
                },
                offset,
            });
        };

        let (len, bits_per_channel) = match version >= 4 {
            true => {
                let mut compressed_len = total_channels / 2;
                compressed_len = match total_channels % 2 == 0 {
                    true => compressed_len,
                    false => compressed_len + 1,
                };
                (compressed_len, 4)
            }
            false => (total_channels, 8),
        };

        let (pixels, _) = deserialize_exact(file, i32_to_usize(len)?).await?;

        Ok(DetailMask {
            channel_count,
            bits_per_channel,
            pixels,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [u8; 4],
    pub color1: Rgba8,
    pub color2: Rgba8,
    pub tex_coord1: [u16; 2],
    pub tex_coord2: [u16; 2],
}

impl Vertex {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let pos = deserialize_f32_le_vec3(file).await?;
        let normal = [
            deserialize(file, R::read_u8).await?,
            deserialize(file, R::read_u8).await?,
            deserialize(file, R::read_u8).await?,
            deserialize(file, R::read_u8).await?,
        ];
        let color1 = Rgba8::deserialize(file, version).await?;
        let color2 = Rgba8::deserialize(file, version).await?;
        let tex_coord1 = [
            deserialize(file, R::read_u16_le).await?,
            deserialize(file, R::read_u16_le).await?,
        ];
        let tex_coord2 = [
            deserialize(file, R::read_u16_le).await?,
            deserialize(file, R::read_u16_le).await?,
        ];

        Ok(Vertex {
            pos,
            normal,
            color1,
            color2,
            tex_coord1,
            tex_coord2,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct TerrainChunk {
    pub tiles: Vec<Tile>,
    pub render_batches: Vec<RenderBatch>,
    pub detail_mask: DetailMask,
    pub indices: Vec<u16>,
    pub vertices: Vec<Vertex>,
}

impl TerrainChunk {
    async fn deserialize<R: AsyncReader>(file: &mut R, version: i32) -> Result<Self, Error> {
        let tiles = deserialize_vec(file, version, Tile::deserialize).await?;
        let render_batches = deserialize_vec(file, version, RenderBatch::deserialize).await?;
        let detail_mask = DetailMask::deserialize(file, version).await?;
        let indices = deserialize_vec(file, version, deserialize_u16).await?;
        let vertices = deserialize_vec(file, version, Vertex::deserialize).await?;
        Ok(TerrainChunk {
            tiles,
            render_batches,
            detail_mask,
            indices,
            vertices,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct TerrainCollision {
    pub bounding_volume_hierarchies: Vec<BoundingVolumeHierarchy>,
}

impl TerrainCollision {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        let mut bounding_volume_hierarchies = Vec::new();
        while !is_eof(file).await? {
            let _len = deserialize(file, R::read_i32_le).await?;
            skip(file, 28).await?;
            bounding_volume_hierarchies.push(BoundingVolumeHierarchy::deserialize(file).await?);
        }

        Ok(TerrainCollision {
            bounding_volume_hierarchies,
        })
    }
}

async fn decompress_section<R: AsyncReader>(file: &mut R) -> Result<Vec<u8>, Error> {
    let expected_decompressed_len = i32_to_usize(deserialize(file, R::read_i32_le).await?)?;
    let compressed_len = deserialize(file, R::read_i32_le).await?;

    let offset = tell(file).await;
    let mut buffer = Vec::with_capacity(expected_decompressed_len);
    let mut decoder = ZlibDecoder::new(file.take(i32_to_u64(compressed_len)?));

    let actual_decompressed_len = decoder
        .read_to_end(&mut buffer)
        .await
        .map_err(|err| Error {
            kind: err.into(),
            offset,
        })?;

    if expected_decompressed_len != actual_decompressed_len {
        return Err(Error {
            kind: ErrorKind::UnexpectedDecompressedLen {
                expected_decompressed_len,
                actual_decompressed_len,
            },
            offset,
        });
    }

    Ok(buffer)
}

#[derive(Serialize, Deserialize)]
pub struct Gcnk {
    pub version: i32,
    pub chunk: TerrainChunk,
    pub collision: TerrainCollision,
}

impl Default for Gcnk {
    fn default() -> Self {
        Self {
            version: 1,
            chunk: Default::default(),
            collision: Default::default(),
        }
    }
}

impl DeserializeAsset for Gcnk {
    async fn deserialize<R: AsyncReader, P: AsRef<std::path::Path> + Send>(
        _: P,
        file: &mut R,
    ) -> Result<Self, Error> {
        let (magic, _) = deserialize_string(file, 4).await?;
        if magic != "GCNK" {
            // Empty GCNK files only contain "hello"
            if magic == "hello"[0..4] {
                return Ok(Gcnk::default());
            }

            return Err(Error {
                kind: ErrorKind::UnknownMagic(magic),
                offset: Some(0),
            });
        }

        let version = deserialize(file, R::read_i32_le).await?;

        let chunk_buffer = decompress_section(file).await?;
        let chunk = TerrainChunk::deserialize(&mut Cursor::new(chunk_buffer), version).await?;

        let collision_buffer = decompress_section(file).await?;
        let collision = TerrainCollision::deserialize(&mut Cursor::new(collision_buffer)).await?;

        Ok(Gcnk {
            version,
            chunk,
            collision,
        })
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
