use std::num::TryFromIntError;

use asset_serialize::gcnk::Gcnk;
use rerecast::{AreaType, BuildContoursFlags, DetailNavmesh, HeightfieldBuilder, TriMesh};

use crate::{game_server::assets::AssetCache, warn};

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
    config: &rerecast::Config,
) -> Result<polyanya::Mesh, NavmeshBuildError> {
    let asset_names =
        asset_cache.filter(zone_asset_name, |asset_name| asset_name.ends_with(".gcnk"));
    let (assets, errors) = asset_cache.deserialize::<Gcnk>(asset_names).await;
    for (asset_name, error) in errors.into_iter() {
        warn!("Failed to deserialize {asset_name} when building navmesh for {zone_asset_name}: {error:?}");
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for (_, asset) in assets.into_iter() {
        for vertex in asset.chunk.vertices.into_iter() {
            vertices.push(vertex.pos.into());
        }

        let base_triangle: u32 = indices.len().try_into()?;
        let base_index: u32 = base_triangle
            .checked_mul(3)
            .ok_or(NavmeshBuildError::TooManyIndices)?;
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

    let mut tri_mesh = TriMesh {
        vertices,
        indices,
        area_types: vec![AreaType::DEFAULT_WALKABLE; triangle_count],
    };

    tri_mesh.mark_walkable_triangles(config.walkable_slope_angle);
    let aabb = tri_mesh.compute_aabb().unwrap();

    let mut heightfield = HeightfieldBuilder {
        aabb,
        cell_size: config.cell_size,
        cell_height: config.cell_height,
    }
    .build()
    .unwrap();
    heightfield
        .rasterize_triangles(&tri_mesh, config.walkable_climb)
        .unwrap();
    heightfield.filter_low_hanging_walkable_obstacles(config.walkable_climb);
    heightfield.filter_ledge_spans(config.walkable_height, config.walkable_climb);
    heightfield.filter_walkable_low_height_spans(config.walkable_height);

    let mut compact_heightfield = heightfield
        .into_compact(config.walkable_height, config.walkable_climb)
        .unwrap();
    compact_heightfield.erode_walkable_area(config.walkable_radius);
    compact_heightfield.build_distance_field();
    compact_heightfield
        .build_regions(
            config.border_size,
            config.min_region_area,
            config.merge_region_area,
        )
        .unwrap();

    let contours = compact_heightfield.build_contours(
        config.max_simplification_error,
        config.max_edge_len,
        BuildContoursFlags::DEFAULT,
    );

    let poly_navmesh = contours
        .into_polygon_mesh(config.max_vertices_per_polygon)
        .unwrap();
    let detail_navmesh = DetailNavmesh::new(
        &poly_navmesh,
        &compact_heightfield,
        config.detail_sample_dist,
        config.detail_sample_max_error,
    )
    .unwrap();

    Ok(polyanya::RecastFullMesh::new(poly_navmesh, detail_navmesh).into())
}
