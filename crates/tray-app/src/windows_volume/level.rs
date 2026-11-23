//! Volume levels between Windows and the Tray core. Pure, so the rounding is
//! tested without an audio device.

use tray_core::volume::Percent;

/// The level for a Windows volume scalar (0.0 to 1.0), rounded to the nearest
/// percent as the Windows volume flyout shows it.
pub fn from_scalar(scalar: f32) -> Percent {
    // 0 to 100 after the clamp; NaN, which Windows never reports, becomes 0.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let level = (scalar.clamp(0.0, 1.0) * 100.0).round() as u8;
    Percent::new(level).unwrap_or(Percent::MAX)
}

/// The Windows volume scalar for `level`.
pub fn to_scalar(level: Percent) -> f32 {
    f32::from(level.get()) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn percent(value: u8) -> Percent {
        Percent::new(value).expect("a test level is at most 100")
    }

    #[test]
    fn every_level_survives_the_trip_through_windows() {
        for value in 0..=100 {
            let level = percent(value);
            assert_eq!(from_scalar(to_scalar(level)), level);
        }
    }

    #[test]
    fn a_scalar_between_two_levels_rounds_to_the_nearer_one() {
        assert_eq!(from_scalar(0.337), percent(34));
        assert_eq!(from_scalar(0.332), percent(33));
        assert_eq!(from_scalar(0.996), percent(100));
        assert_eq!(from_scalar(0.004), percent(0));
    }

    #[test]
    fn scalars_outside_the_range_are_clamped() {
        assert_eq!(from_scalar(-0.1), percent(0));
        assert_eq!(from_scalar(1.5), percent(100));
    }
}
