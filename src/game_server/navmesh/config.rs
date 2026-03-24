use std::{collections::HashMap, fs::File, path::Path};

use glam::Vec2;
use polyanya::{Layer, Mesh, Triangulation};
use serde::Deserialize;

use crate::{game_server::navmesh::Navmesh, ConfigError};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct NavmeshLayer {
    pub outer_edges: Vec<[f32; 3]>,
}

impl From<NavmeshLayer> for Layer {
    fn from(value: NavmeshLayer) -> Self {
        let edges: Vec<Vec2> = value
            .outer_edges
            .iter()
            .map(|edge| Vec2::new(edge[0], edge[2]))
            .collect();
        let triangulation = Triangulation::from_outer_edges(&edges);
        let mut layer = triangulation.as_layer();
        layer.height = value.outer_edges.into_iter().map(|edge| edge[1]).collect();
        layer
    }
}

type NavmeshConfig = HashMap<String, Vec<NavmeshLayer>>;

pub fn load_navmeshes(config_dir: &Path) -> Result<HashMap<String, Navmesh>, ConfigError> {
    let mut file = File::open(config_dir.join("navmeshes.yaml"))?;
    let config: NavmeshConfig = serde_yaml::from_reader(&mut file)?;

    Ok(config
        .into_iter()
        .map(|(asset_name, layers)| {
            (
                asset_name,
                Navmesh::Complex(Mesh {
                    layers: layers.into_iter().map(Layer::from).collect(),
                    ..Default::default()
                }),
            )
        })
        .collect())
}
