use std::num::TryFromIntError;

use asset_serialize::gcnk::Gcnk;
use rerecast::{AreaType, TriMesh};

use crate::game_server::assets::AssetCache;

pub enum NavmeshBuildError {
    TooManyIndices,
}

impl From<TryFromIntError> for NavmeshBuildError {
    fn from(_: TryFromIntError) -> Self {
        NavmeshBuildError::TooManyIndices
    }
}

fn global_index(base_index: u32, index: u16) -> Result<u32, NavmeshBuildError> {
    base_index
        .checked_add(index.into())
        .ok_or(NavmeshBuildError::TooManyIndices)
}

pub async fn build_navmesh(
    asset_cache: &AssetCache,
    zone_asset_name: &str,
) -> Result<(), NavmeshBuildError> {
    let asset_names =
        asset_cache.filter(zone_asset_name, |asset_name| asset_name.ends_with(".gcnk"));
    let (assets, errors) = asset_cache.deserialize::<Gcnk>(asset_names).await;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for (_, asset) in assets.into_iter() {
        for vertex in asset.chunk.vertices.into_iter() {
            vertices.push(vertex.pos.into());
        }

        let base_index: u32 = indices.len().try_into()?;
        for triangle_indices in asset.chunk.indices.chunks(3) {
            let triangle = [
                global_index(base_index, triangle_indices[0])?,
                global_index(base_index, triangle_indices[1])?,
                global_index(base_index, triangle_indices[2])?,
            ];
            indices.push(triangle.into());
        }
    }

    let triangle_count = indices.len();
    TriMesh {
        vertices,
        indices,
        area_types: vec![AreaType::DEFAULT_WALKABLE; triangle_count],
    };

    Ok(())
}
