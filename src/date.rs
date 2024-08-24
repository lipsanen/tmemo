use chrono::{Datelike, Days, Local, NaiveDate, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Date {
    pub day: i32,
}

impl Date {
    pub fn is_after(&self, other: &Date) -> bool {
        self.day >= other.day
    }

    pub fn now() -> Date {
        // Day changes 4 hours from midnight
        let time = Utc::now().checked_add_signed(TimeDelta::hours(-4)).unwrap();
        let dt = time.with_timezone(&Local);
        Date::from_naive(dt.date_naive())
    }

    pub fn from_naive(date: NaiveDate) -> Date {
        Date {
            day: date.num_days_from_ce() - 1,
        }
    }

    pub fn to_naive(&self) -> Option<NaiveDate> {
        let date = NaiveDate::from_ymd_opt(1, 1, 1)?;
        if self.day >= 0 {
            date.checked_add_days(Days::new(self.day as u64))
        } else {
            date.checked_sub_days(Days::new(-self.day as u64))
        }
    }

    pub fn checked_add_days(&self, d: i32) -> Option<Date> {
        let new_day = self.day.checked_add(d)?;
        Some(Date { day: new_day })
    }

    pub fn from_yo_opt(year: i32, ordinal: u32) -> Option<Date> {
        let naive = NaiveDate::from_yo_opt(year, ordinal)?;
        Some(Date::from_naive(naive))
    }

    pub fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<Date> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)?;
        Some(Date::from_naive(naive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naive_conversion_works() {
        let orig = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let converted = Date::from_naive(orig);
        let back = converted.to_naive().unwrap();
        assert_eq!(orig, back);
    }
}
