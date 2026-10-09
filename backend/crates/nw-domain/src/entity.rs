//! Entity tree and active entity scope.
//!
//! Reproduces the legacy session logic (`Session::changeProfile`,
//! `Session::changeActiveEntities`) and the item restriction built by
//! `DbUtils::getEntitiesRestrictCriteria`.

use std::collections::{BTreeSet, HashMap};

/// In-memory copy of `glpi_entities` (`id` -> `entities_id`).
///
/// The `sons_cache` / `ancestors_cache` columns are not trusted: they are
/// often empty, so the tree is rebuilt from the parent links.
#[derive(Debug, Default, Clone)]
pub struct EntityTree {
    parents: HashMap<i32, Option<i32>>,
    children: HashMap<i32, Vec<i32>>,
}

impl EntityTree {
    /// Builds the tree from `(id, entities_id)` rows.
    ///
    /// The root entity points to `-1` (or to itself in old databases);
    /// both mean "no parent".
    pub fn new(rows: impl IntoIterator<Item = (i32, i32)>) -> Self {
        let mut tree = Self::default();
        for (id, parent) in rows {
            let parent = (parent >= 0 && parent != id).then_some(parent);
            tree.parents.insert(id, parent);
            if let Some(parent) = parent {
                tree.children.entry(parent).or_default().push(id);
            }
        }
        tree
    }

    /// Number of entities in the tree.
    pub fn len(&self) -> usize {
        self.parents.len()
    }

    /// Whether the tree has no entity at all.
    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }

    /// The entity itself and all its descendants (legacy `getSonsOf`).
    pub fn sons_of(&self, id: i32) -> BTreeSet<i32> {
        let mut sons = BTreeSet::from([id]);
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            for &child in self.children.get(&current).into_iter().flatten() {
                if sons.insert(child) {
                    stack.push(child);
                }
            }
        }
        sons
    }

    /// All ancestors of the entity, itself excluded (legacy `getAncestorsOf`).
    pub fn ancestors_of(&self, id: i32) -> BTreeSet<i32> {
        let mut ancestors = BTreeSet::new();
        let mut current = id;
        while let Some(Some(parent)) = self.parents.get(&current) {
            if *parent == id || !ancestors.insert(*parent) {
                break; // corrupted data: cycle in the tree
            }
            current = *parent;
        }
        ancestors
    }
}

/// An entity a profile is granted on (`glpi_profiles_users` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileEntity {
    pub entity_id: i32,
    pub is_recursive: bool,
}

/// Entities the current session can see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityScope {
    active: BTreeSet<i32>,
    parents: BTreeSet<i32>,
    all: bool,
}

impl EntityScope {
    /// Computes the scope selected at login for a profile.
    ///
    /// Same order as the legacy `Session::changeProfile`:
    /// 1. the user's default entity, if the profile is granted on it;
    /// 2. the default entity and its sons, if it is below a recursive grant;
    /// 3. otherwise every entity the profile is granted on.
    ///
    /// Returns `None` when the profile has no entity, in which case the
    /// legacy code does not activate the profile.
    pub fn for_profile(
        tree: &EntityTree,
        granted: &[ProfileEntity],
        default_entity: i32,
    ) -> Option<Self> {
        let granted = merge_grants(granted);
        if granted.is_empty() {
            return None;
        }

        let sons_if = |grant: &ProfileEntity| {
            if grant.is_recursive {
                tree.sons_of(grant.entity_id)
            } else {
                BTreeSet::from([grant.entity_id])
            }
        };

        if let Some(grant) = granted.iter().find(|g| g.entity_id == default_entity) {
            return Some(Self::from_active(tree, sons_if(grant)));
        }

        let default_ancestors = tree.ancestors_of(default_entity);
        let below_recursive_grant = granted.iter().any(|g| {
            g.is_recursive
                && (g.entity_id == default_entity || default_ancestors.contains(&g.entity_id))
        });
        if below_recursive_grant {
            return Some(Self::from_active(tree, tree.sons_of(default_entity)));
        }

        let active = granted.iter().flat_map(sons_if).collect();
        Some(Self::from_active(tree, active))
    }

    fn from_active(tree: &EntityTree, active: BTreeSet<i32>) -> Self {
        let parents = active
            .iter()
            .flat_map(|&id| tree.ancestors_of(id))
            .filter(|id| !active.contains(id))
            .collect();
        // Legacy `glpishowallentities`: no restriction at all when every
        // entity is active.
        let all = tree.len() <= active.len();
        Self {
            active,
            parents,
            all,
        }
    }

    /// Entities whose items are visible.
    pub fn active(&self) -> &BTreeSet<i32> {
        &self.active
    }

    /// Ancestors of the active entities: their items are visible only
    /// when they are shared with sub-entities (`is_recursive = 1`).
    pub fn parents(&self) -> &BTreeSet<i32> {
        &self.parents
    }

    /// Whether every entity is visible, so queries need no restriction.
    pub fn is_unrestricted(&self) -> bool {
        self.all
    }

    /// Whether an item stored in `entity_id` is visible.
    pub fn can_see(&self, entity_id: i32, is_recursive: bool) -> bool {
        self.all
            || self.active.contains(&entity_id)
            || (is_recursive && self.parents.contains(&entity_id))
    }
}

/// The same entity can be granted twice: the recursive grant wins
/// (legacy `Session::initEntityProfiles`).
fn merge_grants(granted: &[ProfileEntity]) -> Vec<ProfileEntity> {
    let mut merged: Vec<ProfileEntity> = Vec::with_capacity(granted.len());
    for grant in granted {
        match merged.iter_mut().find(|g| g.entity_id == grant.entity_id) {
            Some(existing) => existing.is_recursive |= grant.is_recursive,
            None => merged.push(*grant),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    //   0 (root)
    //   ├── 1
    //   │   ├── 3
    //   │   └── 4
    //   │       └── 5
    //   └── 2
    fn tree() -> EntityTree {
        EntityTree::new([(0, -1), (1, 0), (2, 0), (3, 1), (4, 1), (5, 4)])
    }

    fn set(ids: &[i32]) -> BTreeSet<i32> {
        ids.iter().copied().collect()
    }

    fn grant(entity_id: i32, is_recursive: bool) -> ProfileEntity {
        ProfileEntity {
            entity_id,
            is_recursive,
        }
    }

    #[test]
    fn sons_include_the_entity_itself() {
        assert_eq!(tree().sons_of(1), set(&[1, 3, 4, 5]));
        assert_eq!(tree().sons_of(5), set(&[5]));
        assert_eq!(tree().sons_of(0), set(&[0, 1, 2, 3, 4, 5]));
    }

    #[test]
    fn ancestors_exclude_the_entity_itself() {
        assert_eq!(tree().ancestors_of(5), set(&[0, 1, 4]));
        assert_eq!(tree().ancestors_of(0), set(&[]));
    }

    #[test]
    fn root_pointing_to_itself_is_a_root() {
        let tree = EntityTree::new([(0, 0), (1, 0)]);
        assert_eq!(tree.ancestors_of(1), set(&[0]));
        assert_eq!(tree.ancestors_of(0), set(&[]));
    }

    #[test]
    fn cycles_do_not_loop_forever() {
        let tree = EntityTree::new([(1, 2), (2, 1)]);
        assert_eq!(tree.ancestors_of(1), set(&[2]));
        assert_eq!(tree.sons_of(1), set(&[1, 2]));
    }

    #[test]
    fn recursive_grant_on_root_sees_everything() {
        let scope = EntityScope::for_profile(&tree(), &[grant(0, true)], 0).unwrap();
        assert!(scope.is_unrestricted());
        assert!(scope.can_see(5, false));
    }

    #[test]
    fn default_entity_granted_directly() {
        let scope =
            EntityScope::for_profile(&tree(), &[grant(1, true), grant(2, false)], 1).unwrap();
        assert_eq!(scope.active(), &set(&[1, 3, 4, 5]));
        assert_eq!(scope.parents(), &set(&[0]));
        assert!(!scope.is_unrestricted());
        assert!(scope.can_see(4, false));
        assert!(!scope.can_see(2, false));
        // Items of the root entity are visible only when shared.
        assert!(!scope.can_see(0, false));
        assert!(scope.can_see(0, true));
    }

    #[test]
    fn default_entity_below_a_recursive_grant() {
        let scope = EntityScope::for_profile(&tree(), &[grant(1, true)], 4).unwrap();
        assert_eq!(scope.active(), &set(&[4, 5]));
        assert_eq!(scope.parents(), &set(&[0, 1]));
    }

    #[test]
    fn default_entity_below_a_non_recursive_grant_falls_back_to_all() {
        let scope =
            EntityScope::for_profile(&tree(), &[grant(1, false), grant(2, false)], 4).unwrap();
        assert_eq!(scope.active(), &set(&[1, 2]));
        assert_eq!(scope.parents(), &set(&[0]));
    }

    #[test]
    fn duplicated_grant_keeps_recursion() {
        let scope =
            EntityScope::for_profile(&tree(), &[grant(1, false), grant(1, true)], 1).unwrap();
        assert_eq!(scope.active(), &set(&[1, 3, 4, 5]));
    }

    #[test]
    fn profile_without_entity_is_not_usable() {
        assert_eq!(EntityScope::for_profile(&tree(), &[], 0), None);
    }
}
