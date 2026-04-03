use std::{collections::HashMap, fs::File, path::Path};

use glam::Vec2;
use kiddo::{immutable::float::kdtree::ImmutableKdTree, SquaredEuclidean};
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
        let all_vertices = &[&value.exterior[..], &value.obstacles.concat()].concat();
        let kd_tree: ImmutableKdTree<f32, usize, 2, 32> = ImmutableKdTree::new_from_slice(
            &all_vertices
                .iter()
                .map(|vertex| [vertex[0], vertex[2]])
                .collect::<Vec<[f32; 2]>>(),
        );

        let exterior_vertices: Vec<Vec2> = value
            .exterior
            .iter()
            .map(|vertex| Vec2::new(vertex[0], vertex[2]))
            .collect();
        let mut triangulation = Triangulation::from_outer_edges(&exterior_vertices);
        triangulation.add_obstacles(value.obstacles.into_iter().map(|obstacle| {
            obstacle
                .into_iter()
                .map(|vertex| Vec2::new(vertex[0], vertex[2]))
        }));
        triangulation.set_agent_radius(0.75);

        let mut layer = triangulation.as_layer();
        layer.height = layer
            .vertices
            .iter()
            .map(|vertex| {
                let nearest = kd_tree
                    .nearest_one::<SquaredEuclidean>(&[vertex.coords.x, vertex.coords.y])
                    .item;
                all_vertices[nearest][1]
            })
            .collect();
        layer.bake();
        layer
    }
}

const fn default_search_delta() -> f32 {
    0.1
}

const fn default_search_steps() -> u32 {
    2
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct NavmeshConfig {
    layers: Vec<NavmeshLayer>,
    #[serde(default = "default_search_delta")]
    search_delta: f32,
    #[serde(default = "default_search_steps")]
    search_steps: u32,
}

type NavmeshConfigs = HashMap<String, NavmeshConfig>;

pub fn load_navmeshes(config_dir: &Path) -> Result<HashMap<String, Navmesh>, ConfigError> {
    let mut file = File::open(config_dir.join("navmeshes.yaml"))?;
    let configs: NavmeshConfigs = serde_yaml::from_reader(&mut file)?;

    if configs
        .iter()
        .any(|(_, config)| config.layers.len() > u8::MAX as usize)
    {
        return Err(ConfigError::ConstraintViolated(format!(
            "Cannot have more than {} navmesh layers",
            u8::MAX
        )));
    }

    Ok(configs
        .into_iter()
        .map(|(asset_name, config)| {
            let mut layers_by_vertex = HashMap::new();
            let mut stitch_vertices: HashMap<(u8, u8), Vec<(usize, usize)>> = HashMap::new();

            for (layer_index, layer) in config.layers.iter().enumerate() {
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
                layers: config.layers.into_iter().map(Layer::from).collect(),
                ..Default::default()
            };

            mesh.stitch_at_vertices(stitch_vertices.into_iter().collect(), false);
            mesh.set_search_delta(config.search_delta);
            mesh.set_search_steps(config.search_steps);

            (asset_name, Navmesh::Complex(mesh))
        })
        .collect())
}
