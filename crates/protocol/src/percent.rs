use serde::{Deserialize, Serialize};

/// A whole percentage, 0 to 100. Nothing else can be made, also not from the
/// wire: a message with a value above 100 fails to read.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(try_from = "u8", into = "u8")]
pub struct Percent(u8);

impl Percent {
    /// 0 %.
    pub const MIN: Self = Self(0);
    /// 100 %.
    pub const MAX: Self = Self(100);

    /// `value` percent; `None` above 100.
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// The value in percent, 0 to 100.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// `self` plus `amount` percent, at most 100 %.
    #[must_use]
    pub const fn saturating_add(self, amount: u8) -> Self {
        let sum = self.0.saturating_add(amount);
        if sum > Self::MAX.0 {
            Self::MAX
        } else {
            Self(sum)
        }
    }

    /// `self` minus `amount` percent, at least 0 %.
    #[must_use]
    pub const fn saturating_sub(self, amount: u8) -> Self {
        Self(self.0.saturating_sub(amount))
    }
}

/// A value above 100 where a [`Percent`] was expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0} % is above 100 %")]
pub struct AbovePercentRange(pub u8);

impl TryFrom<u8> for Percent {
    type Error = AbovePercentRange;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(AbovePercentRange(value))
    }
}

impl From<Percent> for u8 {
    fn from(percent: Percent) -> Self {
        percent.0
    }
}
