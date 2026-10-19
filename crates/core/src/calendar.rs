//! IANA calendar boundaries. Ambiguous/nonexistent midnights are explicit errors.
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeError {
    InvalidDate,
    UnknownZone,
    Reversed,
    Future,
    TooLong,
    InvalidBoundary,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
impl Interval {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, RangeError> {
        if start >= end {
            return Err(RangeError::InvalidBoundary);
        }
        Ok(Self { start, end })
    }
    /// Indivisible buckets must fit whole, never prorate them at query boundaries.
    pub fn contains_bucket(self, bucket: Self) -> bool {
        self.start <= bucket.start && bucket.end <= self.end
    }
}
pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}
#[derive(Clone, Copy, Debug)]
pub struct FakeClock(pub DateTime<Utc>);
impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarRange {
    pub requested: Interval,
    pub effective: Option<Interval>,
    pub reporting_zone: Tz,
    pub as_of: DateTime<Utc>,
    pub provisional: bool,
}
pub fn parse_instant(value: &str) -> Result<DateTime<Utc>, RangeError> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| RangeError::InvalidBoundary)
}
pub fn valid_zone(value: &str) -> bool {
    value.parse::<Tz>().is_ok()
}
pub fn parse_date(value: &str) -> Result<NaiveDate, RangeError> {
    if value.len() != 10 {
        return Err(RangeError::InvalidDate);
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| RangeError::InvalidDate)?;
    if date.format("%Y-%m-%d").to_string() != value {
        return Err(RangeError::InvalidDate);
    }
    Ok(date)
}
pub fn monday(date: NaiveDate) -> Result<NaiveDate, RangeError> {
    date.checked_sub_signed(Duration::days(i64::from(
        date.weekday().num_days_from_monday(),
    )))
    .ok_or(RangeError::InvalidBoundary)
}
pub fn midnight(date: NaiveDate, zone: Tz) -> Result<DateTime<Utc>, RangeError> {
    zone.from_local_datetime(
        &date
            .and_hms_opt(0, 0, 0)
            .ok_or(RangeError::InvalidBoundary)?,
    )
    .single()
    .map(|instant| instant.with_timezone(&Utc))
    .ok_or(RangeError::InvalidBoundary)
}
pub fn calendar_range(
    start: &str,
    end: &str,
    zone: &str,
    clock: &impl Clock,
) -> Result<CalendarRange, RangeError> {
    let start = parse_date(start)?;
    let end = parse_date(end)?;
    let zone: Tz = zone.parse().map_err(|_| RangeError::UnknownZone)?;
    if end < start {
        return Err(RangeError::Reversed);
    }
    if (end - start).num_days() + 1 > 366 {
        return Err(RangeError::TooLong);
    }
    let as_of = clock.now();
    let today = as_of.with_timezone(&zone).date_naive();
    if start > today || end > today {
        return Err(RangeError::Future);
    }
    let requested = Interval::new(
        midnight(start, zone)?,
        midnight(end.succ_opt().ok_or(RangeError::InvalidBoundary)?, zone)?,
    )?;
    let effective_end = requested.end.min(as_of);
    let effective = (requested.start < effective_end)
        .then(|| Interval::new(requested.start, effective_end))
        .transpose()?;
    Ok(CalendarRange {
        requested,
        effective,
        reporting_zone: zone,
        as_of,
        provisional: effective_end < requested.end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock(value: &str) -> FakeClock {
        FakeClock(value.parse().unwrap())
    }
    #[test]
    fn dst_days_are_not_fixed_24_hours() {
        let c = clock("2024-12-01T00:00:00Z");
        for (date, hours) in [("2024-03-10", 23), ("2024-11-03", 25)] {
            let range = calendar_range(date, date, "America/New_York", &c).unwrap();
            assert_eq!(
                (range.effective.unwrap().end - range.effective.unwrap().start).num_hours(),
                hours
            );
            assert!(!range.provisional);
        }
    }
    #[test]
    fn monday_and_single_as_of_clipping() {
        assert_eq!(
            monday(parse_date("2024-01-07").unwrap())
                .unwrap()
                .to_string(),
            "2024-01-01"
        );
        assert_eq!(
            monday(parse_date("2024-01-08").unwrap())
                .unwrap()
                .to_string(),
            "2024-01-08"
        );
        let c = clock("2024-06-03T15:16:17Z");
        let range = calendar_range("2024-06-03", "2024-06-03", "America/New_York", &c).unwrap();
        assert_eq!(range.effective.unwrap().end, c.now());
        assert_eq!(range.as_of, c.now());
        assert!(range.provisional);
        assert_eq!(
            range.requested.end.to_rfc3339(),
            "2024-06-04T04:00:00+00:00"
        );
    }
    #[test]
    fn validation_and_midnight_transition_policy() {
        let c = clock("2025-01-02T12:00:00Z");
        assert!(calendar_range("2024-01-01", "2024-12-31", "UTC", &c).is_ok());
        assert_eq!(
            calendar_range("2024-01-01", "2025-01-01", "UTC", &c),
            Err(RangeError::TooLong)
        );
        assert_eq!(
            calendar_range("2024-02-02", "2024-02-01", "UTC", &c),
            Err(RangeError::Reversed)
        );
        assert_eq!(
            calendar_range("2025-01-03", "2025-01-03", "UTC", &c),
            Err(RangeError::Future)
        );
        assert_eq!(
            calendar_range("2024-01-01", "2024-01-01", "Mars/Olympus", &c),
            Err(RangeError::UnknownZone)
        );
        assert_eq!(
            calendar_range("2024-02-30", "2024-03-01", "UTC", &c),
            Err(RangeError::InvalidDate)
        );
        assert!(parse_date("2024-1-01").is_err());
        // Sao Paulo skipped midnight; Havana repeated it. Neither is guessed.
        for (date, zone) in [
            ("2018-11-04", "America/Sao_Paulo"),
            ("2020-11-01", "America/Havana"),
            ("2011-12-30", "Pacific/Apia"),
        ] {
            assert_eq!(
                calendar_range(date, date, zone, &c),
                Err(RangeError::InvalidBoundary)
            );
        }
    }
    #[test]
    fn source_bucket_compatibility_keeps_whole_boundaries() {
        let c = clock("2024-06-03T12:00:00Z");
        let utc = calendar_range("2024-06-01", "2024-06-01", "UTC", &c).unwrap();
        let ny = calendar_range("2024-06-01", "2024-06-01", "America/New_York", &c).unwrap();
        assert!(!ny
            .effective
            .unwrap()
            .contains_bucket(utc.effective.unwrap()));
        assert!(utc
            .effective
            .unwrap()
            .contains_bucket(utc.effective.unwrap()));
    }
    #[test]
    fn monday_midnight_is_valid_without_a_fabricated_interval() {
        let at_midnight = clock("2024-06-03T04:00:00Z");
        let range =
            calendar_range("2024-06-03", "2024-06-03", "America/New_York", &at_midnight).unwrap();
        assert_eq!(range.effective, None);
        assert!(range.provisional);
        assert_eq!(
            monday(parse_date("2024-06-03").unwrap()).unwrap(),
            parse_date("2024-06-03").unwrap()
        );
        let after = clock("2024-06-03T04:00:00.001Z");
        let range = calendar_range("2024-06-03", "2024-06-03", "America/New_York", &after).unwrap();
        assert_eq!(
            (range.effective.unwrap().end - range.effective.unwrap().start).num_milliseconds(),
            1
        );
        assert_eq!(range.effective.unwrap().end, after.now());
    }
}
