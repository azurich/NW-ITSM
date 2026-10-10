//! Items created from a template: the legacy code clones the template
//! (`CommonDBTM::clone`), then its linked rows (`Clonable::post_clone`, in
//! the order of `getCloneRelations`).
//!
//! The infocom, the contracts and the documents are cloned here the same
//! way. The other links of the template (operating system, components,
//! disks, software, network ports, connections, notes, knowledge base,
//! cartridges) leave the creation to the classic interface:
//! see [`AssetKind::template_blockers`].

use crate::{
    asset::{AssetKind, DEVICE_ITEMS},
    purge::Blocker,
};

/// Rows of the template cloned for the new item, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClonedLink {
    /// `Infocom`: its values, with the default alert of the entity.
    Infocom,
    /// `Contract_Item`, unless the contract has all its items.
    Contract,
    /// `Document_Item`.
    Document,
}

pub const CLONED_LINKS: [ClonedLink; 3] = [
    ClonedLink::Infocom,
    ClonedLink::Contract,
    ClonedLink::Document,
];

impl AssetKind {
    /// Rows of a template whose cloning is not ported.
    pub fn template_blockers(self) -> Vec<Blocker> {
        let mut blockers = vec![
            Blocker::Item("glpi_items_operatingsystems"),
            Blocker::Item("glpi_knowbaseitems_items"),
        ];
        blockers.extend(DEVICE_ITEMS.iter().map(|(_, table)| Blocker::Item(table)));
        if self != Self::Monitor {
            blockers.push(Blocker::Item("glpi_networkports"));
        }
        match self {
            Self::Computer => blockers.extend([
                Blocker::Item("glpi_items_disks"),
                Blocker::Item("glpi_items_softwareversions"),
                Blocker::Item("glpi_items_softwarelicenses"),
                Blocker::Item("glpi_notepads"),
                Blocker::Column("glpi_computers_items", "computers_id"),
            ]),
            Self::NetworkEquipment => {}
            Self::Printer => blockers.extend([
                Blocker::Item("glpi_computers_items"),
                Blocker::Column("glpi_cartridges", "printers_id"),
            ]),
            Self::Monitor | Self::Peripheral | Self::Phone => {
                blockers.push(Blocker::Item("glpi_computers_items"));
            }
        }
        blockers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockers_follow_the_clone_relations() {
        let computer = AssetKind::Computer.template_blockers();
        assert!(computer.contains(&Blocker::Item("glpi_notepads")));
        assert!(computer.contains(&Blocker::Column("glpi_computers_items", "computers_id")));
        assert!(
            !AssetKind::Monitor
                .template_blockers()
                .contains(&Blocker::Item("glpi_networkports"))
        );
        assert!(
            !AssetKind::NetworkEquipment
                .template_blockers()
                .contains(&Blocker::Item("glpi_computers_items"))
        );
    }
}
