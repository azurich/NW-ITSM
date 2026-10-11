//! Items created from a template: the legacy code clones the template
//! (`CommonDBTM::clone`), then its linked rows (`Clonable::post_clone`, in
//! the order of `getCloneRelations`).
//!
//! The infocom, the contracts and the documents of the inventory assets and
//! of the software are cloned here the same way. The other links of the template (operating
//! system, components, disks, software, network ports, connections, notes,
//! knowledge base, cartridges, items of an enclosure, plugs of a PDU) leave
//! the creation to the classic interface: see
//! [`AssetKind::template_blockers`]. Racks and passive equipment clone no
//! link (their classes are not `Clonable`).

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

impl AssetKind {
    /// Rows of the template cloned for the new item, in this order.
    pub fn cloned_links(self) -> &'static [ClonedLink] {
        if self.is_inventory_asset() || self == Self::Software {
            &[
                ClonedLink::Infocom,
                ClonedLink::Contract,
                ClonedLink::Document,
            ]
        } else {
            &[]
        }
    }

    /// Rows of a template whose cloning is not ported.
    pub fn template_blockers(self) -> Vec<Blocker> {
        let devices = DEVICE_ITEMS.iter().map(|(_, table)| Blocker::Item(table));
        match self {
            // Not `Clonable`, or no relation to clone (licenses): nothing is
            // cloned. No templates for clusters.
            Self::Rack | Self::PassiveDcEquipment | Self::Cluster | Self::SoftwareLicense => {
                return Vec::new();
            }
            Self::Enclosure => {
                let mut blockers = vec![Blocker::Column("glpi_items_enclosures", "enclosures_id")];
                blockers.extend(devices);
                blockers.push(Blocker::Item("glpi_networkports"));
                return blockers;
            }
            Self::Pdu => {
                let mut blockers = vec![Blocker::Column("glpi_pdus_plugs", "pdus_id")];
                blockers.extend(devices);
                blockers.push(Blocker::Item("glpi_networkports"));
                return blockers;
            }
            // Its versions and licenses are not cloned.
            Self::Software => return vec![Blocker::Item("glpi_knowbaseitems_items")],
            _ => {}
        }
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
            Self::Rack
            | Self::Enclosure
            | Self::Pdu
            | Self::PassiveDcEquipment
            | Self::Cluster
            | Self::Software
            | Self::SoftwareLicense => {
                unreachable!("handled above")
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
        assert!(AssetKind::Rack.template_blockers().is_empty());
        assert!(AssetKind::Rack.cloned_links().is_empty());
        assert!(
            AssetKind::Pdu
                .template_blockers()
                .contains(&Blocker::Column("glpi_pdus_plugs", "pdus_id"))
        );
        assert_eq!(AssetKind::Monitor.cloned_links().len(), 3);
        assert_eq!(AssetKind::Software.cloned_links().len(), 3);
        assert_eq!(
            AssetKind::Software.template_blockers(),
            [Blocker::Item("glpi_knowbaseitems_items")]
        );
    }
}
