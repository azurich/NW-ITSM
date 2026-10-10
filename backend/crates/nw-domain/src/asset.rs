//! Assets of the inventory: computers, monitors, printers, network
//! equipment, peripherals and phones. They share most of their columns, so
//! one description drives the reads, the writes and the history of all.

use crate::php;

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
    /// Whole number written as text (search option `number`): the legacy
    /// input filter keeps its digits (`intval(cleanInteger())`) and the
    /// stored value is compared as a number.
    Integer,
    /// Decimal number (search option `decimal`): `floatval(cleanDecimal())`,
    /// compared as a number.
    Decimal,
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

const fn flag(column: &'static str, search_option: i32) -> Field {
    Field {
        column,
        search_option,
        kind: FieldKind::Bool,
    }
}

const fn integer(column: &'static str, search_option: i32) -> Field {
    Field {
        column,
        search_option,
        kind: FieldKind::Integer,
    }
}

impl Field {
    /// Whether a new value changes the stored one, as compared by
    /// `CommonDBTM::update`: text exactly (NULL and an empty text are the
    /// same), numbers with PHP's `!=`.
    pub fn changes(&self, stored: &Value, new: &Value) -> bool {
        match (self.kind, stored, new) {
            (FieldKind::Integer | FieldKind::Decimal, Value::Text(old), Value::Text(new)) => {
                !php::loose_eq(
                    old.as_deref().unwrap_or_default(),
                    new.as_deref().unwrap_or_default(),
                )
            }
            _ => new.differs_from(stored),
        }
    }

    /// What the legacy form of a new item gives for this field: its fields
    /// are empty (`getEmpty`), the lists and switches post their first
    /// value, `prepareInputForAdd` turns the empty page counters into 0 and
    /// drops an empty size (`None`: the column keeps its default).
    pub fn form_default(&self) -> Option<Value> {
        Some(match self.kind {
            FieldKind::Text => Value::Text(Some(String::new())),
            FieldKind::Dropdown { .. } | FieldKind::User => Value::Id(0),
            FieldKind::Bool => Value::Bool(false),
            FieldKind::Integer => {
                Value::Text(Some(if self.column == "ram" { "" } else { "0" }.to_owned()))
            }
            FieldKind::Decimal => return None,
        })
    }
}

/// A number as the legacy input filter keeps it (`filterValues`, search
/// option `number`): its digits, as a whole number. Empty stays empty.
pub fn filter_integer(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let digits: String = text
        .replace(',', ".")
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    php::intval(&digits).to_string()
}

/// A decimal number as the legacy input filter keeps it (search option
/// `decimal`). Empty stays empty.
pub fn filter_decimal(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let kept: String = text
        .replace(',', ".")
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
        .collect();
    php::float_to_string(php::floatval(&kept))
}

/// Fields of every asset type, with the search options of the PHP code
/// (identical for the six types).
#[rustfmt::skip]
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
    flag("is_recursive", 86),
];

const NETWORK: Field = dropdown("networks_id", 32, "glpi_networks", false);

/// `is_global`: managed globally (one item for several computers).
const IS_GLOBAL: Field = flag("is_global", 82);

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

    /// Editable fields, in the order of the legacy form (its entity part
    /// first), which is the order of the history of a change.
    pub fn fields(self) -> Vec<Field> {
        self.form_order()
            .iter()
            .map(|column| {
                self.definition(column)
                    .unwrap_or_else(|| panic!("no field {column} for {self:?}"))
            })
            .collect()
    }

    /// Columns posted by the legacy form, in its order.
    #[rustfmt::skip]
    fn form_order(self) -> &'static [&'static str] {
        match self {
            Self::Computer => &[
                "is_recursive", "name", "states_id", "locations_id", "computertypes_id",
                "users_id_tech", "manufacturers_id", "groups_id_tech", "computermodels_id",
                "contact_num", "serial", "otherserial", "contact", "users_id", "networks_id",
                "groups_id", "uuid", "autoupdatesystems_id", "comment",
            ],
            Self::Monitor => &[
                "is_recursive", "name", "states_id", "locations_id", "monitortypes_id",
                "users_id_tech", "manufacturers_id", "groups_id_tech", "monitormodels_id", "serial",
                "otherserial", "contact_num", "contact", "users_id", "groups_id", "comment", "size",
                "is_global", "have_micro", "have_speaker", "have_subd", "have_bnc", "have_dvi",
                "have_pivot", "have_hdmi", "have_displayport",
            ],
            Self::Printer => &[
                "is_recursive", "name", "states_id", "locations_id", "printertypes_id",
                "users_id_tech", "manufacturers_id", "groups_id_tech", "printermodels_id",
                "contact_num", "serial", "contact", "otherserial", "users_id", "is_global",
                "groups_id", "networks_id", "memory_size", "init_pages_counter",
                "last_pages_counter", "comment", "have_serial", "have_parallel", "have_usb",
                "have_ethernet", "have_wifi",
            ],
            Self::NetworkEquipment => &[
                "is_recursive", "name", "states_id", "locations_id", "networkequipmenttypes_id",
                "users_id_tech", "manufacturers_id", "groups_id_tech", "networkequipmentmodels_id",
                "contact_num", "serial", "contact", "otherserial", "users_id", "networks_id",
                "groups_id", "ram", "comment",
            ],
            Self::Peripheral => &[
                "is_recursive", "name", "states_id", "locations_id", "peripheraltypes_id",
                "users_id_tech", "manufacturers_id", "groups_id_tech", "peripheralmodels_id",
                "contact_num", "serial", "contact", "otherserial", "users_id", "is_global",
                "groups_id", "comment", "brand",
            ],
            Self::Phone => &[
                "is_recursive", "name", "states_id", "locations_id", "phonetypes_id",
                "users_id_tech", "manufacturers_id", "groups_id_tech", "phonemodels_id",
                "contact_num", "serial", "otherserial", "contact", "users_id", "is_global",
                "groups_id", "brand", "phonepowersupplies_id", "number_line", "comment",
                "have_headset", "have_hp",
            ],
        }
    }

    /// A field of this type, with its search option.
    fn definition(self, column: &str) -> Option<Field> {
        let (type_column, type_table) = self.type_dropdown();
        let (model_column, model_table) = self.model_dropdown();
        if column == type_column {
            return Some(dropdown(type_column, 4, type_table, false));
        }
        if column == model_column {
            return Some(dropdown(model_column, 40, model_table, false));
        }
        if let Some(field) = COMMON_FIELDS.iter().find(|field| field.column == column) {
            return Some(*field);
        }
        let specific: &[Field] = match self {
            Self::Computer => &[
                text("uuid", 47),
                NETWORK,
                dropdown("autoupdatesystems_id", 42, "glpi_autoupdatesystems", false),
            ],
            Self::Monitor => &[
                Field {
                    column: "size",
                    search_option: 11,
                    kind: FieldKind::Decimal,
                },
                flag("have_micro", 41),
                flag("have_speaker", 42),
                flag("have_subd", 43),
                flag("have_bnc", 44),
                flag("have_dvi", 45),
                flag("have_pivot", 46),
                flag("have_hdmi", 47),
                flag("have_displayport", 48),
                IS_GLOBAL,
            ],
            Self::Printer => &[
                NETWORK,
                flag("have_serial", 42),
                flag("have_parallel", 43),
                flag("have_usb", 44),
                flag("have_ethernet", 45),
                flag("have_wifi", 46),
                text("memory_size", 13),
                integer("init_pages_counter", 11),
                integer("last_pages_counter", 12),
                IS_GLOBAL,
            ],
            Self::NetworkEquipment => &[NETWORK, integer("ram", 14)],
            Self::Peripheral => &[text("brand", 11), IS_GLOBAL],
            Self::Phone => &[
                text("number_line", 9),
                text("brand", 11),
                dropdown(
                    "phonepowersupplies_id",
                    42,
                    "glpi_phonepowersupplies",
                    false,
                ),
                flag("have_headset", 43),
                flag("have_hp", 44),
                IS_GLOBAL,
            ],
        };
        specific
            .iter()
            .find(|field| field.column == column)
            .copied()
    }

    /// `template_name`, a field of the templates only.
    pub fn template_name_field(self) -> Field {
        let search_option = match self {
            Self::Computer | Self::NetworkEquipment => 65,
            _ => 61,
        };
        text("template_name", search_option)
    }

    /// Fields of the form of a template: its name comes first, after the
    /// entity part.
    pub fn template_fields(self) -> Vec<Field> {
        let mut fields = self.fields();
        fields.insert(1, self.template_name_field());
        fields
    }

    /// Sorts values in the order of the legacy form (of a template or not).
    pub fn sort_by_form(self, values: &mut [(Field, Value)], template: bool) {
        let fields = if template {
            self.template_fields()
        } else {
            self.fields()
        };
        values.sort_by_key(|(field, _)| {
            fields
                .iter()
                .position(|known| known.column == field.column)
                .unwrap_or(usize::MAX)
        });
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
        // Fields of one type.
        assert_eq!(option(AssetKind::Monitor, "size"), 11);
        assert_eq!(option(AssetKind::Monitor, "have_displayport"), 48);
        assert_eq!(option(AssetKind::Printer, "have_wifi"), 46);
        assert_eq!(option(AssetKind::Printer, "memory_size"), 13);
        assert_eq!(option(AssetKind::Printer, "last_pages_counter"), 12);
        assert_eq!(option(AssetKind::NetworkEquipment, "ram"), 14);
        assert_eq!(option(AssetKind::Phone, "phonepowersupplies_id"), 42);
        assert_eq!(option(AssetKind::Phone, "have_hp"), 44);
        for kind in AssetKind::ALL {
            assert_eq!(kind.field("is_global").is_some(), kind.can_be_global());
            // Every column of the form is known, once.
            let fields = kind.fields();
            for field in &fields {
                assert_eq!(
                    fields
                        .iter()
                        .filter(|other| other.column == field.column)
                        .count(),
                    1
                );
            }
            assert_eq!(fields[0].column, "is_recursive");
        }
    }

    #[test]
    fn numbers_are_filtered_and_compared_like_php() {
        assert_eq!(filter_integer("1 024 Mio"), "1024");
        assert_eq!(filter_integer("1.5"), "15");
        assert_eq!(filter_integer(""), "");
        assert_eq!(filter_decimal("21,50"), "21.5");
        assert_eq!(filter_decimal("abc"), "0");
        let size = AssetKind::Monitor.field("size").unwrap();
        let text = |value: &str| Value::Text(Some(value.to_owned()));
        assert!(!size.changes(&text("21.50"), &text("21.5")));
        assert!(size.changes(&text("21.50"), &text("22")));
        let ram = AssetKind::NetworkEquipment.field("ram").unwrap();
        assert!(ram.changes(&text("abc"), &text("0")));
        assert!(!ram.changes(&Value::Text(None), &text("")));
        assert_eq!(size.form_default(), None);
        assert_eq!(ram.form_default(), Some(text("")));
        let counter = AssetKind::Printer.field("init_pages_counter").unwrap();
        assert_eq!(counter.form_default(), Some(text("0")));
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
