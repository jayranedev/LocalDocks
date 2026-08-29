//! Timestamp formatting.
//!
//! The IPC contract uses ISO-8601 UTC strings (`capturedAt`, `startedAt`)
//! because the frontend parses them with `new Date(...)`, and because
//! `startedAt` is half of a process's identity — it must serialise to a stable,
//! exact string.
//!
//! Hand-rolled rather than pulling in `chrono`: this is ~30 lines of civil-date
//! arithmetic, it is the only date work the backend does, and the project rule
//! is to keep dependencies minimal. Being pure and syscall-free, it is also
//! exactly the kind of code docs/BACKEND.md says belongs in unit tests.

use std::time::{SystemTime, UNIX_EPOCH};

/// Days since 1970-01-01 -> (year, month, day).
///
/// Howard Hinnant's `civil_from_days`, which is correct for the proleptic
/// Gregorian calendar over the whole i64 range — including leap years and
/// dates before the epoch. All arithmetic is signed to avoid underflow on
/// negative days.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Shift the era so that it begins on 0000-03-01, which makes leap day the
    // last day of the year and removes the special case entirely.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // day of era, [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // month, March-based, [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]

    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Milliseconds since the Unix epoch -> `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn to_iso8601(unix_millis: i64) -> String {
    // Euclidean division so pre-epoch values floor correctly rather than
    // truncating toward zero.
    let secs = unix_millis.div_euclid(1000);
    let millis = unix_millis.rem_euclid(1000);

    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400); // seconds of day, always [0, 86399]

    let (y, m, d) = civil_from_days(days);
    let (h, min, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);

    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{millis:03}Z")
}

/// Current time as milliseconds since the Unix epoch.
///
/// A scan needs the capture instant twice — once as the ISO-8601 `capturedAt`
/// string and once as a number, to subtract process creation times from. Taking
/// the reading once and deriving both keeps every `uptimeSeconds` in a snapshot
/// measured against the same instant.
pub fn now_unix_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        // System clock set before 1970. Vanishingly unlikely, but returning a
        // negative offset is more honest than panicking on a clock reading.
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

/// Number of 100-nanosecond intervals between 1601-01-01 and 1970-01-01.
///
/// Windows counts time from the start of the Gregorian calendar's fourth
/// century-cycle; Unix counts from 1970. This is the gap.
const FILETIME_EPOCH_OFFSET_100NS: i64 = 116_444_736_000_000_000;

/// Windows `FILETIME` -> Unix milliseconds.
///
/// Takes the two halves as plain `u32`s rather than the `FILETIME` struct so
/// this stays platform-independent and testable without Windows — the pure
/// layer must not depend on `windows::`.
///
/// A zero FILETIME means "not set" and maps to the Unix epoch rather than to a
/// nonsensical 1601 date.
pub fn filetime_to_unix_millis(low: u32, high: u32) -> i64 {
    let ticks = ((high as u64) << 32 | low as u64) as i64;
    if ticks == 0 {
        return 0;
    }
    (ticks - FILETIME_EPOCH_OFFSET_100NS) / 10_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_epoch() {
        assert_eq!(to_iso8601(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn keeps_millisecond_precision() {
        assert_eq!(to_iso8601(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(to_iso8601(999), "1970-01-01T00:00:00.999Z");
        assert_eq!(to_iso8601(1000), "1970-01-01T00:00:01.000Z");
    }

    #[test]
    fn formats_a_known_instant() {
        // 2026-08-28T09:00:00.000Z — the value used in the frontend tests.
        assert_eq!(to_iso8601(1_787_907_600_000), "2026-08-28T09:00:00.000Z");
    }

    #[test]
    fn handles_leap_days() {
        // 2024 is a leap year (divisible by 4), so 29 February exists...
        assert_eq!(to_iso8601(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
        assert_eq!(to_iso8601(1_709_251_200_000), "2024-03-01T00:00:00.000Z");

        // ...but 2100 is not (divisible by 100, not by 400), so the day after
        // 28 February is 1 March. This is the case a naive `year % 4` gets wrong.
        assert_eq!(to_iso8601(4_107_456_000_000), "2100-02-28T00:00:00.000Z");
        assert_eq!(to_iso8601(4_107_542_400_000), "2100-03-01T00:00:00.000Z");

        // 2000 was a leap year (divisible by 400) — the other half of the rule.
        assert_eq!(to_iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn handles_year_boundaries() {
        assert_eq!(to_iso8601(1_767_225_599_999), "2025-12-31T23:59:59.999Z");
        assert_eq!(to_iso8601(1_767_225_600_000), "2026-01-01T00:00:00.000Z");
    }

    #[test]
    fn floors_rather_than_truncating_before_the_epoch() {
        // Truncation toward zero would give "1969-12-31T23:59:59.-001Z".
        assert_eq!(to_iso8601(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(to_iso8601(-86_400_000), "1969-12-31T00:00:00.000Z");
    }

    #[test]
    fn round_trips_every_day_for_a_decade() {
        // Guards the civil-date arithmetic against off-by-one drift: each day
        // must be exactly 86_400_000 ms after the previous one.
        let mut previous = to_iso8601(1_577_836_800_000); // 2020-01-01
        for day in 1..3653 {
            let ms = 1_577_836_800_000 + day * 86_400_000;
            let current = to_iso8601(ms);
            assert!(current > previous, "{current} should sort after {previous}");
            assert!(current.ends_with("T00:00:00.000Z"));
            previous = current;
        }
    }

    #[test]
    fn the_current_time_renders_as_a_well_formed_timestamp() {
        // Exercises the path a real scan takes: read the clock once, format it.
        let s = to_iso8601(now_unix_millis());
        assert_eq!(s.len(), 24, "expected YYYY-MM-DDTHH:MM:SS.mmmZ, got {s}");
        assert!(s.ends_with('Z'));
        assert!(s.starts_with("20"), "expected a 21st-century year, got {s}");
    }

    #[test]
    fn filetime_epoch_maps_to_unix_epoch() {
        // 116444736000000000 is 1970-01-01 expressed as a FILETIME.
        let ticks: u64 = 116_444_736_000_000_000;
        let (low, high) = (ticks as u32, (ticks >> 32) as u32);
        assert_eq!(filetime_to_unix_millis(low, high), 0);
        assert_eq!(
            to_iso8601(filetime_to_unix_millis(low, high)),
            "1970-01-01T00:00:00.000Z"
        );
    }

    #[test]
    fn filetime_zero_is_treated_as_unset() {
        // Rather than reporting a 1601 date, which would look like real data.
        assert_eq!(filetime_to_unix_millis(0, 0), 0);
    }

    #[test]
    fn filetime_converts_a_known_instant() {
        // 2026-08-28T09:00:00.000Z
        let ticks: u64 = 116_444_736_000_000_000 + 1_787_907_600_000 * 10_000;
        let (low, high) = (ticks as u32, (ticks >> 32) as u32);
        assert_eq!(
            to_iso8601(filetime_to_unix_millis(low, high)),
            "2026-08-28T09:00:00.000Z"
        );
    }

    #[test]
    fn filetime_splits_the_64_bit_value_correctly() {
        // Guards against swapping low/high, which would silently produce dates
        // centuries away rather than failing loudly.
        let ticks: u64 = 116_444_736_000_000_000 + 86_400_000 * 10_000; // 1970-01-02
        let (low, high) = (ticks as u32, (ticks >> 32) as u32);
        assert_eq!(
            to_iso8601(filetime_to_unix_millis(low, high)),
            "1970-01-02T00:00:00.000Z"
        );
        assert_ne!(
            to_iso8601(filetime_to_unix_millis(high, low)),
            "1970-01-02T00:00:00.000Z"
        );
    }
}
