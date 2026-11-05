//! When a routine runs (`[[agents.routines]]`): a cron of five fields (`0 3 * * 1-5`), in a time
//! zone or the computer's, or a pace (`every = "6h"`). Read here, its next times computed here;
//! the window writes it in words.

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use jiff::{civil, tz::TimeZone, Timestamp};

/// A cron's five fields, each the set of values it allows.
#[derive(Debug, Clone, PartialEq)]
pub struct Cron {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    days: Vec<u8>,
    months: Vec<u8>,
    weekdays: Vec<u8>,
    /// The day of the month is restricted (not `*`): with the weekday restricted too, either matches.
    days_set: bool,
    weekdays_set: bool,
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

impl Cron {
    pub fn parse(text: &str) -> Result<Self> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        ensure!(fields.len() == 5, "{}", prism_base::i18n::tr("a cron has five fields: minute hour day month weekday"));
        let minutes = field(fields[0], 0, 59, &[])?;
        let hours = field(fields[1], 0, 23, &[])?;
        let days = field(fields[2], 1, 31, &[])?;
        let months = field(fields[3], 1, 12, &MONTHS)?;
        // 7 is Sunday too.
        let mut weekdays: Vec<u8> = field(fields[4], 0, 7, &WEEKDAYS)?.into_iter().map(|d| d % 7).collect();
        weekdays.sort_unstable();
        weekdays.dedup();
        Ok(Cron { minutes, hours, days, months, weekdays, days_set: fields[2] != "*", weekdays_set: fields[4] != "*" })
    }

    fn day_matches(&self, d: civil::Date) -> bool {
        if !self.months.contains(&(d.month() as u8)) {
            return false;
        }
        let day = self.days.contains(&(d.day() as u8));
        let weekday = self.weekdays.contains(&(d.weekday().to_sunday_zero_offset() as u8));
        match (self.days_set, self.weekdays_set) {
            (true, true) => day || weekday,
            (true, false) => day,
            (false, true) => weekday,
            (false, false) => true,
        }
    }

    /// The first time strictly after `after` that it names, in `tz`; none within five years
    /// (`0 0 31 2 *`).
    pub fn next_after(&self, after: DateTime<Utc>, tz: &TimeZone) -> Option<DateTime<Utc>> {
        let from = Timestamp::from_second(after.timestamp()).ok()?;
        let mut date = from.to_zoned(tz.clone()).date();
        for _ in 0..366 * 5 {
            if self.day_matches(date) {
                for &h in &self.hours {
                    for &m in &self.minutes {
                        // A time the clock skips (summer time) runs at the first one after it.
                        let Ok(at) = date.at(h as i8, m as i8, 0, 0).to_zoned(tz.clone()) else { continue };
                        if at.timestamp() > from {
                            return DateTime::from_timestamp(at.timestamp().as_second(), 0);
                        }
                    }
                }
            }
            date = date.tomorrow().ok()?;
        }
        None
    }
}

/// One field: `*`, `5`, `1-5`, `*/15`, `10-40/10`, `mon-fri`, lists of them with commas.
fn field(text: &str, min: u8, max: u8, names: &[&str]) -> Result<Vec<u8>> {
    let value = |s: &str| -> Result<u8> {
        let lower = s.to_ascii_lowercase();
        if let Some(i) = names.iter().position(|n| *n == lower) {
            // Months count from 1, weekdays from 0.
            return Ok(i as u8 + min);
        }
        let v: u8 = s.parse().with_context(|| prism_base::tr!("{value} is not a number of a cron", value = s))?;
        ensure!((min..=max).contains(&v), "{}", prism_base::tr!("{value} is out of {min}-{max}", value = v, min = min, max = max));
        Ok(v)
    };
    let mut out = Vec::new();
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u8>().ok().filter(|s| *s > 0).with_context(|| prism_base::tr!("{value} is not a step of a cron", value = s))?),
            None => (part, 1),
        };
        let (from, to) = match range {
            "*" => (min, max),
            r => match r.split_once('-') {
                Some((a, b)) => (value(a)?, value(b)?),
                // `5/10`: from 5 to the end.
                None if part.contains('/') => (value(r)?, max),
                None => {
                    let v = value(r)?;
                    (v, v)
                }
            },
        };
        ensure!(from <= to, "{}", prism_base::tr!("{range}: its start is after its end", range = range));
        out.extend((from..=to).step_by(step as usize));
    }
    if out.is_empty() {
        bail!("{}", prism_base::tr!("{field}: no value", field = text));
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// `every = "6h"`: a number and its unit (`m`, `h`, `d`), several in a row (`1h30m`); a minute at least.
pub fn every(text: &str) -> Result<chrono::Duration> {
    let mut total = 0i64;
    let mut digits = String::new();
    for c in text.trim().chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        let n: i64 = digits.parse().with_context(|| prism_base::tr!("{value}: a number, then m, h or d", value = text))?;
        digits.clear();
        total += n * match c {
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            _ => bail!("{}", prism_base::tr!("{value}: a number, then m, h or d", value = text)),
        };
    }
    ensure!(digits.is_empty() && total > 0, "{}", prism_base::tr!("{value}: a number, then m, h or d", value = text));
    ensure!(total >= 60, "{}", prism_base::i18n::tr("a minute at least"));
    Ok(chrono::Duration::seconds(total))
}

/// The time zone named (`Europe/Paris`), otherwise the computer's.
pub fn zone(name: Option<&str>) -> Result<TimeZone> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => TimeZone::get(n).with_context(|| prism_base::tr!("{zone} is not a time zone", zone = n)),
        None => Ok(TimeZone::system()),
    }
}

/// When a routine runs: its cron in its zone, or its pace.
#[derive(Debug, Clone)]
pub enum When {
    Cron(Box<Cron>, TimeZone),
    Every(chrono::Duration),
}

impl When {
    /// From its keys: `cron` or `every`, one only.
    pub fn of(cron: Option<&str>, every_: Option<&str>, time_zone: Option<&str>) -> Result<Self> {
        fn some(s: Option<&str>) -> Option<&str> {
            s.map(str::trim).filter(|s| !s.is_empty())
        }
        match (some(cron), some(every_)) {
            (Some(_), Some(_)) => bail!("{}", prism_base::i18n::tr("cron and every exclude each other")),
            (None, None) => bail!("{}", prism_base::i18n::tr("a cron or every")),
            (Some(c), None) => Ok(When::Cron(Box::new(Cron::parse(c)?), zone(time_zone)?)),
            (None, Some(e)) => Ok(When::Every(every(e)?)),
        }
    }

    /// The next time after `after`; for a pace, `after` is its last run.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            When::Cron(c, tz) => c.next_after(after, tz),
            When::Every(d) => Some(after + *d),
        }
    }

    /// The last time it named at or before `now`, after `since` (excluded), if any: what is due.
    pub fn due(&self, since: DateTime<Utc>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            When::Every(d) => {
                if now < since + *d {
                    return None;
                }
                // The last whole step before now: a long sleep is one due time, not a burst.
                let steps = (now - since).num_seconds() / d.num_seconds().max(1);
                Some(since + *d * steps as i32)
            }
            When::Cron(..) => {
                let mut at = self.next_after(since)?;
                if at > now {
                    return None;
                }
                // At most a day of minutes: a computer asleep for weeks catches up once, from the last.
                for _ in 0..1500 {
                    match self.next_after(at) {
                        Some(n) if n <= now => at = n,
                        _ => return Some(at),
                    }
                }
                // Still behind: the times before `now` too many to walk, the one just before it.
                let mut back = now - chrono::Duration::days(1);
                while let Some(n) = self.next_after(back).filter(|n| *n <= now) {
                    at = n;
                    back = n;
                }
                Some(at)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn cron_fields_read() {
        let c = Cron::parse("*/15 3 * * mon-fri").unwrap();
        assert_eq!(c.minutes, [0, 15, 30, 45]);
        assert_eq!(c.weekdays, [1, 2, 3, 4, 5]);
        assert_eq!(Cron::parse("0 0 * * 7").unwrap().weekdays, [0], "7 is Sunday");
        assert_eq!(Cron::parse("0 0 1 jan,jul *").unwrap().months, [1, 7]);
        assert_eq!(Cron::parse("5/20 * * * *").unwrap().minutes, [5, 25, 45]);
        for bad in ["* * * *", "60 * * * *", "* 24 * * *", "*/0 * * * *", "5-1 * * * *", "x * * * *"] {
            assert!(Cron::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn next_times_in_a_zone() {
        let paris = TimeZone::get("Europe/Paris").unwrap();
        let c = Cron::parse("0 3 * * 1-5").unwrap();
        // Thursday 5 November 2026, 10:00 in Paris (09:00 UTC): Friday 03:00 Paris, 02:00 UTC.
        assert_eq!(c.next_after(utc("2026-11-05T09:00:00Z"), &paris), Some(utc("2026-11-06T02:00:00Z")));
        // Friday after 03:00: Monday.
        assert_eq!(c.next_after(utc("2026-11-06T02:00:00Z"), &paris), Some(utc("2026-11-09T02:00:00Z")));
        // Day of month and weekday both set: either.
        let either = Cron::parse("0 12 1 * 5").unwrap();
        assert_eq!(either.next_after(utc("2026-11-05T13:00:00Z"), &TimeZone::UTC), Some(utc("2026-11-06T12:00:00Z")));
        assert_eq!(Cron::parse("0 0 31 2 *").unwrap().next_after(utc("2026-01-01T00:00:00Z"), &TimeZone::UTC), None);
    }

    #[test]
    fn paces_read() {
        assert_eq!(every("30m").unwrap().num_minutes(), 30);
        assert_eq!(every("1h30m").unwrap().num_minutes(), 90);
        assert_eq!(every("2d").unwrap().num_hours(), 48);
        for bad in ["", "30", "m", "30s", "0m", "3w"] {
            assert!(every(bad).is_err(), "{bad}");
        }
        assert!(When::of(Some("* * * * *"), Some("1h"), None).is_err(), "cron and every exclude each other");
        assert!(When::of(None, None, None).is_err());
        assert!(When::of(Some("0 3 * * *"), None, Some("Mars/Olympus")).is_err());
    }

    #[test]
    fn what_is_due() {
        let w = When::of(Some("*/2 * * * *"), None, Some("UTC")).unwrap();
        assert_eq!(w.due(utc("2026-11-05T10:00:00Z"), utc("2026-11-05T10:01:30Z")), None);
        assert_eq!(w.due(utc("2026-11-05T10:00:00Z"), utc("2026-11-05T10:02:10Z")), Some(utc("2026-11-05T10:02:00Z")));
        // Asleep for a while: the last time only.
        assert_eq!(w.due(utc("2026-11-05T10:00:00Z"), utc("2026-11-05T14:07:00Z")), Some(utc("2026-11-05T14:06:00Z")));
        assert_eq!(w.due(utc("2026-10-01T10:00:00Z"), utc("2026-11-05T14:07:00Z")), Some(utc("2026-11-05T14:06:00Z")), "weeks asleep");
        let pace = When::of(None, Some("30m"), None).unwrap();
        assert_eq!(pace.due(utc("2026-11-05T10:00:00Z"), utc("2026-11-05T10:29:00Z")), None);
        assert_eq!(pace.due(utc("2026-11-05T10:00:00Z"), utc("2026-11-05T11:10:00Z")), Some(utc("2026-11-05T11:00:00Z")));
    }
}
