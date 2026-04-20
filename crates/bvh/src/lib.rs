use std::{
    cell::Cell,
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
};

use bvh::{
    aabb::{Aabb, Bounded},
    bounding_hierarchy::BHShape,
    bvh::Bvh as SubBvh,
    ray::Ray,
};
use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};
use glam::{EulerRot, Quat, Vec3};
use serde::{Deserialize, Serialize};

fn vertex_from_index(vertices: &[[f32; 3]], index: u16) -> [f32; 3] {
    let index = usize::from(index);
    vertices[index]
}

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

fn with_vertices<'a>(
    vertices: &'a [[f32; 3]],
    triangles: &'a [Triangle],
) -> Vec<TriangleWithVertices<'a>> {
    triangles
        .iter()
        .map(|triangle| TriangleWithVertices {
            triangle,
            vertices: &vertices,
        })
        .collect()
}

fn generate_bvh(vertices: &[[f32; 3]], triangles: &mut [Triangle]) -> SubBvh<f32, 3> {
    let mut triangles_with_vertices = with_vertices(vertices, triangles);
    SubBvh::build(&mut triangles_with_vertices)
}

#[derive(Deserialize, Serialize)]
struct Triangle {
    indices: [u16; 3],
    node_index: Cell<usize>,
}

impl From<[u16; 3]> for Triangle {
    fn from(indices: [u16; 3]) -> Self {
        Triangle {
            indices,
            node_index: Cell::new(0),
        }
    }
}

struct TriangleWithVertices<'a> {
    triangle: &'a Triangle,
    vertices: &'a [[f32; 3]],
}

impl<'a> Bounded<f32, 3> for TriangleWithVertices<'a> {
    fn aabb(&self) -> Aabb<f32, 3> {
        let v1 = vertex_from_index(self.vertices, self.triangle.indices[0]);
        let v2 = vertex_from_index(self.vertices, self.triangle.indices[1]);
        let v3 = vertex_from_index(self.vertices, self.triangle.indices[2]);
        triangle_to_aabb(v1, v2, v3)
    }
}

impl<'a> BHShape<f32, 3> for TriangleWithVertices<'a> {
    fn set_bh_node_index(&mut self, node_index: usize) {
        self.triangle.node_index.set(node_index);
    }

    fn bh_node_index(&self) -> usize {
        self.triangle.node_index.get()
    }
}

#[derive(Deserialize, Serialize)]
pub struct BvhTemplate {
    bvh: SubBvh<f32, 3>,
    vertices: Vec<[f32; 3]>,
    triangles: Vec<Triangle>,
}

impl BvhTemplate {
    pub fn new(vertices: Vec<[f32; 3]>, triangles: Vec<[u16; 3]>) -> Self {
        let mut triangles: Vec<Triangle> = triangles
            .iter()
            .map(|triangle| Triangle::from(*triangle))
            .collect();

        BvhTemplate {
            bvh: generate_bvh(&vertices, &mut triangles),
            vertices,
            triangles,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct BvhInstance {
    id: u32,
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
    aabb: Aabb<f32, 3>,
    node_index: usize,
}

impl BvhInstance {
    pub fn new(
        id: u32,
        pos: [f32; 3],
        rot: [f32; 3],
        scale: f32,
        vertices: &[[f32; 3]],
        triangles: &[[u16; 3]],
    ) -> Self {
        BvhInstance {
            id,
            pos,
            rot,
            scale,
            aabb: triangles
                .iter()
                .map(|triangle| {
                    triangle_to_aabb(
                        vertices[usize::from(triangle[0])],
                        vertices[usize::from(triangle[1])],
                        vertices[usize::from(triangle[2])],
                    )
                })
                .fold(Aabb::empty(), |acc, next| acc.join(&next)),
            node_index: 0,
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

#[derive(Deserialize, Serialize)]
pub struct Bvh {
    root: SubBvh<f32, 3>,
    templates: HashMap<u32, BvhTemplate>,
    instances: Vec<BvhInstance>,
}

impl Bvh {
    pub fn new(templates: HashMap<u32, BvhTemplate>, mut instances: Vec<BvhInstance>) -> Self {
        Bvh {
            root: SubBvh::build(&mut instances),
            templates,
            instances,
        }
    }

    pub fn has_line_of_sight(&self, start: [f32; 3], end: [f32; 3]) -> bool {
        let start = Vec3::from(start);
        let end = Vec3::from(end);
        let direction: [f32; 3] = (end - start).normalize().into();
        let ray = Ray::new(<[f32; 3]>::from(start).into(), direction.into());
        for bvh_instance in self.root.traverse(&ray, &self.instances) {
            let Some(bvh_template) = self.templates.get(&bvh_instance.id) else {
                continue;
            };

            let inverse_rotation = Quat::from_euler(
                EulerRot::YXZ,
                bvh_instance.rot[0],
                bvh_instance.rot[1],
                bvh_instance.rot[2],
            )
            .inverse();
            let relative_start =
                (inverse_rotation * (start - Vec3::from(bvh_instance.pos))) / bvh_instance.scale;
            let relative_end =
                (inverse_rotation * (end - Vec3::from(bvh_instance.pos))) / bvh_instance.scale;
            let relative_direction: [f32; 3] = (relative_end - relative_start).normalize().into();
            let relative_ray = Ray::new(
                <[f32; 3]>::from(relative_start).into(),
                relative_direction.into(),
            );

            let triangles_with_vertices =
                with_vertices(&bvh_template.vertices, &bvh_template.triangles);
            for triangle in bvh_template
                .bvh
                .traverse(&relative_ray, &triangles_with_vertices)
            {
                let intersection = relative_ray.intersects_triangle(
                    &triangle.vertices[0].into(),
                    &triangle.vertices[1].into(),
                    &triangle.vertices[2].into(),
                );
                if intersection.distance < f32::INFINITY {
                    return false;
                }
            }
        }

        true
    }
}

pub fn write_bvh(file: &File, bvh: &Bvh) -> Result<(), pot::Error> {
    let serialized_bvh: Vec<u8> = pot::to_vec(bvh)?;
    let mut encoder = GzEncoder::new(file, Compression::best());
    std::io::Write::write_all(&mut encoder, &serialized_bvh)?;
    encoder.finish()?;
    Ok(())
}

pub fn read_bvh(file: &File) -> Result<Bvh, pot::Error> {
    let mut decoder = GzDecoder::new(BufReader::new(file));
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer)?;
    pot::from_slice(&buffer)
}
