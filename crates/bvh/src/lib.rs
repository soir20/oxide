use std::{
    fs::File,
    io::{BufReader, Read, Write},
};

use bvh::{
    aabb::{Aabb, Bounded},
    bounding_hierarchy::BHShape,
    bvh::Bvh as SubBvh,
    ray::Ray,
};
use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};
use glam::{Affine3A, EulerRot, Quat, Vec3};
use serde::{de::Error, Deserialize, Serialize};

fn triangle_to_aabb(v1: [f32; 3], v2: [f32; 3], v3: [f32; 3]) -> Aabb<f32, 3> {
    Aabb::with_bounds(
        [
            v1[0].min(v2[0]).min(v3[0]),
            v1[1].min(v2[1]).min(v3[1]),
            v1[2].min(v2[2]).min(v3[2]),
        ]
        .into(),
        [
            v1[0].max(v2[0]).max(v3[0]),
            v1[1].max(v2[1]).max(v3[1]),
            v1[2].max(v2[2]).max(v3[2]),
        ]
        .into(),
    )
}

#[derive(Debug, Deserialize, Serialize)]
struct Triangle {
    indices: [u16; 3],
    node_index: usize,
}

impl From<[u16; 3]> for Triangle {
    fn from(indices: [u16; 3]) -> Self {
        Triangle {
            indices,
            node_index: 0,
        }
    }
}

struct TriangleAabb {
    aabb: Aabb<f32, 3>,
    node_index: usize,
}

impl Bounded<f32, 3> for TriangleAabb {
    fn aabb(&self) -> Aabb<f32, 3> {
        self.aabb
    }
}

impl BHShape<f32, 3> for TriangleAabb {
    fn set_bh_node_index(&mut self, node_index: usize) {
        self.node_index = node_index;
    }

    fn bh_node_index(&self) -> usize {
        self.node_index
    }
}

fn generate_bvh(vertices: &[[f32; 3]], triangles: &mut [Triangle]) -> SubBvh<f32, 3> {
    let mut aabbs: Vec<TriangleAabb> = triangles
        .iter()
        .map(|triangle| TriangleAabb {
            aabb: triangle_to_aabb(
                vertices[triangle.indices[0] as usize],
                vertices[triangle.indices[1] as usize],
                vertices[triangle.indices[2] as usize],
            ),
            node_index: triangle.node_index,
        })
        .collect();
    let bvh = SubBvh::build(&mut aabbs);
    aabbs
        .iter()
        .enumerate()
        .for_each(|(index, aabb)| triangles[index].node_index = aabb.node_index);
    bvh
}

#[derive(Debug, Clone)]
struct BakedTriangle {
    vertex1_index: usize,
    aabb: Aabb<f32, 3>,
}

impl Bounded<f32, 3> for BakedTriangle {
    fn aabb(&self) -> Aabb<f32, 3> {
        self.aabb
    }
}

#[derive(Deserialize)]
struct BvhTemplateData {
    vertices: Vec<[f32; 3]>,
    triangles: Vec<Triangle>,
    bvh: SubBvh<f32, 3>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(from = "BvhTemplateData")]
pub struct BvhTemplate {
    bvh: SubBvh<f32, 3>,
    vertices: Vec<[f32; 3]>,
    triangles: Vec<Triangle>,

    #[serde(skip)]
    baked_triangles: Vec<BakedTriangle>,
}

impl BvhTemplate {
    pub fn new(vertices: Vec<[f32; 3]>, triangles: Vec<[u16; 3]>) -> Self {
        let mut triangles: Vec<Triangle> = triangles
            .iter()
            .map(|triangle| Triangle::from(*triangle))
            .collect();

        BvhTemplateData {
            bvh: generate_bvh(&vertices, &mut triangles),
            vertices,
            triangles,
        }
        .into()
    }
}

impl From<BvhTemplateData> for BvhTemplate {
    fn from(data: BvhTemplateData) -> Self {
        let mut sequential_vertices = Vec::with_capacity(data.triangles.len() * 3);

        let baked_triangles = data
            .triangles
            .iter()
            .map(|triangle| {
                let v1 = data.vertices[triangle.indices[0] as usize];
                let v2 = data.vertices[triangle.indices[1] as usize];
                let v3 = data.vertices[triangle.indices[2] as usize];

                let vertex1_index = sequential_vertices.len();
                sequential_vertices.push(v1);
                sequential_vertices.push(v2);
                sequential_vertices.push(v3);

                BakedTriangle {
                    vertex1_index,
                    aabb: triangle_to_aabb(v1, v2, v3),
                }
            })
            .collect();

        BvhTemplate {
            vertices: sequential_vertices,
            triangles: data.triangles,
            bvh: data.bvh,
            baked_triangles,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct BvhInstanceData {
    bvh_index: usize,
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
    aabb: Aabb<f32, 3>,
    node_index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(from = "BvhInstanceData")]
pub struct BvhInstance {
    bvh_index: usize,
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
    aabb: Aabb<f32, 3>,
    node_index: usize,

    #[serde(skip)]
    global_to_local: Affine3A,
}

impl BvhInstance {
    pub fn new(
        bvh_index: usize,
        pos: [f32; 3],
        rot: [f32; 3],
        scale: f32,
        global_triangles: impl Iterator<Item = [[f32; 3]; 3]>,
    ) -> Self {
        BvhInstanceData {
            bvh_index,
            pos,
            rot,
            scale,
            aabb: global_triangles
                .map(|triangle| triangle_to_aabb(triangle[0], triangle[1], triangle[2]))
                .fold(Aabb::empty(), |acc, next| acc.join(&next)),
            node_index: 0,
        }
        .into()
    }
}

impl From<BvhInstanceData> for BvhInstance {
    fn from(value: BvhInstanceData) -> BvhInstance {
        let rotation = Quat::from_euler(EulerRot::YXZ, value.rot[0], value.rot[1], value.rot[2]);
        let translation = Vec3::from(value.pos);
        let scale = Vec3::splat(value.scale);

        let local_to_global =
            Affine3A::from_scale_rotation_translation(scale, rotation, translation);
        let global_to_local = local_to_global.inverse();

        BvhInstance {
            bvh_index: value.bvh_index,
            pos: value.pos,
            rot: value.rot,
            scale: value.scale,
            aabb: value.aabb,
            node_index: value.node_index,
            global_to_local,
        }
    }
}

impl Bounded<f32, 3> for BvhInstance {
    fn aabb(&self) -> Aabb<f32, 3> {
        self.aabb
    }
}

impl BHShape<f32, 3> for BvhInstance {
    fn set_bh_node_index(&mut self, node_index: usize) {
        self.node_index = node_index;
    }

    fn bh_node_index(&self) -> usize {
        self.node_index
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Bvh {
    root: SubBvh<f32, 3>,
    templates: Vec<BvhTemplate>,
    instances: Vec<BvhInstance>,
}

impl Bvh {
    pub fn new(templates: Vec<BvhTemplate>, mut instances: Vec<BvhInstance>) -> Self {
        Bvh {
            root: SubBvh::build(&mut instances),
            templates,
            instances,
        }
    }

    pub fn has_line_of_sight(&self, start: [f32; 3], end: [f32; 3]) -> bool {
        let start_vec = Vec3::from(start);
        let end_vec = Vec3::from(end);
        let delta = end_vec - start_vec;
        let global_max_distance = delta.length();

        if global_max_distance < f32::EPSILON {
            return true;
        }

        let direction = delta / global_max_distance;
        let ray = Ray::new(start.into(), direction.to_array().into());

        for bvh_instance in self.root.traverse(&ray, &self.instances) {
            let Some(bvh_template) = self.templates.get(bvh_instance.bvh_index) else {
                continue;
            };

            let relative_start = bvh_instance.global_to_local.transform_point3(start_vec);
            let relative_end = bvh_instance.global_to_local.transform_point3(end_vec);

            let local_delta = relative_end - relative_start;
            let relative_max_distance = local_delta.length();

            if relative_max_distance < f32::EPSILON {
                continue;
            }

            let relative_direction = local_delta / relative_max_distance;
            let relative_ray = Ray::new(
                relative_start.to_array().into(),
                relative_direction.to_array().into(),
            );

            for triangle in bvh_template
                .bvh
                .traverse(&relative_ray, &bvh_template.baked_triangles)
            {
                let v = &bvh_template.vertices[triangle.vertex1_index..triangle.vertex1_index + 3];
                let v1 = v[0].into();
                let v2 = v[1].into();
                let v3 = v[2].into();

                let intersection = relative_ray.intersects_triangle(&v1, &v2, &v3);
                if intersection.distance >= 0.0 && intersection.distance <= relative_max_distance {
                    return false;
                }
            }
        }

        true
    }
}

const BVH_MAGIC: &[u8; 9] = b"OXIDE_BVH";

pub fn write_bvh(file: &mut File, bvh: &Bvh) -> Result<(), pot::Error> {
    file.write_all(BVH_MAGIC)?;

    file.write_all(&1u32.to_le_bytes())?;

    let serialized_bvh: Vec<u8> = pot::to_vec(bvh)?;
    let mut encoder = GzEncoder::new(file, Compression::best());
    Write::write_all(&mut encoder, &serialized_bvh)?;
    encoder.finish()?;
    Ok(())
}

pub fn read_bvh(file: &File) -> Result<Bvh, pot::Error> {
    let mut reader = BufReader::new(file);

    let mut magic_buf = [0u8; BVH_MAGIC.len()];
    reader.read_exact(&mut magic_buf)?;
    if &magic_buf != BVH_MAGIC {
        return Err(pot::Error::custom(format!(
            "Invalid magic header: expected '{}', got '{:?}'",
            String::from_utf8_lossy(BVH_MAGIC),
            String::from_utf8_lossy(&magic_buf)
        )));
    }

    let mut version_buf = [0u8; 4];
    reader.read_exact(&mut version_buf)?;
    let version = u32::from_le_bytes(version_buf);
    if version != 1 {
        return Err(pot::Error::custom(format!(
            "Unknown file version: {}",
            version
        )));
    }

    let mut decoder = GzDecoder::new(reader);
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer)?;
    pot::from_slice(&buffer)
}
