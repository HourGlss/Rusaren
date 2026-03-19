use crate::{DomainError, MAX_ROUNDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoundNumber(u8);

impl RoundNumber {
    /// # Errors
    ///
    /// Returns [`DomainError::RoundOutOfRange`] when `value` is outside
    /// `1..=MAX_ROUNDS`.
    pub fn new(value: u8) -> Result<Self, DomainError> {
        if !(1..=MAX_ROUNDS).contains(&value) {
            return Err(DomainError::RoundOutOfRange {
                round: value,
                min: 1,
                max: MAX_ROUNDS,
            });
        }

        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub fn next(self) -> Option<Self> {
        if self.0 == MAX_ROUNDS {
            None
        } else {
            Some(Self(self.0 + 1))
        }
    }
}
