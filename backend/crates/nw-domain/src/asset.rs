//! Assets of the inventory: computers, monitors, printers, network
//! equipment, peripherals and phones. They share most of their columns, so
//! one description drives the reads, the writes and the history of all.

/// A type of asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Computer,
    Monitor,
    Printer,
    NetworkEquipment,
    Peripheral,
    Phone,
}

/// How a field is stored, which decides how it is validated and shown in
/// the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Plain text, stored with `<` and `>` encoded.
    Text,
    /// Id of a row of a dropdown table (`0` for none).
    Dropdown {
        table: &'static str,
        tree: bool,
    },
    /// Id of a user (`0` for none).
    User,
    Bool,
}

/// An editable column and the search option identifying it in the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub column: &'static str,
    pub search_option: i32,
    pub kind: FieldKind,
}

const fn text(column: &'static str, search_option: i32) -> Field {
    Field {
        column,
        search_option,
        kind: FieldKind::Text,
    }
}

const fn dropdown(
    column: &'static str,
    search_option: i32,
    table: &'static str,
    tree: bool,
) -> Field {
    Field {
        column,
        search_option,
        kind: FieldKind::Dropdown { table, tree },
    }
}

const fn user(column: &'static str, search_option: i32) -> Field {
    Field {
        column,
        search_option,
        kind: FieldKind::User,
    }
}

/// Fields of every asset type, with the search options of the PHP code
/// (identical for the six types).
const COMMON_FIELDS: [Field; 14] = [
    text("name", 1),
    text("serial", 5),
    text("otherserial", 6),
    text("contact", 7),
    text("contact_num", 8),
    text("comment", 16),
    dropdown("locations_id", 3, "glpi_locations", true),
    dropdown("states_id", 31, "glpi_states", true),
    dropdown("manufacturers_id", 23, "glpi_manufacturers", false),
    user("users_id", 70),
    dropdown("groups_id", 71, "glpi_groups", true),
    user("users_id_tech", 24),
    dropdown("groups_id_tech", 49, "glpi_groups", true),
    Field {
        column: "is_recursive",
        search_option: 86,
        kind: FieldKind::Bool,
    },
];

const NETWORK: Field = dropdown("networks_id", 32, "glpi_networks", false);

impl AssetKind {
    pub const ALL: [Self; 6] = [
        Self::Computer,
        Self::Monitor,
        Self::Printer,
        Self::NetworkEquipment,
        Self::Peripheral,
        Self::Phone,
    ];

    /// Name in the API paths (`/assets/computers`...).
    pub fn slug(self) -> &'static str {
        match self {
            Self::Computer => "computers",
            Self::Monitor => "monitors",
            Self::Printer => "printers",
            Self::NetworkEquipment => "network-equipments",
            Self::Peripheral => "peripherals",
            Self::Phone => "phones",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.slug() == slug)
    }

    /// PHP class, stored in the `itemtype` columns.
    pub fn itemtype(self) -> &'static str {
        match self {
            Self::Computer => "Computer",
            Self::Monitor => "Monitor",
            Self::Printer => "Printer",
            Self::NetworkEquipment => "NetworkEquipment",
            Self::Peripheral => "Peripheral",
            Self::Phone => "Phone",
        }
    }

    pub fn from_itemtype(itemtype: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.itemtype() == itemtype)
    }

    pub fn table(self) -> &'static str {
        match self {
            Self::Computer => "glpi_computers",
            Self::Monitor => "glpi_monitors",
            Self::Printer => "glpi_printers",
            Self::NetworkEquipment => "glpi_networkequipments",
            Self::Peripheral => "glpi_peripherals",
            Self::Phone => "glpi_phones",
        }
    }

    /// Module of `glpi_profilerights`.
    pub fn right(self) -> &'static str {
        match self {
            Self::Computer => "computer",
            Self::Monitor => "monitor",
            Self::Printer => "printer",
            Self::NetworkEquipment => "networking",
            Self::Peripheral => "peripheral",
            Self::Phone => "phone",
        }
    }

    /// `type` of the `glpi_events` written by the legacy forms.
    pub fn event_type(self) -> &'static str {
        match self {
            Self::Computer => "computers",
            Self::Monitor => "monitors",
            Self::Printer => "printers",
            Self::NetworkEquipment => "networkequipment",
            Self::Peripheral => "peripherals",
            Self::Phone => "phones",
        }
    }

    fn lowercase(self) -> &'static str {
        match self {
            Self::Computer => "computer",
            Self::Monitor => "monitor",
            Self::Printer => "printer",
            Self::NetworkEquipment => "networkequipment",
            Self::Peripheral => "peripheral",
            Self::Phone => "phone",
        }
    }

    /// Type dropdown: `(column, table)`.
    pub fn type_dropdown(self) -> (&'static str, &'static str) {
        match self {
            Self::Computer => ("computertypes_id", "glpi_computertypes"),
            Self::Monitor => ("monitortypes_id", "glpi_monitortypes"),
            Self::Printer => ("printertypes_id", "glpi_printertypes"),
            Self::NetworkEquipment => ("networkequipmenttypes_id", "glpi_networkequipmenttypes"),
            Self::Peripheral => ("peripheraltypes_id", "glpi_peripheraltypes"),
            Self::Phone => ("phonetypes_id", "glpi_phonetypes"),
        }
    }

    /// Model dropdown: `(column, table)`.
    pub fn model_dropdown(self) -> (&'static str, &'static str) {
        match self {
            Self::Computer => ("computermodels_id", "glpi_computermodels"),
            Self::Monitor => ("monitormodels_id", "glpi_monitormodels"),
            Self::Printer => ("printermodels_id", "glpi_printermodels"),
            Self::NetworkEquipment => ("networkequipmentmodels_id", "glpi_networkequipmentmodels"),
            Self::Peripheral => ("peripheralmodels_id", "glpi_peripheralmodels"),
            Self::Phone => ("phonemodels_id", "glpi_phonemodels"),
        }
    }

    /// Column of `glpi_states` telling whether a status is offered for this type.
    pub fn state_visibility_column(self) -> String {
        format!("is_visible_{}", self.lowercase())
    }

    /// Can be used by several computers at once (`is_global`): such items
    /// do not follow the changes of the computers they are connected to.
    pub fn can_be_global(self) -> bool {
        !matches!(self, Self::Computer | Self::NetworkEquipment)
    }

    /// Editable fields, in a stable order.
    pub fn fields(self) -> Vec<Field> {
        let (type_column, type_table) = self.type_dropdown();
        let (model_column, model_table) = self.model_dropdown();
        let mut fields = COMMON_FIELDS.to_vec();
        fields.push(dropdown(type_column, 4, type_table, false));
        fields.push(dropdown(model_column, 40, model_table, false));
        match self {
            Self::Computer => fields.extend([
                text("uuid", 47),
                NETWORK,
                dropdown("autoupdatesystems_id", 42, "glpi_autoupdatesystems", false),
            ]),
            Self::Printer | Self::NetworkEquipment => fields.push(NETWORK),
            Self::Peripheral => fields.push(text("brand", 11)),
            Self::Phone => fields.extend([text("number_line", 9), text("brand", 11)]),
            Self::Monitor => {}
        }
        fields
    }

    /// Tables of the items following the entity and sharing of the asset
    /// (`$forward_entity_to`), infocoms excepted.
    pub fn forward_entity_tables(self) -> &'static [&'static str] {
        match self {
            Self::Computer => &[
                "glpi_items_disks",
                "glpi_computervirtualmachines",
                "glpi_items_softwareversions",
                "glpi_networkports",
                "glpi_reservationitems",
                "glpi_items_operatingsystems",
            ],
            Self::Monitor | Self::Peripheral => &[
                "glpi_reservationitems",
                "glpi_items_operatingsystems",
                "glpi_networkports",
                "glpi_items_softwareversions",
            ],
            Self::NetworkEquipment | Self::Printer => &[
                "glpi_networkports",
                "glpi_reservationitems",
                "glpi_items_operatingsystems",
                "glpi_items_disks",
                "glpi_items_softwareversions",
            ],
            Self::Phone => &[
                "glpi_networkports",
                "glpi_reservationitems",
                "glpi_items_operatingsystems",
                "glpi_items_disks",
            ],
        }
    }

    pub fn field(self, column: &str) -> Option<Field> {
        self.fields()
            .into_iter()
            .find(|field| field.column == column)
    }
}

/// Value of a field, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Stored text (`<` and `>` encoded), `None` for NULL.
    Text(Option<String>),
    /// Id of a dropdown row or user, `0` for none.
    Id(i32),
    Bool(bool),
}

impl Value {
    /// Whether a new value changes the stored one, as compared by
    /// `CommonDBTM::update` (NULL and an empty text are the same).
    pub fn differs_from(&self, stored: &Value) -> bool {
        match (self, stored) {
            (Self::Text(new), Self::Text(old)) => {
                new.as_deref().unwrap_or_default() != old.as_deref().unwrap_or_default()
            }
            (new, old) => new != old,
        }
    }

    /// Raw value for the history of text and boolean fields.
    pub fn raw(&self) -> String {
        match self {
            Self::Text(text) => text.clone().unwrap_or_default(),
            Self::Id(id) => id.to_string(),
            Self::Bool(value) => i32::from(*value).to_string(),
        }
    }

    pub fn id(&self) -> i32 {
        match self {
            Self::Id(id) => *id,
            _ => 0,
        }
    }
}

/// Items connected to computers (`glpi_computers_items`), legacy
/// `$CFG_GLPI['directconnect_types']`.
pub const DIRECT_CONNECT_KINDS: [AssetKind; 4] = [
    AssetKind::Monitor,
    AssetKind::Peripheral,
    AssetKind::Phone,
    AssetKind::Printer,
];

/// Components of an item (`Item_Device*` classes and their tables).
pub const DEVICE_ITEMS: [(&str, &str); 17] = [
    ("Item_DeviceMotherboard", "glpi_items_devicemotherboards"),
    ("Item_DeviceFirmware", "glpi_items_devicefirmwares"),
    ("Item_DeviceProcessor", "glpi_items_deviceprocessors"),
    ("Item_DeviceMemory", "glpi_items_devicememories"),
    ("Item_DeviceHardDrive", "glpi_items_deviceharddrives"),
    ("Item_DeviceNetworkCard", "glpi_items_devicenetworkcards"),
    ("Item_DeviceDrive", "glpi_items_devicedrives"),
    ("Item_DeviceBattery", "glpi_items_devicebatteries"),
    ("Item_DeviceGraphicCard", "glpi_items_devicegraphiccards"),
    ("Item_DeviceSoundCard", "glpi_items_devicesoundcards"),
    ("Item_DeviceControl", "glpi_items_devicecontrols"),
    ("Item_DevicePci", "glpi_items_devicepcis"),
    ("Item_DeviceCase", "glpi_items_devicecases"),
    ("Item_DevicePowerSupply", "glpi_items_devicepowersupplies"),
    ("Item_DeviceGeneric", "glpi_items_devicegenerics"),
    ("Item_DeviceSimcard", "glpi_items_devicesimcards"),
    ("Item_DeviceSensor", "glpi_items_devicesensors"),
];

/// Fields of the components, with their search options (same for all).
pub const DEVICE_FIELDS: [Field; 2] = [
    dropdown("locations_id", 13, "glpi_locations", true),
    dropdown("states_id", 14, "glpi_states", true),
];

/// Which changes of a computer are copied to its connected items and
/// components (`is_*_autoupdate` and `state_autoupdate_mode` settings,
/// legacy `Computer::post_updateItem`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Propagation {
    pub contact: bool,
    pub user: bool,
    pub group: bool,
    pub location: bool,
    pub state: bool,
}

impl Propagation {
    /// Changed columns copied to the connected items.
    pub fn to_connected_items<'a>(&self, changed: &[&'a str]) -> Vec<&'a str> {
        changed
            .iter()
            .copied()
            .filter(|column| match *column {
                "contact" | "contact_num" => self.contact,
                "users_id" => self.user,
                "groups_id" => self.group,
                "states_id" => self.state,
                "locations_id" => self.location,
                _ => false,
            })
            .collect()
    }

    /// Changed columns copied to the components.
    pub fn to_devices<'a>(&self, changed: &[&'a str]) -> Vec<&'a str> {
        changed
            .iter()
            .copied()
            .filter(|column| match *column {
                "states_id" => self.state,
                "locations_id" => self.location,
                _ => false,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_itemtypes_round_trip() {
        for kind in AssetKind::ALL {
            assert_eq!(AssetKind::from_slug(kind.slug()), Some(kind));
            assert_eq!(AssetKind::from_itemtype(kind.itemtype()), Some(kind));
        }
        assert_eq!(AssetKind::from_slug("tickets"), None);
    }

    #[test]
    fn search_options_match_the_php_code() {
        // Search::getOptions() of each class, recorded from the PHP code.
        let computer = AssetKind::Computer;
        let option = |kind: AssetKind, column| kind.field(column).unwrap().search_option;
        assert_eq!(option(computer, "name"), 1);
        assert_eq!(option(computer, "locations_id"), 3);
        assert_eq!(option(computer, "computertypes_id"), 4);
        assert_eq!(option(computer, "computermodels_id"), 40);
        assert_eq!(option(computer, "users_id"), 70);
        assert_eq!(option(computer, "uuid"), 47);
        assert_eq!(option(AssetKind::Phone, "number_line"), 9);
        assert_eq!(option(AssetKind::Peripheral, "brand"), 11);
        assert_eq!(option(AssetKind::NetworkEquipment, "networks_id"), 32);
        assert!(AssetKind::Monitor.field("networks_id").is_none());
    }

    #[test]
    fn null_and_empty_text_are_the_same() {
        let null = Value::Text(None);
        assert!(!Value::Text(Some(String::new())).differs_from(&null));
        assert!(Value::Text(Some("x".into())).differs_from(&null));
        assert!(Value::Id(2).differs_from(&Value::Id(1)));
        assert_eq!(Value::Bool(true).raw(), "1");
        assert_eq!(null.raw(), "");
    }

    #[test]
    fn propagation_follows_the_settings() {
        let changed = ["name", "contact", "users_id", "locations_id", "states_id"];
        let defaults = Propagation {
            contact: true,
            user: true,
            group: true,
            location: true,
            state: false,
        };
        assert_eq!(
            defaults.to_connected_items(&changed),
            ["contact", "users_id", "locations_id"]
        );
        assert_eq!(defaults.to_devices(&changed), ["locations_id"]);
        assert!(
            Propagation::default()
                .to_connected_items(&changed)
                .is_empty()
        );
    }
}
