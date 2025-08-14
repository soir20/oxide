use chrono::{DateTime, Datelike, Days, FixedOffset, TimeDelta, Weekday};

use super::{packets::Pos, Broadcast, GameServer, ProcessPacketError};

pub mod character;
pub mod chat;
pub mod command;
pub mod daily;
pub mod fleet_commander;
pub mod force_connection;
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
pub mod tick;
pub mod time;
pub mod unique_guid;
pub mod update_position;
pub mod zone;

pub type WriteLockingBroadcastSupplier = Result<
    Box<dyn FnOnce(&GameServer) -> Result<Vec<Broadcast>, ProcessPacketError>>,
    ProcessPacketError,
>;

pub fn distance3_pos(pos1: Pos, pos2: Pos) -> f32 {
    distance3(pos1.x, pos1.y, pos1.z, pos2.x, pos2.y, pos2.z)
}

pub fn distance3(x1: f32, y1: f32, z1: f32, x2: f32, y2: f32, z2: f32) -> f32 {
    let diff_x = x2 - x1;
    let diff_y = y2 - y1;
    let diff_z = z2 - z1;
    (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt()
}

pub fn are_dates_in_same_week(
    date1: &DateTime<FixedOffset>,
    date2: &DateTime<FixedOffset>,
    timezone: &FixedOffset,
) -> bool {
    let date1 = date1.with_timezone(timezone);
    let date2 = date2.with_timezone(timezone);

    // Subtract a day since the ISO week starts from Monday, and we want to start
    // the week on Sunday
    let week1 = match date1.weekday() {
        Weekday::Sun => date1
            .checked_add_days(Days::new(1))
            .map(|date| date.iso_week()),
        _ => Some(date1.iso_week()),
    };

    let week2 = match date2.weekday() {
        Weekday::Sun => date2
            .checked_add_days(Days::new(1))
            .map(|date| date.iso_week()),
        _ => Some(date2.iso_week()),
    };

    week1 == week2
}
