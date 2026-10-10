//! What purging an asset does to the rows linked to it, as
//! `CommonDBTM::deleteFromDB` with `$force` (`cleanDBonPurge` of the class,
//! `cleanHistory`, `cleanRelationData`, `cleanRelationTable`).
//!
//! The rows the PHP code only deletes, without other effect, are deleted
//! here the same way. Where it does more (updates of other items, history
//! on them, cascades), the presence of such rows leaves the purge to the
//! classic interface: see [`AssetKind::purge_blockers`].

use crate::asset::{AssetKind, DEVICE_ITEMS};

/// How rows point to the asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// `itemtype` and `items_id` columns.
    Item,
    /// `computers_id`, for the rows of a computer.
    Computer,
    /// A connection: `computers_id` for a computer, `itemtype` and
    /// `items_id` for the connected item.
    Connection,
}

/// Rows deleted with the asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub table: &'static str,
    pub owner: Owner,
    /// Class of the rows when it keeps a history: theirs is deleted too.
    pub history: Option<&'static str>,
    /// Class of the rows when they have alerts (`glpi_alerts`), deleted too.
    pub alerts: Option<&'static str>,
}

const fn link(table: &'static str, owner: Owner) -> Link {
    Link {
        table,
        owner,
        history: None,
        alerts: None,
    }
}

/// Linked rows whose purge is not ported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    /// Rows of the asset in a table (`itemtype` and `items_id`).
    Item(&'static str),
    /// Rows of a table pointing to the asset by a column.
    Column(&'static str, &'static str),
    /// Reservations of the asset (`glpi_reservations` of its reservation
    /// item): deleted with notifications.
    Reservations,
    /// Relations of the impact analysis (source or impacted).
    Impact,
}

/// Infocom of the asset (`cleanRelationTable`), with its history and
/// alerts.
pub const INFOCOM: Link = Link {
    table: "glpi_infocoms",
    owner: Owner::Item,
    history: Some("Infocom"),
    alerts: Some("Infocom"),
};

impl AssetKind {
    /// Rows deleted with the asset. Its own history (`glpi_logs`) is
    /// deleted too.
    pub fn purged_links(self) -> Vec<Link> {
        let mut links = vec![INFOCOM];
        // `cleanDBonPurge` of the class.
        if self == Self::Computer {
            links.extend([
                Link {
                    history: Some("ComputerAntivirus"),
                    ..link("glpi_computerantiviruses", Owner::Computer)
                },
                Link {
                    history: Some("ComputerVirtualMachine"),
                    ..link("glpi_computervirtualmachines", Owner::Computer)
                },
                Link {
                    history: Some("Item_Disk"),
                    ..link("glpi_items_disks", Owner::Item)
                },
            ]);
        }
        if self != Self::NetworkEquipment {
            links.push(link("glpi_computers_items", Owner::Connection));
        }
        if matches!(
            self,
            Self::Computer | Self::Printer | Self::NetworkEquipment | Self::Peripheral
        ) {
            links.push(link("glpi_certificates_items", Owner::Item));
        }
        links.push(link("glpi_items_projects", Owner::Item));
        // `cleanRelationTable`.
        links.extend([
            Link {
                alerts: Some("ReservationItem"),
                ..link("glpi_reservationitems", Owner::Item)
            },
            link("glpi_contracts_items", Owner::Item),
            link("glpi_documents_items", Owner::Item),
            link("glpi_notepads", Owner::Item),
            Link {
                history: Some("KnowbaseItem_Item"),
                ..link("glpi_knowbaseitems_items", Owner::Item)
            },
            link("glpi_changes_items", Owner::Item),
            link("glpi_items_problems", Owner::Item),
        ]);
        if matches!(self, Self::Computer | Self::NetworkEquipment) {
            links.push(link("glpi_items_clusters", Owner::Item));
        }
        links.extend([
            link("glpi_items_operatingsystems", Owner::Item),
            link("glpi_domains_items", Owner::Item),
        ]);
        links
    }

    /// Linked rows whose purge does more than deleting them: network ports
    /// (names, addresses, links), components and installed software
    /// (history, licenses), tickets (updated, deleted when the item was
    /// their only one), racks, cartridges, reservations, impact analysis.
    pub fn purge_blockers(self) -> Vec<Blocker> {
        let mut blockers = vec![
            Blocker::Item("glpi_networkports"),
            Blocker::Item("glpi_items_tickets"),
            Blocker::Item("glpi_items_softwareversions"),
            Blocker::Reservations,
            Blocker::Impact,
        ];
        blockers.extend(DEVICE_ITEMS.iter().map(|(_, table)| Blocker::Item(table)));
        if matches!(
            self,
            Self::Computer | Self::Monitor | Self::NetworkEquipment | Self::Peripheral
        ) {
            blockers.extend([
                Blocker::Item("glpi_items_racks"),
                Blocker::Item("glpi_items_enclosures"),
            ]);
        }
        match self {
            Self::Computer => blockers.push(Blocker::Item("glpi_items_softwarelicenses")),
            Self::Printer => blockers.push(Blocker::Column("glpi_cartridges", "printers_id")),
            _ => {}
        }
        blockers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_follow_the_classes() {
        let tables = |kind: AssetKind| -> Vec<&str> {
            kind.purged_links().iter().map(|link| link.table).collect()
        };
        assert!(tables(AssetKind::Computer).contains(&"glpi_items_disks"));
        assert!(!tables(AssetKind::Printer).contains(&"glpi_items_disks"));
        assert!(!tables(AssetKind::NetworkEquipment).contains(&"glpi_computers_items"));
        assert!(!tables(AssetKind::Monitor).contains(&"glpi_certificates_items"));
        assert!(tables(AssetKind::NetworkEquipment).contains(&"glpi_items_clusters"));
        for kind in AssetKind::ALL {
            assert_eq!(kind.purged_links()[0], INFOCOM);
            assert!(
                kind.purge_blockers()
                    .contains(&Blocker::Item("glpi_items_devicesimcards"))
            );
        }
        assert!(
            AssetKind::Printer
                .purge_blockers()
                .contains(&Blocker::Column("glpi_cartridges", "printers_id"))
        );
        assert!(
            !AssetKind::Printer
                .purge_blockers()
                .contains(&Blocker::Item("glpi_items_racks"))
        );
    }
}
