use std::{collections::HashMap, fs::File, path::Path};

use glam::Vec2;
use polyanya::{Layer, Mesh, Triangulation};
use serde::Deserialize;

use crate::{game_server::navmesh::Navmesh, ConfigError};

type Polygon = Vec<[f32; 3]>;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct NavmeshLayer {
    pub exterior: Polygon,
    pub obstacles: Vec<Polygon>,
}

impl From<NavmeshLayer> for Layer {
    fn from(value: NavmeshLayer) -> Self {
        let edges: Vec<Vec2> = value
            .exterior
            .iter()
            .map(|edge| Vec2::new(edge[0], edge[2]))
            .collect();
        let triangulation = Triangulation::from_outer_edges(&edges);
        let mut layer = triangulation.as_layer();
        layer.height = value.exterior.into_iter().map(|edge| edge[1]).collect();
        layer
    }
}

type NavmeshConfig = HashMap<String, Vec<NavmeshLayer>>;

pub fn load_navmeshes(config_dir: &Path) -> Result<HashMap<String, Navmesh>, ConfigError> {
    let mut file = File::open(config_dir.join("navmeshes.yaml"))?;
    let config: NavmeshConfig = serde_yaml::from_reader(&mut file)?;

    if config
        .iter()
        .any(|(_, layers)| layers.len() > u8::MAX as usize)
    {
        return Err(ConfigError::ConstraintViolated(format!(
            "Cannot have more than {} navmesh layers",
            u8::MAX
        )));
    }

    Ok(config
        .into_iter()
        .map(|(asset_name, layers)| {
            let mut layers_by_vertex = HashMap::new();
            let mut stitch_vertices: HashMap<(u8, u8), Vec<(usize, usize)>> = HashMap::new();

            for (layer_index, layer) in layers.iter().enumerate() {
                let layer_index = layer_index as u8;

                for (vertex_index, vertex) in layer.exterior.iter().enumerate() {
                    let vertex_bits = [
                        vertex[0].to_bits(),
                        vertex[1].to_bits(),
                        vertex[2].to_bits(),
                    ];

                    let vertex_layers = layers_by_vertex
                        .entry(vertex_bits)
                        .or_insert_with(|| vec![(layer_index, vertex_index)]);

                    if vertex_layers.last().map(|(layer_index, _)| layer_index)
                        != Some(&layer_index)
                    {
                        for (other_layer_index, other_vertex_index) in vertex_layers.iter() {
                            stitch_vertices
                                .entry((*other_layer_index, layer_index))
                                .or_default()
                                .push((vertex_index, *other_vertex_index));
                        }
                    }
                }
            }

            let mut mesh = Mesh {
                layers: layers.into_iter().map(Layer::from).collect(),
                ..Default::default()
            };

            mesh.stitch_at_vertices(stitch_vertices.into_iter().collect(), false);

            (asset_name, Navmesh::Complex(mesh))
        })
        .collect())
}
