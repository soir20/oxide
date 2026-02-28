use num_enum::TryFromPrimitive;
use tokio::io::AsyncReadExt;

use crate::{deserialize, skip, tell, AsyncReader, Error, ErrorKind};

async fn deserialize_u16_le_vec3<R: AsyncReader>(file: &mut R) -> Result<[u16; 3], Error> {
    Ok([
        deserialize(file, R::read_u16_le).await?,
        deserialize(file, R::read_u16_le).await?,
        deserialize(file, R::read_u16_le).await?,
    ])
}

async fn deserialize_f32_le_vec3<R: AsyncReader>(file: &mut R) -> Result<[f32; 3], Error> {
    Ok([
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
    ])
}

async fn deserialize_f32_le_vec4<R: AsyncReader>(file: &mut R) -> Result<[f32; 4], Error> {
    Ok([
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
    ])
}

pub enum OriginalNodeIndex {
    Escape { escape_index: i32 },
    Triangle { triangle_index: i32, sub_part: i32 },
}

pub struct OriginalNode {
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub index: OriginalNodeIndex,
}

impl OriginalNode {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        let aabb_min = deserialize_f32_le_vec3(file).await?;
        let aabb_max = deserialize_f32_le_vec3(file).await?;
        let escape_index = deserialize(file, R::read_i32_le).await?;
        let sub_part = deserialize(file, R::read_i32_le).await?;
        let triangle_index = deserialize(file, R::read_i32_le).await?;

        let index = match escape_index >= 0 {
            true => OriginalNodeIndex::Escape { escape_index },
            false => OriginalNodeIndex::Triangle {
                triangle_index,
                sub_part,
            },
        };

        skip(file, 20).await?;

        Ok(OriginalNode {
            aabb_min,
            aabb_max,
            index,
        })
    }
}

pub enum QuantizedNodeIndex {
    Escape { escape_index: i32 },
    Triangle { triangle_index: i32 },
}

impl From<i32> for QuantizedNodeIndex {
    fn from(value: i32) -> Self {
        match value >= 0 {
            true => {
                let triangle_index = 0b1_1111_1111_1111_1111_1111;
                QuantizedNodeIndex::Triangle {
                    triangle_index: value & triangle_index,
                }
            }
            false => QuantizedNodeIndex::Escape {
                escape_index: -value,
            },
        }
    }
}

pub struct QuantizedNode {
    pub aabb_min: [u16; 3],
    pub aabb_max: [u16; 3],
    pub index: QuantizedNodeIndex,
}

impl QuantizedNode {
    pub fn unquantize(aabb: &[u16; 3], quantization: &[f32; 4]) -> [f32; 3] {
        let x: f32 = aabb[0].into();
        let y: f32 = aabb[1].into();
        let z: f32 = aabb[2].into();
        [
            x / quantization[0],
            y / quantization[1],
            z / quantization[2],
        ]
    }

    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        let aabb_min = deserialize_u16_le_vec3(file).await?;
        let aabb_max = deserialize_u16_le_vec3(file).await?;
        let escape_or_triangle_index = deserialize(file, R::read_i32_le).await?;

        Ok(QuantizedNode {
            aabb_min,
            aabb_max,
            index: escape_or_triangle_index.into(),
        })
    }
}

pub struct SubtreeHeader {
    pub quantized_aabb_min: [u16; 3],
    pub quantized_aabb_max: [u16; 3],
    pub root_node_index: i32,
    pub subtree_size: i32,
}

impl SubtreeHeader {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        let quantized_aabb_min = deserialize_u16_le_vec3(file).await?;
        let quantized_aabb_max = deserialize_u16_le_vec3(file).await?;
        let root_node_index = deserialize(file, R::read_i32_le).await?;
        let subtree_size = deserialize(file, R::read_i32_le).await?;

        skip(file, 12).await?;

        Ok(SubtreeHeader {
            quantized_aabb_min,
            quantized_aabb_max,
            root_node_index,
            subtree_size,
        })
    }
}

#[derive(Copy, Clone, Debug, TryFromPrimitive)]
#[repr(u32)]
pub enum TraversalMode {
    Stackless = 0,
    StacklessCacheFriendly = 1,
    Recursive = 2,
}

impl TraversalMode {
    async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        let offset = tell(file).await;
        let value = deserialize(file, R::read_u32_le).await?;
        let traversal_mode = Self::try_from_primitive(value).map_err(|_| Error {
            kind: ErrorKind::UnknownDiscriminant(value.into(), Self::NAME),
            offset,
        })?;

        Ok(traversal_mode)
    }
}

pub enum NodeVec {
    Original { original_nodes: Vec<OriginalNode> },
    Quantized { quantized_nodes: Vec<QuantizedNode> },
}

pub struct BoundingVolumeHierarchy {
    pub aabb_min: [f32; 4],
    pub aabb_max: [f32; 4],
    pub quantization: [f32; 4],
    pub bullet_version: i32,
    pub traversal_mode: TraversalMode,
    pub nodes: NodeVec,
    pub subtree_headers: Vec<SubtreeHeader>,
}

impl BoundingVolumeHierarchy {
    pub(crate) async fn deserialize<R: AsyncReader>(file: &mut R) -> Result<Self, Error> {
        let aabb_min = deserialize_f32_le_vec4(file).await?;
        let aabb_max = deserialize_f32_le_vec4(file).await?;
        let quantization = deserialize_f32_le_vec4(file).await?;
        let bullet_version = deserialize(file, R::read_i32_le).await?;
        let node_count = deserialize(file, R::read_i32_le).await?;
        let use_quantization = deserialize(file, R::read_u8).await? != 0;
        skip(file, 83).await?;
        let traversal_mode = TraversalMode::deserialize(file).await?;
        skip(file, 20).await?;
        let subtree_header_count = deserialize(file, R::read_i32_le).await?;
        skip(file, 8).await?;

        let nodes = match use_quantization {
            true => {
                let mut quantized_nodes = Vec::new();
                for _ in 0..node_count {
                    quantized_nodes.push(QuantizedNode::deserialize(file).await?);
                }
                NodeVec::Quantized { quantized_nodes }
            }
            false => {
                let mut original_nodes = Vec::new();
                for _ in 0..node_count {
                    original_nodes.push(OriginalNode::deserialize(file).await?);
                }
                NodeVec::Original { original_nodes }
            }
        };

        let mut subtree_headers = Vec::new();
        for _ in 0..subtree_header_count {
            subtree_headers.push(SubtreeHeader::deserialize(file).await?);
        }

        Ok(BoundingVolumeHierarchy {
            aabb_min,
            aabb_max,
            quantization,
            bullet_version,
            traversal_mode,
            nodes,
            subtree_headers,
        })
    }
}
