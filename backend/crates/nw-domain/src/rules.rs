//! Business rules of the assets (`RuleAsset`), applied by the PHP code on
//! every manual add or update. The rules engine is not ported yet: this
//! module only tells whether an active rule could change a manual write,
//! so that such writes are left to the classic interface instead of
//! silently skipping the rule.

/// `Rule::PATTERN_IS`.
const PATTERN_IS: i32 = 0;
/// `Rule::PATTERN_IS_NOT`.
const PATTERN_IS_NOT: i32 = 1;

/// Write to which a rule applies (`RuleAsset::ONADD`, `ONUPDATE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Add,
    Update,
}

impl Operation {
    fn bit(self) -> i32 {
        match self {
            Self::Add => 1,
            Self::Update => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub criteria: String,
    pub condition: i32,
    pub pattern: String,
}

/// An active `RuleAsset` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRule {
    /// `AND` matching: every criterion must match (else `OR`).
    pub match_all: bool,
    /// Bitmask of the operations the rule runs on.
    pub condition: i32,
    pub criteria: Vec<Criterion>,
}

impl AssetRule {
    /// Whether the rule could match a manual write (`_auto = 0`) of the
    /// item type. Only the `_auto` and `_itemtype` criteria are understood:
    /// any other criterion is assumed to possibly match.
    pub fn may_apply_to_manual_write(&self, itemtype: &str, operation: Operation) -> bool {
        if self.condition & operation.bit() == 0 {
            return false;
        }
        if self.criteria.is_empty() {
            return true;
        }
        let excludes = |criterion: &Criterion| {
            let pattern = criterion.pattern.as_str();
            match (criterion.criteria.as_str(), criterion.condition) {
                ("_auto", PATTERN_IS) => pattern != "0",
                ("_auto", PATTERN_IS_NOT) => pattern == "0",
                ("_itemtype", PATTERN_IS) => pattern != itemtype,
                ("_itemtype", PATTERN_IS_NOT) => pattern == itemtype,
                _ => false,
            }
        };
        if self.match_all {
            !self.criteria.iter().any(excludes)
        } else {
            !self.criteria.iter().all(excludes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn criterion(criteria: &str, condition: i32, pattern: &str) -> Criterion {
        Criterion {
            criteria: criteria.to_owned(),
            condition,
            pattern: pattern.to_owned(),
        }
    }

    /// One of the three rules of a fresh install ("One user assignation").
    fn default_rule() -> AssetRule {
        AssetRule {
            match_all: true,
            condition: 3,
            criteria: vec![
                criterion("_itemtype", PATTERN_IS, "Computer"),
                criterion("_auto", PATTERN_IS, "1"),
                criterion("contact", 6, "/(.*)/"),
            ],
        }
    }

    #[test]
    fn default_rules_only_concern_automatic_inventory() {
        let rule = default_rule();
        assert!(!rule.may_apply_to_manual_write("Computer", Operation::Add));
        assert!(!rule.may_apply_to_manual_write("Computer", Operation::Update));
    }

    #[test]
    fn manual_rules_are_detected() {
        let mut rule = default_rule();
        rule.criteria.retain(|c| c.criteria != "_auto");
        assert!(rule.may_apply_to_manual_write("Computer", Operation::Update));
        // Another item type.
        assert!(!rule.may_apply_to_manual_write("Monitor", Operation::Update));
        // Only on add.
        rule.condition = 1;
        assert!(!rule.may_apply_to_manual_write("Computer", Operation::Update));
        assert!(rule.may_apply_to_manual_write("Computer", Operation::Add));
    }

    #[test]
    fn or_rules_need_every_criterion_excluded() {
        let rule = AssetRule {
            match_all: false,
            condition: 3,
            criteria: vec![
                criterion("_auto", PATTERN_IS, "1"),
                criterion("name", PATTERN_IS, "x"),
            ],
        };
        assert!(rule.may_apply_to_manual_write("Computer", Operation::Add));
        let rule = AssetRule {
            criteria: vec![
                criterion("_auto", PATTERN_IS, "1"),
                criterion("_itemtype", PATTERN_IS_NOT, "Computer"),
            ],
            ..rule
        };
        assert!(!rule.may_apply_to_manual_write("Computer", Operation::Add));
    }
}
