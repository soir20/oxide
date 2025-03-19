use super::packets::Pos;

pub mod character;
pub mod chat;
pub mod command;
pub mod guid;
pub mod housing;
pub mod inventory;
pub mod item;
pub mod lock_enforcer;
pub mod login;
pub mod minigame;
pub mod mount;
pub mod reference_data;
pub mod saber_strike;
pub mod store;
pub mod test_data;
pub mod time;
pub mod unique_guid;
pub mod update_position;
pub mod zone;

pub fn distance3_pos(pos1: Pos, pos2: Pos) -> f32 {
    distance3(pos1.x, pos1.y, pos1.z, pos2.x, pos2.y, pos2.z)
}

pub fn distance3(x1: f32, y1: f32, z1: f32, x2: f32, y2: f32, z2: f32) -> f32 {
    let diff_x = x2 - x1;
    let diff_y = y2 - y1;
    let diff_z = z2 - z1;
    (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt()
}
