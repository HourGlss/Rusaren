use crate::DomainError;

macro_rules! non_zero_id {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// # Errors
            ///
            /// Returns [`DomainError::IdMustBeNonZero`] when `value` is zero.
            pub fn new(value: u32) -> Result<Self, DomainError> {
                if value == 0 {
                    return Err(DomainError::IdMustBeNonZero($label));
                }

                Ok(Self(value))
            }

            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }
    };
}

non_zero_id!(PlayerId, "player_id");
non_zero_id!(LobbyId, "lobby_id");
non_zero_id!(MatchId, "match_id");
non_zero_id!(EntityId, "entity_id");
