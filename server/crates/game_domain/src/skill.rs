use std::collections::BTreeMap;
use std::fmt;

use crate::{DomainError, MAX_SKILL_TIER, MAX_SKILL_TREE_NAME_LEN};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KnownSkillTree {
    Warrior,
    Rogue,
    Mage,
    Cleric,
}

impl KnownSkillTree {
    pub const ALL: [Self; 4] = [Self::Warrior, Self::Rogue, Self::Mage, Self::Cleric];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warrior => "Warrior",
            Self::Rogue => "Rogue",
            Self::Mage => "Mage",
            Self::Cleric => "Cleric",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillTree {
    Known(KnownSkillTree),
    Custom(String),
}

impl SkillTree {
    #[allow(non_upper_case_globals)]
    pub const Warrior: Self = Self::Known(KnownSkillTree::Warrior);
    #[allow(non_upper_case_globals)]
    pub const Rogue: Self = Self::Known(KnownSkillTree::Rogue);
    #[allow(non_upper_case_globals)]
    pub const Mage: Self = Self::Known(KnownSkillTree::Mage);
    #[allow(non_upper_case_globals)]
    pub const Cleric: Self = Self::Known(KnownSkillTree::Cleric);

    /// # Errors
    ///
    /// Returns a [`DomainError`] when the trimmed tree name is empty, too long,
    /// or contains unsupported characters.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            return Err(DomainError::SkillTreeNameEmpty);
        }

        if trimmed.len() > MAX_SKILL_TREE_NAME_LEN {
            return Err(DomainError::SkillTreeNameTooLong {
                len: trimmed.len(),
                max: MAX_SKILL_TREE_NAME_LEN,
            });
        }

        if let Some(ch) = trimmed
            .chars()
            .find(|ch| !ch.is_ascii_alphanumeric() && *ch != '_' && *ch != '-' && *ch != ' ')
        {
            return Err(DomainError::SkillTreeNameInvalidCharacter { ch });
        }

        if let Some(known) = KnownSkillTree::ALL
            .iter()
            .copied()
            .find(|known| known.as_str().eq_ignore_ascii_case(trimmed))
        {
            return Ok(Self::Known(known));
        }

        Ok(Self::Custom(trimmed.to_owned()))
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::new(raw).ok()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(known) => known.as_str(),
            Self::Custom(name) => name.as_str(),
        }
    }
}

impl fmt::Display for SkillTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillChoice {
    pub tree: SkillTree,
    pub tier: u8,
}

impl SkillChoice {
    /// # Errors
    ///
    /// Returns [`DomainError::SkillTierOutOfRange`] when `tier` is outside
    /// `1..=MAX_SKILL_TIER`.
    pub fn new(tree: SkillTree, tier: u8) -> Result<Self, DomainError> {
        if !(1..=MAX_SKILL_TIER).contains(&tier) {
            return Err(DomainError::SkillTierOutOfRange {
                tier,
                min: 1,
                max: MAX_SKILL_TIER,
            });
        }

        Ok(Self { tree, tier })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadoutProgress {
    unlocked_tiers: BTreeMap<SkillTree, u8>,
}

impl LoadoutProgress {
    #[must_use]
    pub fn new() -> Self {
        Self {
            unlocked_tiers: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn tier_for(&self, tree: &SkillTree) -> u8 {
        self.unlocked_tiers.get(tree).copied().unwrap_or(0)
    }

    /// # Errors
    ///
    /// Returns [`DomainError::SkillTierGap`] when `choice` does not continue the
    /// caller's current progression for that tree.
    pub fn can_apply(&self, choice: &SkillChoice) -> Result<(), DomainError> {
        let current = self.tier_for(&choice.tree);
        let expected = if current == 0 { 1 } else { current + 1 };

        if choice.tier != expected {
            return Err(DomainError::SkillTierGap {
                tree: choice.tree.clone(),
                expected,
                actual: choice.tier,
            });
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Returns the same errors as [`LoadoutProgress::can_apply`] when `choice`
    /// is not the next valid skill tier for its tree.
    pub fn apply(&mut self, choice: &SkillChoice) -> Result<(), DomainError> {
        self.can_apply(choice)?;
        self.unlocked_tiers.insert(choice.tree.clone(), choice.tier);
        Ok(())
    }
}
