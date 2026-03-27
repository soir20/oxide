use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use glam::{Vec2, Vec3};

use crate::game_server::{
    handlers::distance3_pos,
    packets::{update_position::UpdatePlayerPos, CharacterState, CharacterStateFlags, Pos},
};

pub mod config;

#[derive(Clone, Debug, Default, PartialEq)]
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

#[derive(Clone)]
struct LinearPathState {
    direction_unit_vector: Pos,
    distance_traveled: f32,
    distance_required: f32,
    old_pos: Pos,
    new_pos: Pos,
    estimated_delta_since_last_tick: Pos,
    destination: NavmeshWaypoint,
    last_speed_update: Instant,
    last_character_state: CharacterState,
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
            last_speed_update: Instant::now(),
            last_character_state: CharacterState::default(),
        }
    }

    pub fn update_speed(&mut self, previous_speed: f32) {
        let now = Instant::now();
        let seconds_since_last_speed_update = now
            .saturating_duration_since(self.last_speed_update)
            .as_secs_f32();
        self.estimated_delta_since_last_tick +=
            self.direction_unit_vector * previous_speed * seconds_since_last_speed_update;
        self.last_speed_update = now;
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

        self.update_speed(speed);

        let estimated_current_pos = self.old_pos + self.estimated_delta_since_last_tick;
        let max_distance_traveled = distance3_pos(self.old_pos, estimated_current_pos);
        let distance_to_new_pos = distance3_pos(self.old_pos, self.new_pos);

        self.distance_traveled += match self.last_character_state.moving() {
            true => max_distance_traveled,
            false => distance_to_new_pos,
        };

        // Allow the next tickable step to start just as the NPC is almost reaching its
        // destination on clients. Since we set the old_pos to destination.pos, the NPC's
        // position will be set to the desired end position without drift.
        let seconds_per_tick = tick_duration.as_secs_f32();
        let estimated_distance_per_tick = speed * seconds_per_tick;
        let close_enough_distance = self.distance_required - estimated_distance_per_tick * 0.25;

        // The max distance traveled might be less than we expect if the NPC slowed down
        // during the tick. If the tick was longer than we expected, then the NPC stopped
        // at the new_pos and did not go any further.
        self.old_pos = match self.distance_traveled >= close_enough_distance {
            true => self.destination.pos,
            false => match self.last_character_state.moving() {
                true => estimated_current_pos,
                false => self.new_pos,
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
        self.last_character_state = character_state;
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

#[derive(Clone)]
pub struct NonLinearPathState {
    waypoints: VecDeque<NavmeshWaypoint>,
    linear_path_state: LinearPathState,
}

impl NonLinearPathState {
    pub fn new(current_pos: Pos, mut destination: NavmeshWaypoint, navmesh: &Navmesh) -> Self {
        let mut waypoints: VecDeque<NavmeshWaypoint> = navmesh
            .path(current_pos, destination.pos)
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
            destination.pos = last_waypoint.pos;
            waypoints.push_back(destination);
        }

        NonLinearPathState {
            waypoints,
            linear_path_state: LinearPathState::new(
                NavmeshWaypoint::without_rot(current_pos, CharacterState::default()),
                current_pos,
            ),
        }
    }

    pub fn update_speed(&mut self, previous_speed: f32) {
        self.linear_path_state.update_speed(previous_speed);
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

#[derive(Clone, Debug, Default)]
pub enum Navmesh {
    #[default]
    Simple,
    Complex(polyanya::Mesh),
}

pub const DEFAULT_NAVMESH: Navmesh = Navmesh::Simple;

impl From<polyanya::Mesh> for Navmesh {
    fn from(value: polyanya::Mesh) -> Self {
        Navmesh::Complex(value)
    }
}

impl Navmesh {
    pub fn path(&self, start: Pos, end: Pos) -> Vec<Pos> {
        match self {
            Navmesh::Simple => vec![end],
            Navmesh::Complex(navmesh) => {
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
