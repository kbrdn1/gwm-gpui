//! A branch's age, as the sidebar shows it. The contract gives `age_seconds`,
//! the branch's age against the trunk baseline; it is read the way Monocode
//! reads a session's time (`relative` in `crates/monocode-core/src/time.rs`, and
//! `age_label` in `shell/sidebar_list.rs` for the day buckets).

use std::time::{SystemTime, UNIX_EPOCH};

const MONTHS: [&str; 12] = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The local calendar day a moment falls on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Day {
  /// Days since the epoch, in local time: two moments are on the same day when
  /// their indexes are equal.
  pub index: i64,
  /// 0 for January.
  pub month: u32,
  pub day: u32,
}

/// Seconds since the epoch.
pub fn now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| since.as_secs().cast_signed())
}

pub fn local_day(unix: i64) -> Day {
  // SAFETY: `localtime_r` only writes the `tm` it is given. A zeroed `tm` is a
  // valid value of the type, and stays one if the call fails.
  let mut tm: libc::tm = unsafe { std::mem::zeroed() };
  unsafe { libc::localtime_r(&unix, &mut tm) };
  Day {
    index: (unix + tm.tm_gmtoff).div_euclid(86_400),
    month: tm.tm_mon.clamp(0, 11).cast_unsigned(),
    day: tm.tm_mday.cast_unsigned(),
  }
}

/// Ages as of now. gwm computes `age_seconds` when it scans, and its daemon
/// sends no new snapshot when only ages change: an age is anchored to when its
/// snapshot arrived, then counted on from there.
#[derive(Clone, Copy, Debug)]
pub struct Ages {
  /// When the snapshot arrived, in seconds since the epoch.
  pub received_at: i64,
  pub now: i64,
}

impl Ages {
  fn started(self, age_seconds: u64) -> i64 {
    self.received_at - age_seconds.cast_signed()
  }

  pub fn label(self, age_seconds: u64) -> String {
    let started = self.started(age_seconds);
    label((self.now - started).max(0).cast_unsigned(), local_day(started))
  }

  pub fn bucket(self, age_seconds: u64) -> Bucket {
    Bucket::of(local_day(self.started(age_seconds)), local_day(self.now))
  }

  /// The age of a moment gwm dates itself, in seconds since the epoch (a
  /// session's last activity): counted from that moment, whenever the
  /// snapshot arrived.
  pub fn since(self, unix: u64) -> String {
    let at = unix.cast_signed();
    label((self.now - at).max(0).cast_unsigned(), local_day(at))
  }
}

/// "now", "25m", "1h 37m", "6d", then the day the branch started, "Oct 22":
/// Monocode's format.
pub fn label(age_seconds: u64, started: Day) -> String {
  let minutes = age_seconds / 60;
  let hours = minutes / 60;
  let days = hours / 24;
  match () {
    () if minutes == 0 => "now".to_owned(),
    () if hours == 0 => format!("{minutes}m"),
    () if days == 0 && minutes.is_multiple_of(60) => format!("{hours}h"),
    () if days == 0 => format!("{hours}h {}m", minutes % 60),
    () if days < 7 => format!("{days}d"),
    () => format!("{} {}", MONTHS[started.month as usize], started.day),
  }
}

/// The sidebar's groups, by local calendar days between the branch's start and
/// today, as Monocode groups sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
  Today,
  Yesterday,
  LastWeek,
  Older,
}

impl Bucket {
  pub fn of(started: Day, today: Day) -> Self {
    match today.index - started.index {
      ..=0 => Self::Today,
      1 => Self::Yesterday,
      2..=6 => Self::LastWeek,
      _ => Self::Older,
    }
  }

  pub fn title(self) -> &'static str {
    match self {
      Self::Today => "Today",
      Self::Yesterday => "Yesterday",
      Self::LastWeek => "Last 7 days",
      Self::Older => "Older",
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const OCT_22: Day = Day {
    index: 0,
    month: 9,
    day: 22,
  };

  #[test]
  fn labels_follow_monocode() {
    let cases = [
      (59, "now"),
      (60, "1m"),
      (25 * 60, "25m"),
      (3600, "1h"),
      (3600 + 37 * 60, "1h 37m"),
      (7 * 3600 + 6 * 60 + 59, "7h 6m"),
      (6 * 86_400 + 3600, "6d"),
      (7 * 86_400, "Oct 22"),
    ];
    for (seconds, expected) in cases {
      assert_eq!(label(seconds, OCT_22), expected, "{seconds} s");
    }
  }

  /// The daemon sends no snapshot when only ages change: an hour later, the
  /// branch must still be an hour older.
  #[test]
  fn an_age_keeps_counting_after_its_snapshot() {
    let ages = Ages {
      received_at: 1_000_000,
      now: 1_000_000 + 3600,
    };
    assert_eq!(ages.label(25 * 60), "1h 25m");
  }

  /// gwm's clock ahead of this one is "now", not a negative age.
  #[test]
  fn a_moment_gwm_dates_is_aged_from_itself_not_from_its_snapshot() {
    let ages = Ages {
      received_at: 1_000_000,
      now: 1_000_000 + 3600,
    };
    assert_eq!(ages.since(1_000_000 + 3600 - 25 * 60), "25m");
    assert_eq!(ages.since(1_000_000 - 3600), "2h");
    assert_eq!(ages.since(1_000_000 + 7200), "now");
  }

  #[test]
  fn buckets_count_calendar_days_not_hours() {
    let day = |index| Day { index, ..OCT_22 };
    assert_eq!(Bucket::of(day(10), day(10)), Bucket::Today);
    assert_eq!(Bucket::of(day(9), day(10)), Bucket::Yesterday);
    assert_eq!(Bucket::of(day(4), day(10)), Bucket::LastWeek);
    assert_eq!(Bucket::of(day(3), day(10)), Bucket::Older);
  }
}
