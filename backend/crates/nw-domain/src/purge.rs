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
        if self == Self::Software {
            return software_purged_links();
        }
        if self == Self::SoftwareLicense {
            return license_purged_links();
        }
        if !self.is_inventory_asset() {
            return self.datacenter_purged_links();
        }
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

    /// Rows deleted with a datacenter item, by the branches of
    /// `cleanRelationTable` its type is in.
    fn datacenter_purged_links(self) -> Vec<Link> {
        let mut links = Vec::new();
        if self.has_infocom() {
            links.push(INFOCOM);
        }
        // `contract_types`.
        if self != Self::PassiveDcEquipment {
            links.push(link("glpi_contracts_items", Owner::Item));
        }
        // `document_types`.
        if self == Self::Cluster {
            links.push(link("glpi_documents_items", Owner::Item));
        }
        // `ticket_types`: changes and problems.
        links.extend([
            link("glpi_changes_items", Owner::Item),
            link("glpi_items_problems", Owner::Item),
        ]);
        links
    }

    /// Linked rows whose purge does more than deleting them: network ports
    /// (names, addresses, links), components and installed software
    /// (history, licenses), tickets (updated, deleted when the item was
    /// their only one), racks, cartridges, reservations, impact analysis,
    /// the items of a rack, an enclosure or a cluster, the plugs of a PDU.
    pub fn purge_blockers(self) -> Vec<Blocker> {
        if self == Self::Software {
            // Its versions are purged with their installations (history
            // of the installed items), its licenses put in the trash.
            return vec![
                Blocker::Column("glpi_softwareversions", "softwares_id"),
                Blocker::Column("glpi_softwarelicenses", "softwares_id"),
                Blocker::Item("glpi_items_tickets"),
                Blocker::Reservations,
                Blocker::Impact,
            ];
        }
        if self == Self::SoftwareLicense {
            // Its children are moved under its parent with their history
            // (`pre_deleteItem` of the trees); its affectations and
            // certificates are deleted with the history of their items.
            return vec![
                Blocker::Column("glpi_softwarelicenses", "softwarelicenses_id"),
                Blocker::Column("glpi_items_softwarelicenses", "softwarelicenses_id"),
                Blocker::Item("glpi_certificates_items"),
                Blocker::Item("glpi_items_tickets"),
            ];
        }
        if !self.is_inventory_asset() {
            return self.datacenter_purge_blockers();
        }
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

    /// Columns of other items pointing to the asset, set to 0 by a form
    /// update (`cleanRelationData`: a new `date_mod`, no history): the
    /// software that is an upgrade of a software.
    pub fn cleared_references(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Software => &[("glpi_softwares", "softwares_id")],
            _ => &[],
        }
    }

    fn datacenter_purge_blockers(self) -> Vec<Blocker> {
        let mut blockers = vec![Blocker::Item("glpi_items_tickets")];
        // `networkport_types`.
        if matches!(self, Self::Enclosure | Self::Pdu | Self::Cluster) {
            blockers.push(Blocker::Item("glpi_networkports"));
        }
        // `impact_asset_types`.
        if self != Self::PassiveDcEquipment {
            blockers.push(Blocker::Impact);
        }
        // `itemdevices_types`.
        if self == Self::Enclosure {
            blockers.extend(DEVICE_ITEMS.iter().map(|(_, table)| Blocker::Item(table)));
        }
        // `rackable_types`: its place in a rack or an enclosure.
        if matches!(self, Self::Enclosure | Self::Pdu | Self::PassiveDcEquipment) {
            blockers.extend([
                Blocker::Item("glpi_items_racks"),
                Blocker::Item("glpi_items_enclosures"),
            ]);
        }
        // `cleanDBonPurge` of the class: what it holds.
        blockers.extend(match self {
            Self::Rack => vec![
                Blocker::Column("glpi_items_racks", "racks_id"),
                Blocker::Column("glpi_pdus_racks", "racks_id"),
            ],
            Self::Enclosure => vec![Blocker::Column("glpi_items_enclosures", "enclosures_id")],
            Self::Pdu => vec![
                Blocker::Column("glpi_pdus_plugs", "pdus_id"),
                Blocker::Column("glpi_pdus_racks", "pdus_id"),
            ],
            Self::Cluster => vec![Blocker::Column("glpi_items_clusters", "clusters_id")],
            _ => Vec::new(),
        });
        blockers
    }
}

/// Rows deleted with a software: its projects (`cleanDBonPurge`), then
/// the branches of `cleanRelationTable` its type is in.
fn software_purged_links() -> Vec<Link> {
    vec![
        link("glpi_items_projects", Owner::Item),
        INFOCOM,
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
        link("glpi_domains_items", Owner::Item),
    ]
}

/// Rows deleted with a license: its alerts (`cleanDBonPurge`), then the
/// branches of `cleanRelationTable` its type is in.
fn license_purged_links() -> Vec<Link> {
    vec![
        link("glpi_alerts", Owner::Item),
        INFOCOM,
        link("glpi_contracts_items", Owner::Item),
        link("glpi_documents_items", Owner::Item),
        link("glpi_notepads", Owner::Item),
        Link {
            history: Some("KnowbaseItem_Item"),
            ..link("glpi_knowbaseitems_items", Owner::Item)
        },
        link("glpi_changes_items", Owner::Item),
        link("glpi_items_problems", Owner::Item),
    ]
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
        for kind in AssetKind::ALL
            .into_iter()
            .filter(|kind| kind.is_inventory_asset())
        {
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
        // Datacenter items.
        assert!(!tables(AssetKind::Cluster).contains(&"glpi_infocoms"));
        assert!(tables(AssetKind::Cluster).contains(&"glpi_documents_items"));
        assert!(!tables(AssetKind::Rack).contains(&"glpi_documents_items"));
        assert!(!tables(AssetKind::PassiveDcEquipment).contains(&"glpi_contracts_items"));
        assert!(
            AssetKind::Rack
                .purge_blockers()
                .contains(&Blocker::Column("glpi_items_racks", "racks_id"))
        );
        assert!(
            !AssetKind::PassiveDcEquipment
                .purge_blockers()
                .contains(&Blocker::Impact)
        );
        // Software.
        assert!(tables(AssetKind::Software).contains(&"glpi_notepads"));
        assert!(!tables(AssetKind::Software).contains(&"glpi_computers_items"));
        assert!(
            AssetKind::Software
                .purge_blockers()
                .contains(&Blocker::Column("glpi_softwareversions", "softwares_id"))
        );
        assert_eq!(
            AssetKind::Software.cleared_references(),
            [("glpi_softwares", "softwares_id")]
        );
    }
}
