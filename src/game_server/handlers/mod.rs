use chrono::{DateTime, Datelike, Days, FixedOffset, Weekday};

use super::{packets::Pos, Broadcast, GameServer, ProcessPacketError};

pub mod character;
pub mod chat;
pub mod chat_command;
pub mod clicked_location;
pub mod combat;
pub mod command;
pub mod daily;
pub mod dialog;
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
pub mod saber_duel;
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

pub fn direction(old_pos: Pos, new_pos: Pos) -> Pos {
    let diff_x = new_pos.x - old_pos.x;
    let diff_y = new_pos.y - old_pos.y;
    let diff_z = new_pos.z - old_pos.z;

    let distance_required = distance3_pos(old_pos, new_pos).max(f32::MIN_POSITIVE);
    Pos {
        x: diff_x / distance_required,
        y: diff_y / distance_required,
        z: diff_z / distance_required,
        w: new_pos.w,
    }
}

pub fn is_between(segment_start: Pos, segment_end: Pos, pos: Pos) -> bool {
    segment_start.x.min(segment_end.x) <= pos.x
        && pos.x <= segment_start.x.max(segment_end.x)
        && segment_start.y.min(segment_end.y) <= pos.y
        && pos.y <= segment_start.y.max(segment_end.y)
        && segment_start.z.min(segment_end.z) <= pos.z
        && pos.z <= segment_start.z.max(segment_end.z)
}

pub fn pos_on_segment_at_distance_from_pos(
    segment_start: Pos,
    segment_end: Pos,
    target: Pos,
    distance: f32,
) -> Option<Pos> {
    //                         target
    //                           *
    //                          /|
    //                         / | perpendicular_distance
    //                        /  |
    // segment_start --------*--------*-------- segment_end
    //                     cand1    cand2
    //               (closer to start)

    let segment_direction = direction(segment_start, segment_end);
    let vector_to_target = target - segment_start;

    let projection_len = vector_to_target.x * segment_direction.x
        + vector_to_target.y * segment_direction.y
        + vector_to_target.z * segment_direction.z;

    let closest_pos_on_segment = Pos {
        x: segment_start.x + projection_len * segment_direction.x,
        y: segment_start.y + projection_len * segment_direction.y,
        z: segment_start.z + projection_len * segment_direction.z,
        w: segment_start.w,
    };

    let perpendicular_distance = distance3_pos(target, closest_pos_on_segment);
    let offset_squared = distance * distance - perpendicular_distance * perpendicular_distance;
    if offset_squared < 0.0 {
        return None;
    }

    let offset_from_closest_pos = offset_squared.sqrt();
    let candidate1 = Pos {
        x: closest_pos_on_segment.x + offset_from_closest_pos * segment_direction.x,
        y: closest_pos_on_segment.y + offset_from_closest_pos * segment_direction.y,
        z: closest_pos_on_segment.z + offset_from_closest_pos * segment_direction.z,
        w: closest_pos_on_segment.w,
    };
    let candidate2 = Pos {
        x: closest_pos_on_segment.x - offset_from_closest_pos * segment_direction.x,
        y: closest_pos_on_segment.y - offset_from_closest_pos * segment_direction.y,
        z: closest_pos_on_segment.z - offset_from_closest_pos * segment_direction.z,
        w: closest_pos_on_segment.w,
    };

    let candidate1_on_segment = is_between(segment_start, segment_end, candidate1);
    let candidate2_on_segment = is_between(segment_start, segment_end, candidate2);

    let dist1_to_start = distance3_pos(segment_start, candidate1);
    let dist2_to_start = distance3_pos(segment_start, candidate2);

    match (
        candidate1_on_segment,
        candidate2_on_segment,
        dist1_to_start < dist2_to_start,
    ) {
        (true, true, true) => Some(candidate1),
        (true, true, false) => Some(candidate2),
        (true, false, _) => Some(candidate1),
        (false, true, _) => Some(candidate2),
        (false, false, _) => None,
    }
}

pub fn offset_destination(old_pos: Pos, new_pos: Pos, offset: f32) -> Pos {
    let unit_vector = direction(old_pos, new_pos);

    Pos {
        x: new_pos.x - offset * unit_vector.x,
        y: new_pos.y - offset * unit_vector.y,
        z: new_pos.z - offset * unit_vector.z,
        w: new_pos.w,
    }
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

pub fn are_dates_consecutive(
    date1: &DateTime<FixedOffset>,
    date2: &DateTime<FixedOffset>,
    timezone: &FixedOffset,
) -> bool {
    let date1 = date1.with_timezone(timezone);
    let date2 = date2.with_timezone(timezone);

    date1.num_days_from_ce().abs_diff(date2.num_days_from_ce()) == 1
}

#[cfg(test)]
mod tests {
    use chrono::{Offset, TimeZone, Utc};

    use super::*;

    #[test]
    fn test_same_day_in_same_week() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 14, 23, 59, 59).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 0, 0, 0).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_diff_days_in_same_week() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 11, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 16, 8, 45).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_diff_days_in_same_week_for_timezone() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 9, 23, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 16, 22, 59, 59).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
    }

    #[test]
    fn test_diff_months_in_same_week() {
        let date1 = Utc.with_ymd_and_hms(2025, 7, 30, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 2, 16, 8, 45).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_diff_years_in_same_week() {
        let date1 = Utc.with_ymd_and_hms(2024, 12, 30, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 1, 2, 16, 8, 45).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_sunday_and_diff_day_in_same_week() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 10, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 16, 8, 45).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_saturday_and_diff_day_in_same_week() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 16, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 16, 8, 45).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_same_day_in_diff_weeks() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 7, 23, 59, 59).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 0, 0, 0).unwrap();
        assert!(!are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_diff_days_in_diff_weeks() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 4, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 16, 8, 45).unwrap();
        assert!(!are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_diff_days_in_diff_week_for_timezone() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 16, 23, 0, 0).unwrap();
        assert!(!are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
        assert!(!are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
    }

    #[test]
    fn test_sunday_and_diff_day_in_diff_weeks() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 17, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 16, 8, 45).unwrap();
        assert!(!are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_saturday_and_diff_day_in_diff_weeks() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 9, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 16, 8, 45).unwrap();
        assert!(!are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_diff_days_in_same_week_leap_second() {
        let date1 = Utc.timestamp_opt(1483228799, 1_000_000_000).unwrap();
        let date2 = Utc.with_ymd_and_hms(2016, 12, 25, 0, 0, 0).unwrap();
        assert!(are_dates_in_same_week(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_in_same_week(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_same_day_are_not_consecutive() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 14, 23, 59, 59).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 0, 0, 0).unwrap();
        assert!(!are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_consecutive_days_are_consecutive() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 14, 0, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 15, 23, 59, 59).unwrap();
        assert!(are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_consecutive_days_are_consecutive_for_timezone() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 13, 0, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 13, 23, 0, 0).unwrap();
        assert!(are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
        assert!(are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
    }

    #[test]
    fn test_non_consecutive_days_are_not_consecutive() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 14, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 16, 16, 8, 45).unwrap();
        assert!(!are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_non_consecutive_days_are_not_consecutive_for_timezone() {
        let date1 = Utc.with_ymd_and_hms(2025, 8, 13, 0, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 14, 23, 0, 0).unwrap();
        assert!(!are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
        assert!(!are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &FixedOffset::east_opt(3600).unwrap()
        ));
    }

    #[test]
    fn test_non_consecutive_days_diff_months_are_not_consecutive() {
        let date1 = Utc.with_ymd_and_hms(2025, 7, 15, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 16, 16, 8, 45).unwrap();
        assert!(!are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }

    #[test]
    fn test_non_consecutive_days_diff_years_are_not_consecutive() {
        let date1 = Utc.with_ymd_and_hms(2024, 8, 15, 5, 17, 24).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 8, 16, 16, 8, 45).unwrap();
        assert!(!are_dates_consecutive(
            &date1.fixed_offset(),
            &date2.fixed_offset(),
            &Utc.fix()
        ));
        assert!(!are_dates_consecutive(
            &date2.fixed_offset(),
            &date1.fixed_offset(),
            &Utc.fix()
        ));
    }
}
