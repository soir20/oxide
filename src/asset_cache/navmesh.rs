use std::{collections::HashMap, num::TryFromIntError};

use asset_serialize::gcnk::Gcnk;
use rerecast::{
    Aabb3d, AreaType, BuildContoursFlags, ConvexVolume, DetailNavmesh, HeightfieldBuilder,
    HeightfieldBuilderError, TriMesh,
};
use serde::Deserialize;

use crate::{asset_cache::AssetCache, warn};

#[derive(Deserialize)]
pub struct RerecastConfigOverride {
    pub cell_size_fraction: Option<f32>,
    pub cell_height_fraction: Option<f32>,
    pub agent_height: Option<f32>,
    pub agent_radius: Option<f32>,
    pub walkable_climb: Option<f32>,
    pub walkable_slope_angle: Option<f32>,
    pub min_region_size: Option<u16>,
    pub merge_region_size: Option<u16>,
    pub edge_max_len_factor: Option<u16>,
    pub max_simplification_error: Option<f32>,
    pub max_vertices_per_polygon: Option<u16>,
    pub detail_sample_dist: Option<f32>,
    pub detail_sample_max_error: Option<f32>,
    pub tile_size: Option<u16>,
    pub aabb: Option<Aabb3d>,
    pub contour_flags: Option<BuildContoursFlags>,
    pub tiling: Option<bool>,
    pub area_volumes: Option<Vec<ConvexVolume>>,
}

impl RerecastConfigOverride {
    pub fn merge(self, defaults: &rerecast::ConfigBuilder) -> rerecast::ConfigBuilder {
        rerecast::ConfigBuilder {
            cell_size_fraction: self
                .cell_size_fraction
                .unwrap_or(defaults.cell_size_fraction),
            cell_height_fraction: self
                .cell_height_fraction
                .unwrap_or(defaults.cell_height_fraction),
            agent_height: self.agent_height.unwrap_or(defaults.agent_height),
            agent_radius: self.agent_radius.unwrap_or(defaults.agent_radius),
            walkable_climb: self.walkable_climb.unwrap_or(defaults.walkable_climb),
            walkable_slope_angle: self
                .walkable_slope_angle
                .unwrap_or(defaults.walkable_slope_angle),
            min_region_size: self.min_region_size.unwrap_or(defaults.min_region_size),
            merge_region_size: self.merge_region_size.unwrap_or(defaults.merge_region_size),
            edge_max_len_factor: self
                .edge_max_len_factor
                .unwrap_or(defaults.edge_max_len_factor),
            max_simplification_error: self
                .max_simplification_error
                .unwrap_or(defaults.max_simplification_error),
            max_vertices_per_polygon: self
                .max_vertices_per_polygon
                .unwrap_or(defaults.max_vertices_per_polygon),
            detail_sample_dist: self
                .detail_sample_dist
                .unwrap_or(defaults.detail_sample_dist),
            detail_sample_max_error: self
                .detail_sample_max_error
                .unwrap_or(defaults.detail_sample_max_error),
            tile_size: self.tile_size.unwrap_or(defaults.tile_size),
            aabb: self.aabb.unwrap_or(defaults.aabb),
            contour_flags: self.contour_flags.unwrap_or(defaults.contour_flags),
            tiling: self.tiling.unwrap_or(defaults.tiling),
            area_volumes: self.area_volumes.unwrap_or(defaults.area_volumes.clone()),
        }
    }
}

#[derive(Deserialize)]
pub struct NavmeshConfig {
    pub assets: HashMap<String, Option<RerecastConfigOverride>>,
    pub default_settings: rerecast::ConfigBuilder,
}

pub enum NavmeshBuildError {
    TooManyIndices,
    EmptyMesh,
    Heightfield(HeightfieldBuilderError),
    Rasterization(String),
    CompactHeightfield(String),
    Region(String),
    PolygonNavmesh(String),
    DetailNavmesh(String),
}

impl From<TryFromIntError> for NavmeshBuildError {
    fn from(_: TryFromIntError) -> Self {
        NavmeshBuildError::TooManyIndices
    }
}

impl From<HeightfieldBuilderError> for NavmeshBuildError {
    fn from(value: HeightfieldBuilderError) -> Self {
        NavmeshBuildError::Heightfield(value)
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
    config: rerecast::ConfigBuilder,
) -> Result<polyanya::Mesh, NavmeshBuildError> {
    let config = config.build();
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
    let aabb = tri_mesh
        .compute_aabb()
        .ok_or(NavmeshBuildError::EmptyMesh)?;

    let mut heightfield = HeightfieldBuilder {
        aabb,
        cell_size: config.cell_size,
        cell_height: config.cell_height,
    }
    .build()?;
    heightfield
        .rasterize_triangles(&tri_mesh, config.walkable_climb)
        .map_err(|err| NavmeshBuildError::Rasterization(format!("{:?}", err)))?;
    heightfield.filter_low_hanging_walkable_obstacles(config.walkable_climb);
    heightfield.filter_ledge_spans(config.walkable_height, config.walkable_climb);
    heightfield.filter_walkable_low_height_spans(config.walkable_height);

    let mut compact_heightfield = heightfield
        .into_compact(config.walkable_height, config.walkable_climb)
        .map_err(|err| NavmeshBuildError::CompactHeightfield(format!("{:?}", err)))?;
    compact_heightfield.erode_walkable_area(config.walkable_radius);
    compact_heightfield.build_distance_field();
    compact_heightfield
        .build_regions(
            config.border_size,
            config.min_region_area,
            config.merge_region_area,
        )
        .map_err(|err| NavmeshBuildError::Region(format!("{:?}", err)))?;

    let contours = compact_heightfield.build_contours(
        config.max_simplification_error,
        config.max_edge_len,
        BuildContoursFlags::DEFAULT,
    );

    let poly_navmesh = contours
        .into_polygon_mesh(config.max_vertices_per_polygon)
        .map_err(|err| NavmeshBuildError::PolygonNavmesh(format!("{:?}", err)))?;
    let detail_navmesh = DetailNavmesh::new(
        &poly_navmesh,
        &compact_heightfield,
        config.detail_sample_dist,
        config.detail_sample_max_error,
    )
    .map_err(|err| NavmeshBuildError::DetailNavmesh(format!("{:?}", err)))?;

    Ok(polyanya::RecastFullMesh::new(poly_navmesh, detail_navmesh).into())
}
