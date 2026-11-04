//! Game time and durations, in whole minutes (ADR-001 §2).
//!
//! # Literal formats
//!
//! [`Time`]:
//! - `dN HH:MM`: day `N` (starting at 1, no leading zeros), one space, then a
//!   two-digit hour (`00`–`23`) and two-digit minute (`00`–`59`).
//!   `d1 00:00` is minute 0, `d3 22:00` is minute 4200.
//! - `HH:MM`: shorthand for day 1.
//!
//! [`Display`](fmt::Display) always writes the `dN HH:MM` form.
//!
//! [`Duration`]: an optional `-`, then one or more `<number><unit>`
//! components with units `d` (1440 minutes), `h` (60) and `m` (1), each used at
//! most once and in that order, with no spaces: `15m`, `2h`, `1d`, `1h30m`,
//! `2d12h`, `-45m`. Numbers have no leading zeros (except `0` itself) and are
//! not limited to their unit's range, so `90m` is valid. `Display` writes the
//! canonical form: largest units first, zero components omitted, `0m` for zero.
//! So `90m` displays as `1h30m`; every displayed duration parses back to the
//! same value.

use std::fmt;
use std::str::FromStr;

use crate::error::{CoreError, Result};

const MINUTES_PER_HOUR: i64 = 60;
const MINUTES_PER_DAY: i64 = 24 * MINUTES_PER_HOUR;

/// A point in game time: whole minutes since game start (`d1 00:00`).
///
/// Never negative. Arithmetic is checked: it returns `None` instead of
/// overflowing or going before game start, and never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Time(i64);

impl Time {
    /// Game start, `d1 00:00`.
    pub const START: Self = Self(0);

    /// Creates a time from minutes since game start. `None` if negative.
    pub fn from_minutes(minutes: i64) -> Option<Self> {
        (minutes >= 0).then_some(Self(minutes))
    }

    /// Creates a time from a day (starting at 1), hour (0–23) and minute
    /// (0–59).
    pub fn from_day_hm(day: i64, hour: u32, minute: u32) -> Result<Self> {
        let err = |reason| CoreError::InvalidTime {
            text: format!("day {day}, {hour:02}:{minute:02}"),
            reason,
        };
        if day < 1 {
            return Err(err("days start at 1"));
        }
        if hour > 23 {
            return Err(err("hour must be 00-23"));
        }
        if minute > 59 {
            return Err(err("minute must be 00-59"));
        }
        (day - 1)
            .checked_mul(MINUTES_PER_DAY)
            .and_then(|m| m.checked_add(i64::from(hour) * MINUTES_PER_HOUR + i64::from(minute)))
            .map(Self)
            .ok_or_else(|| err("day is too large"))
    }

    /// Minutes since game start.
    pub fn minutes(self) -> i64 {
        self.0
    }

    /// The day, starting at 1.
    pub fn day(self) -> i64 {
        self.0 / MINUTES_PER_DAY + 1
    }

    /// The hour of the day, 0–23.
    pub fn hour(self) -> i64 {
        self.0 % MINUTES_PER_DAY / MINUTES_PER_HOUR
    }

    /// The minute of the hour, 0–59.
    pub fn minute(self) -> i64 {
        self.0 % MINUTES_PER_HOUR
    }

    /// `self + duration`; `None` on overflow or if the result is before game
    /// start.
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.0.checked_add(duration.0).and_then(Self::from_minutes)
    }

    /// `self - duration`; `None` on overflow or if the result is before game
    /// start.
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        self.0.checked_sub(duration.0).and_then(Self::from_minutes)
    }

    /// `self - earlier`, negative if `earlier` is later than `self`.
    ///
    /// Infallible: both times are non-negative, so the difference always fits.
    pub fn duration_since(self, earlier: Self) -> Duration {
        // Both operands are in 0..=i64::MAX, so the difference is in
        // -i64::MAX..=i64::MAX and cannot overflow; saturating keeps that
        // explicit without a panic path.
        Duration(self.0.saturating_sub(earlier.0))
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{} {:02}:{:02}", self.day(), self.hour(), self.minute())
    }
}

/// Parses a run of ASCII digits with no sign and no leading zeros.
fn parse_number(digits: &str) -> std::result::Result<u64, &'static str> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected a number");
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return Err("numbers must not have leading zeros");
    }
    digits.parse().map_err(|_| "number is too large")
}

/// Parses exactly two ASCII digits.
fn parse_two_digits(digits: &str) -> std::result::Result<u32, &'static str> {
    if digits.len() != 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected `HH:MM` with two-digit hour and minute");
    }
    digits.parse().map_err(|_| "expected two digits")
}

impl FromStr for Time {
    type Err = CoreError;

    /// Parses `dN HH:MM` or `HH:MM` (day 1).
    fn from_str(s: &str) -> Result<Self> {
        let err = |reason| CoreError::InvalidTime {
            text: s.to_owned(),
            reason,
        };
        let (day, clock) = match s.strip_prefix('d') {
            Some(rest) => {
                let (day, clock) = rest
                    .split_once(' ')
                    .ok_or_else(|| err("expected `dN HH:MM`"))?;
                let day = parse_number(day).map_err(err)?;
                (
                    i64::try_from(day).map_err(|_| err("day is too large"))?,
                    clock,
                )
            }
            None => (1, s),
        };
        let (hour, minute) = clock
            .split_once(':')
            .ok_or_else(|| err("expected `HH:MM` or `dN HH:MM`"))?;
        let hour = parse_two_digits(hour).map_err(err)?;
        let minute = parse_two_digits(minute).map_err(err)?;
        Self::from_day_hm(day, hour, minute).map_err(|e| match e {
            CoreError::InvalidTime { reason, .. } => err(reason),
            other => other,
        })
    }
}

/// A signed span of game time in whole minutes.
///
/// Negative durations are allowed (time may move backwards, ADR-001 §3).
/// Arithmetic is checked and never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration(i64);

impl Duration {
    /// Zero minutes.
    pub const ZERO: Self = Self(0);

    /// A duration of `minutes` minutes.
    pub fn from_minutes(minutes: i64) -> Self {
        Self(minutes)
    }

    /// A duration of `hours` hours; `None` on overflow.
    pub fn from_hours(hours: i64) -> Option<Self> {
        hours.checked_mul(MINUTES_PER_HOUR).map(Self)
    }

    /// A duration of `days` days; `None` on overflow.
    pub fn from_days(days: i64) -> Option<Self> {
        days.checked_mul(MINUTES_PER_DAY).map(Self)
    }

    /// The length in minutes.
    pub fn minutes(self) -> i64 {
        self.0
    }

    /// Whether the duration is below zero.
    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// `self + other`; `None` on overflow.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    /// `self - other`; `None` on overflow.
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }

    /// `-self`; `None` on overflow (only for the most negative duration).
    pub fn checked_neg(self) -> Option<Self> {
        self.0.checked_neg().map(Self)
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 0 {
            f.write_str("-")?;
        }
        // unsigned_abs handles i64::MIN without overflow.
        let total = self.0.unsigned_abs();
        let per_day = MINUTES_PER_DAY.unsigned_abs();
        let per_hour = MINUTES_PER_HOUR.unsigned_abs();
        let (days, hours, minutes) = (
            total / per_day,
            total % per_day / per_hour,
            total % per_hour,
        );
        if days > 0 {
            write!(f, "{days}d")?;
        }
        if hours > 0 {
            write!(f, "{hours}h")?;
        }
        if minutes > 0 || total == 0 {
            write!(f, "{minutes}m")?;
        }
        Ok(())
    }
}

impl FromStr for Duration {
    type Err = CoreError;

    /// Parses `15m`, `2h`, `1d`, `1d2h30m`, `-45m` and so on.
    fn from_str(s: &str) -> Result<Self> {
        let err = |reason| CoreError::InvalidDuration {
            text: s.to_owned(),
            reason,
        };
        let (negative, mut rest) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        if rest.is_empty() {
            return Err(err("expected components such as `15m`, `2h` or `1d`"));
        }
        // Units in the only allowed order, with their size in minutes.
        let units: [(char, u64); 3] = [
            ('d', MINUTES_PER_DAY.unsigned_abs()),
            ('h', MINUTES_PER_HOUR.unsigned_abs()),
            ('m', 1),
        ];
        let mut next_unit = 0;
        let mut total: u64 = 0;
        while !rest.is_empty() {
            let unit_at = rest
                .find(|c: char| !c.is_ascii_digit())
                .ok_or_else(|| err("number is missing a unit (`d`, `h` or `m`)"))?;
            let number = parse_number(&rest[..unit_at]).map_err(err)?;
            let unit = rest[unit_at..].chars().next().unwrap_or(' ');
            let position = units[next_unit..]
                .iter()
                .position(|(u, _)| *u == unit)
                .ok_or_else(|| {
                    if units.iter().any(|(u, _)| *u == unit) {
                        err("units must appear at most once, in the order `d`, `h`, `m`")
                    } else {
                        err("unknown unit; expected `d`, `h` or `m`")
                    }
                })?;
            let size = units[next_unit + position].1;
            next_unit += position + 1;
            total = number
                .checked_mul(size)
                .and_then(|m| total.checked_add(m))
                .ok_or_else(|| err("duration is too large"))?;
            rest = &rest[unit_at + unit.len_utf8()..];
        }
        let minutes = if negative {
            // Allows exactly i64::MIN, the negation of i64::MAX + 1.
            0i64.checked_sub_unsigned(total)
        } else {
            i64::try_from(total).ok()
        };
        minutes
            .map(Self)
            .ok_or_else(|| err("duration is too large"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Time {
        s.parse().unwrap()
    }

    fn d(s: &str) -> Duration {
        s.parse().unwrap()
    }

    #[test]
    fn time_literals_match_the_adr() {
        assert_eq!(t("d1 00:00").minutes(), 0);
        assert_eq!(t("d2 00:00").minutes(), 1440);
        assert_eq!(t("d3 22:00").minutes(), 4200);
        assert_eq!(t("22:00"), t("d1 22:00"));
        assert_eq!(t("00:00"), Time::START);
        assert_eq!(t("d1 23:59").minutes(), 1439);
    }

    #[test]
    fn time_display_round_trips() {
        for text in ["d1 00:00", "d3 22:00", "d10 09:05", "d1 23:59"] {
            assert_eq!(t(text).to_string(), text);
        }
        assert_eq!(t("22:00").to_string(), "d1 22:00");
        for minutes in [0, 1, 59, 60, 1439, 1440, 4200, i64::MAX - 1, i64::MAX] {
            let time = Time::from_minutes(minutes).unwrap();
            assert_eq!(t(&time.to_string()), time, "{minutes}");
        }
    }

    #[test]
    fn time_parts() {
        let time = t("d3 22:07");
        assert_eq!((time.day(), time.hour(), time.minute()), (3, 22, 7));
    }

    #[test]
    fn time_rejects_bad_literals() {
        for bad in [
            "",
            "d0 00:00",
            "d01 00:00",
            "d 10:00",
            "d1 24:00",
            "d1 23:60",
            "9:00",
            "09:0",
            "d1  09:00",
            "d1\t09:00",
            "d1 09:00 ",
            "D1 09:00",
            "d-1 09:00",
            "d+1 09:00",
            "+09:00",
            "09:00:00",
            "d99999999999999999999 00:00",
            "d9223372036854775807 00:00",
        ] {
            assert!(bad.parse::<Time>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn time_constructor_validates() {
        assert_eq!(Time::from_day_hm(3, 22, 0).unwrap().minutes(), 4200);
        assert!(Time::from_day_hm(0, 0, 0).is_err());
        assert!(Time::from_day_hm(1, 24, 0).is_err());
        assert!(Time::from_day_hm(1, 0, 60).is_err());
        assert!(Time::from_day_hm(i64::MAX, 0, 0).is_err());
        assert_eq!(Time::from_minutes(-1), None);
    }

    #[test]
    fn time_arithmetic_is_checked() {
        let start = t("d1 23:00");
        assert_eq!(start.checked_add(d("2h")), Some(t("d2 01:00")));
        assert_eq!(start.checked_sub(d("23h")), Some(Time::START));
        assert_eq!(start.checked_sub(d("1d")), None);
        assert_eq!(start.checked_add(d("-1d")), None);
        assert_eq!(start.checked_add(d("-1h")), Some(t("22:00")));
        let max = Time::from_minutes(i64::MAX).unwrap();
        assert_eq!(max.checked_add(d("1m")), None);
        assert_eq!(max.checked_sub(Duration::from_minutes(i64::MIN)), None);
    }

    #[test]
    fn time_difference() {
        assert_eq!(t("d2 01:00").duration_since(t("d1 23:00")), d("2h"));
        assert_eq!(t("d1 23:00").duration_since(t("d2 01:00")), d("-2h"));
        let max = Time::from_minutes(i64::MAX).unwrap();
        assert_eq!(max.duration_since(Time::START).minutes(), i64::MAX);
        assert_eq!(Time::START.duration_since(max).minutes(), -i64::MAX);
    }

    #[test]
    fn duration_literals() {
        assert_eq!(d("15m").minutes(), 15);
        assert_eq!(d("2h").minutes(), 120);
        assert_eq!(d("1d").minutes(), 1440);
        assert_eq!(d("1h30m").minutes(), 90);
        assert_eq!(d("2d12h").minutes(), 3600);
        assert_eq!(d("1d1h1m").minutes(), 1501);
        assert_eq!(d("90m").minutes(), 90);
        assert_eq!(d("0m").minutes(), 0);
        assert_eq!(d("-45m").minutes(), -45);
        assert_eq!(d("-1h30m").minutes(), -90);
    }

    #[test]
    fn duration_display_is_canonical_and_round_trips() {
        for (text, shown) in [
            ("15m", "15m"),
            ("2h", "2h"),
            ("1d", "1d"),
            ("90m", "1h30m"),
            ("0m", "0m"),
            ("0d", "0m"),
            ("-0m", "0m"),
            ("1d0h5m", "1d5m"),
            ("-1h30m", "-1h30m"),
            ("25h", "1d1h"),
        ] {
            assert_eq!(d(text).to_string(), shown, "{text}");
        }
        for minutes in [0, 1, -1, 59, 60, 61, 1440, 1501, i64::MAX, i64::MIN] {
            let duration = Duration::from_minutes(minutes);
            assert_eq!(d(&duration.to_string()), duration, "{minutes}");
        }
    }

    #[test]
    fn duration_rejects_bad_literals() {
        for bad in [
            "",
            "-",
            "15",
            "m",
            "15x",
            "15 m",
            " 15m",
            "15m ",
            "1m1h",
            "1h1h",
            "1h1d",
            "05m",
            "+5m",
            "--5m",
            "1.5h",
            "15M",
            "9223372036854775808m",
            "-9223372036854775809m",
            "6405119470038039d",
        ] {
            assert!(bad.parse::<Duration>().is_err(), "{bad:?}");
        }
        assert_eq!(d("-9223372036854775808m").minutes(), i64::MIN);
        assert_eq!(d("9223372036854775807m").minutes(), i64::MAX);
    }

    #[test]
    fn duration_constructors_and_arithmetic() {
        assert_eq!(Duration::from_hours(2), Some(d("2h")));
        assert_eq!(Duration::from_days(1), Some(d("1d")));
        assert_eq!(Duration::from_hours(i64::MAX), None);
        assert_eq!(Duration::from_days(i64::MIN), None);
        assert_eq!(d("1h").checked_add(d("30m")), Some(d("90m")));
        assert_eq!(d("1h").checked_sub(d("2h")), Some(d("-1h")));
        assert!(d("-1h").is_negative());
        assert_eq!(d("1h").checked_neg(), Some(d("-1h")));
        let max = Duration::from_minutes(i64::MAX);
        let min = Duration::from_minutes(i64::MIN);
        assert_eq!(max.checked_add(d("1m")), None);
        assert_eq!(min.checked_sub(d("1m")), None);
        assert_eq!(min.checked_neg(), None);
        assert_eq!(Duration::ZERO, d("0m"));
    }
}
