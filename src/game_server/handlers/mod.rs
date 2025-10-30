use chrono::{DateTime, Datelike, Days, FixedOffset, Weekday};

use super::{packets::Pos, Broadcast, GameServer, ProcessPacketError};

pub mod character;
pub mod chat;
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
