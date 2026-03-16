use std::{collections::VecDeque, num::TryFromIntError, time::Duration};

use asset_serialize::gcnk::Gcnk;
use glam::{Vec2, Vec3};
use rerecast::{
    AreaType, BuildContoursFlags, DetailNavmesh, HeightfieldBuilder, HeightfieldBuilderError,
    TriMesh,
};

use crate::{
    game_server::{
        assets::AssetCache,
        handlers::distance3_pos,
        packets::{update_position::UpdatePlayerPos, CharacterState, CharacterStateFlags, Pos},
    },
    warn,
};

#[derive(Clone, Default, PartialEq)]
pub struct NavmeshWaypoint {
    pub pos: Pos,
    pub rot_x: Option<f32>,
    pub rot_y: Option<f32>,
    pub rot_z: Option<f32>,
    pub rot_x_offset: f32,
    pub rot_y_offset: f32,
    pub rot_z_offset: f32,
    pub character_state: CharacterState,
}

impl NavmeshWaypoint {
    pub fn without_rot(pos: Pos, character_state: CharacterState) -> Self {
        NavmeshWaypoint {
            pos,
            rot_x: None,
            rot_y: None,
            rot_z: None,
            rot_x_offset: 0.0,
            rot_y_offset: 0.0,
            rot_z_offset: 0.0,
            character_state,
        }
    }

    pub fn differs_from(&self, pos: Pos, rot: Pos, character_state: CharacterState) -> bool {
        self.pos != pos
            || self.rot_x.unwrap_or(rot.x) + self.rot_x_offset != rot.x
            || self.rot_y.unwrap_or(rot.y) + self.rot_y_offset != rot.y
            || self.rot_z.unwrap_or(rot.z) + self.rot_z_offset != rot.z
            || self.character_state != character_state
    }
}

#[derive(Clone, Default)]
struct LinearPathState {
    direction_unit_vector: Pos,
    distance_traveled: f32,
    distance_required: f32,
    old_pos: Pos,
    new_pos: Pos,
    estimated_delta_since_last_tick: Pos,
    destination: NavmeshWaypoint,
}

impl LinearPathState {
    pub fn new(destination: NavmeshWaypoint, start_pos: Pos) -> Self {
        let end_pos = destination.pos;
        let distance_required = distance3_pos(start_pos, end_pos);
        LinearPathState {
            direction_unit_vector: (end_pos - start_pos) / distance_required.max(f32::MIN_POSITIVE),
            distance_traveled: 0.0,
            distance_required,
            old_pos: start_pos,
            new_pos: start_pos,
            estimated_delta_since_last_tick: Pos::default(),
            destination,
        }
    }

    pub fn tick(
        &mut self,
        guid: u64,
        speed: f32,
        tick_duration: Duration,
        current_rot: Pos,
    ) -> Option<UpdatePlayerPos> {
        if self.reached_destination() {
            return None;
        }

        let estimated_current_pos = self.old_pos + self.estimated_delta_since_last_tick;
        let max_distance_traveled = distance3_pos(self.old_pos, estimated_current_pos);
        let distance_to_new_pos = distance3_pos(self.old_pos, self.new_pos);

        self.distance_traveled += max_distance_traveled.min(distance_to_new_pos);

        // Allow the next tickable step to start just as the NPC is almost reaching its
        // destination on clients. Since we set the old_pos to destination.pos, the NPC's
        // position will be set to the desired end position without drift.
        let seconds_per_tick = tick_duration.as_secs_f32();
        let estimated_distance_per_tick = speed * seconds_per_tick;
        let close_enough_distance = self.distance_required - estimated_distance_per_tick;

        // The max distance traveled might be less than we expect if the NPC slowed down
        // during the tick. If the tick was longer than we expected, then the NPC stopped
        // at the new_pos and did not go any further.
        self.old_pos = match self.distance_traveled >= close_enough_distance {
            true => self.destination.pos,
            false => match max_distance_traveled > distance_to_new_pos {
                true => self.new_pos,
                false => estimated_current_pos,
            },
        };

        // We don't know for certain if the NPC will reach the destination in the next tick,
        // because its speed could change
        let should_reach_destination =
            self.distance_traveled + estimated_distance_per_tick >= self.distance_required;
        self.new_pos = match should_reach_destination {
            true => self.destination.pos,
            false => self.old_pos + self.direction_unit_vector * estimated_distance_per_tick,
        };

        let mut new_rot = Pos {
            x: self.direction_unit_vector.x,
            y: current_rot.y,
            z: self.direction_unit_vector.z,
            w: current_rot.w,
        };
        let mut character_state = CharacterStateFlags {
            moving: true,
            jumping: false,
        }
        .into();

        if should_reach_destination {
            if let Some(new_rot_x) = self.destination.rot_x {
                new_rot.x = new_rot_x;
            }
            new_rot.x += self.destination.rot_x_offset;

            if let Some(new_rot_y) = self.destination.rot_y {
                new_rot.y = new_rot_y;
            }
            new_rot.y += self.destination.rot_y_offset;

            if let Some(new_rot_z) = self.destination.rot_z {
                new_rot.z = new_rot_z;
            }
            new_rot.z += self.destination.rot_z_offset;

            character_state = self.destination.character_state;
        }

        // The client doesn't rotate the character after it stops moving when rotation is (0, 0)
        if new_rot.x == 0.0 && new_rot.z == 0.0 {
            new_rot.x = self.direction_unit_vector.x;
            new_rot.z = self.direction_unit_vector.z;
        }

        self.estimated_delta_since_last_tick = Pos::default();
        Some(UpdatePlayerPos {
            guid,
            pos_x: self.new_pos.x,
            pos_y: self.new_pos.y,
            pos_z: self.new_pos.z,
            rot_x: new_rot.x,
            rot_y: new_rot.y,
            rot_z: new_rot.z,
            character_state,
            unknown: 0,
        })
    }

    pub fn reached_destination(&self) -> bool {
        // We can do an exact comparison because we set old_pos to the destination pos exactly
        self.old_pos == self.destination.pos
    }
}

pub struct NonLinearPathState {
    waypoints: VecDeque<NavmeshWaypoint>,
    linear_path_state: LinearPathState,
}

impl NonLinearPathState {
    pub fn new(current_pos: Pos, mut new_destination: NavmeshWaypoint, navmesh: &Navmesh) -> Self {
        let mut waypoints: VecDeque<NavmeshWaypoint> = navmesh
            .path(current_pos, new_destination.pos)
            .into_iter()
            .map(|pos| {
                NavmeshWaypoint::without_rot(
                    pos,
                    CharacterStateFlags {
                        moving: true,
                        jumping: false,
                    }
                    .into(),
                )
            })
            .collect();

        if let Some(last_waypoint) = waypoints.pop_back() {
            new_destination.pos = last_waypoint.pos;
            waypoints.push_back(new_destination);
        }

        NonLinearPathState {
            waypoints,
            linear_path_state: LinearPathState::default(),
        }
    }

    pub fn tick(
        &mut self,
        guid: u64,
        speed: f32,
        tick_duration: Duration,
        current_rot: Pos,
    ) -> Option<UpdatePlayerPos> {
        if self.linear_path_state.reached_destination() {
            while let Some(waypoint) = self.waypoints.pop_front() {
                let mut linear_path_state =
                    LinearPathState::new(waypoint, self.linear_path_state.old_pos);
                let pos_update = linear_path_state.tick(guid, speed, tick_duration, current_rot);
                if !linear_path_state.reached_destination() {
                    self.linear_path_state = linear_path_state;
                    return pos_update;
                }
            }
        }

        self.linear_path_state
            .tick(guid, speed, tick_duration, current_rot)
    }

    pub fn reached_destination(&self) -> bool {
        self.waypoints.is_empty() && self.linear_path_state.reached_destination()
    }
}

#[derive(Default)]
pub enum Navmesh {
    #[default]
    Simple,
    Recast(polyanya::Mesh),
}

impl Navmesh {
    pub fn path(&self, start: Pos, end: Pos) -> Vec<Pos> {
        match self {
            Navmesh::Simple => todo!(),
            Navmesh::Recast(navmesh) => {
                let Some(start_polygon) =
                    navmesh.get_closest_point_at_height(Vec2::new(start.x, start.z), start.y)
                else {
                    return Vec::new();
                };
                let Some(end_polygon) =
                    navmesh.get_closest_point_at_height(Vec2::new(end.x, end.z), end.y)
                else {
                    return Vec::new();
                };

                navmesh
                    .path(start_polygon, end_polygon)
                    .map(|path| {
                        path.path_with_height(
                            Vec3::new(start.x, start.y, start.z),
                            Vec3::new(end.x, end.y, end.z),
                            navmesh,
                        )
                        .into_iter()
                        .map(|coord| Pos {
                            x: coord.x,
                            y: coord.y,
                            z: coord.z,
                            w: start.w,
                        })
                        .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }
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
