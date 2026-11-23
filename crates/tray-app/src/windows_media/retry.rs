//! When to read the current Media session again after failed reads. Pure, so
//! the schedule is tested without Windows.

use std::time::Duration;

/// How long to wait before reading again, given how many reads in a row have
/// failed; `None` after a successful read (`failures == 0`), when only a
/// change event triggers the next read.
///
/// The delay grows from half a second and holds at five seconds while the
/// failures continue, so a stuck session costs little.
pub fn retry_delay(failures: u32) -> Option<Duration> {
    let millis = match failures {
        0 => return None,
        1 => 500,
        2 => 1_000,
        3 => 2_000,
        _ => 5_000,
    };
    Some(Duration::from_millis(millis))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: f64) -> Option<Duration> {
        Some(Duration::from_secs_f64(seconds))
    }

    #[test]
    fn after_a_successful_read_only_change_events_trigger_a_read() {
        assert_eq!(retry_delay(0), None);
    }

    #[test]
    fn the_delay_grows_with_each_failure_in_a_row() {
        assert_eq!(retry_delay(1), secs(0.5));
        assert_eq!(retry_delay(2), secs(1.0));
        assert_eq!(retry_delay(3), secs(2.0));
    }

    #[test]
    fn the_delay_holds_at_five_seconds_while_failures_continue() {
        assert_eq!(retry_delay(4), secs(5.0));
        assert_eq!(retry_delay(100), secs(5.0));
        assert_eq!(retry_delay(u32::MAX), secs(5.0));
    }
}
