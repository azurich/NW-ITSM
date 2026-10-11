//! Texts of the legacy application in the languages of its users.
//!
//! The PHP code writes some texts translated in the database (the event
//! log). The translations the API needs come from the gettext files of the
//! application (`locales/*.mo`, loaded by Laminas): they are extracted into
//! `i18n/catalog.tsv` with the same resolution rules (plural rule, context,
//! fallback to English), so the binary does not depend on the PHP files.
//! Regenerate it with `UPDATE_TRANSLATIONS=1 cargo test -p nw-domain --test
//! translations`.

use std::{collections::HashMap, sync::OnceLock};

use crate::languages;

/// How the PHP code asks for a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `__($text)`.
    Text(&'static str),
    /// `_n($singular, $plural, 1)`.
    One(&'static str, &'static str),
    /// `_x($context, $text)`.
    Context(&'static str, &'static str),
}

impl Source {
    /// The English text, also the key of the text in the catalog.
    pub fn id(self) -> &'static str {
        match self {
            Self::Text(text) | Self::One(text, _) | Self::Context(_, text) => text,
        }
    }
}

/// Texts of the API.
pub mod text {
    use super::Source::{self, Context, One, Text};

    pub const LOGIN: Source = Text("%1$s log in from IP %2$s");
    pub const FAILED_LOGIN: Source = Text("Failed login for %1$s from IP %2$s");
    pub const ADDS_ITEM: Source = Text("%1$s adds the item %2$s");
    pub const UPDATES_ITEM: Source = Text("%s updates an item");
    pub const DELETES_ITEM: Source = Text("%s deletes an item");
    pub const RESTORES_ITEM: Source = Text("%s restores an item");
    pub const PURGES_ITEM: Source = Text("%s purges an item");
    pub const ADDS_VERSION: Source = Text("%1$s adds the version %2$s");
    pub const UPDATES_VERSION: Source = Text("%1$s updates the version %2$s");
    pub const PURGES_VERSION: Source = Text("%1$s purges the version %2$s");
    pub const INSTALLS_SOFTWARE: Source = Text("%s installs software");
    pub const DUPLICATE_EVENT: Source =
        Text("%1$s trying to add an item that already exists: %2$s");
    pub const IMPOSSIBLE_RECORD: Source = Text("Impossible record for %s");
    pub const DUPLICATE_RECORD: Source = Text("Item successfully added but duplicate record on %s");
    pub const OTHER_ITEM_EXIST: Source = Text("Other item exist");
    pub const IN_TRASHBIN: Source = Text("Item in the trashbin");
    pub const EQUALS: Source = Text("%1$s = %2$s");
    pub const COLON: Source = Text("%1$s: %2$s");
    pub const DASH: Source = Text("%1$s - %2$s");
    pub const NAME_ID: Source = Text("%1$s (%2$s)");
    pub const ID: Source = Text("ID");
    pub const ENTITY: Source = One("Entity", "Entities");
    pub const NAME: Source = Text("Name");
    pub const SERIAL: Source = Text("Serial number");
    pub const INVENTORY_NUMBER: Source = Text("Inventory number");
    pub const CONTACT: Source = Text("Alternate username");
    pub const CONTACT_NUMBER: Source = Text("Alternate username number");
    pub const UUID: Source = Text("UUID");
    pub const BRAND: Source = Text("Brand");
    pub const NUMBER_LINE: Source = Context("quantity", "Number of lines");
    pub const USER: Source = One("User", "Users");
    pub const MANUFACTURER: Source = One("Manufacturer", "Manufacturers");
    pub const TYPE: Source = One("Type", "Types");
    pub const MODEL: Source = One("Model", "Models");
    pub const NETWORK: Source = One("Network", "Networks");
    pub const UPDATE_SOURCE: Source = One("Update Source", "Update Sources");
    pub const LOCATION_COMMENTS: Source = Text("Location comments");
    pub const CHILD_ENTITIES: Source = Text("Child entities");
    pub const SIZE: Source = Text("Size");
    pub const MICROPHONE: Source = Text("Microphone");
    pub const SPEAKERS: Source = Text("Speakers");
    pub const SUB_D: Source = Text("Sub-D");
    pub const BNC: Source = Text("BNC");
    pub const DVI: Source = Text("DVI");
    pub const PIVOT: Source = Text("Pivot");
    pub const HDMI: Source = Text("HDMI");
    pub const DISPLAYPORT: Source = Text("DisplayPort");
    pub const GLOBAL_MANAGEMENT: Source = Text("Global management");
    pub const SERIAL_PORT: Source = Text("Serial");
    pub const PARALLEL: Source = Text("Parallel");
    pub const USB: Source = Text("USB");
    pub const ETHERNET: Source = Text("Ethernet");
    pub const WIFI: Source = Text("Wifi");
    pub const MEMORY: Source = One("Memory", "Memories");
    pub const MIO: Source = Text("Mio");
    pub const INITIAL_PAGE_COUNTER: Source = Text("Initial page counter");
    pub const PAGE_COUNTER: Source = Text("Current counter of pages");
    pub const POWER_SUPPLY: Source = One("Power supply", "Power supplies");
    pub const HEADSET: Source = Text("Headset");
    pub const SPEAKER: Source = Text("Speaker");
    pub const TECH_USER: Source = Text("Technician in charge of the hardware");
    pub const COMMENTS: Source = Text("Comments");
    pub const TEMPLATE_NAME: Source = Text("Template name");
    pub const NUMBER_OF_UNITS: Source = Text("Number of units");
    pub const SERVER_ROOM: Source = One("Server room", "Server rooms");
    pub const PUBLISHER: Source = Text("Publisher");
    pub const TECH_SOFTWARE: Source = Text("Technician in charge of the software");
    pub const HELPDESK_VISIBLE: Source = Text("Associable to a ticket");

    /// Every text, for the catalog.
    pub const ALL: [Source; 68] = [
        LOGIN,
        FAILED_LOGIN,
        ADDS_ITEM,
        UPDATES_ITEM,
        DELETES_ITEM,
        RESTORES_ITEM,
        PURGES_ITEM,
        DUPLICATE_EVENT,
        IMPOSSIBLE_RECORD,
        DUPLICATE_RECORD,
        OTHER_ITEM_EXIST,
        IN_TRASHBIN,
        EQUALS,
        COLON,
        DASH,
        NAME_ID,
        ID,
        ENTITY,
        NAME,
        SERIAL,
        INVENTORY_NUMBER,
        CONTACT,
        CONTACT_NUMBER,
        UUID,
        BRAND,
        NUMBER_LINE,
        USER,
        MANUFACTURER,
        TYPE,
        MODEL,
        NETWORK,
        UPDATE_SOURCE,
        LOCATION_COMMENTS,
        CHILD_ENTITIES,
        SIZE,
        MICROPHONE,
        SPEAKERS,
        SUB_D,
        BNC,
        DVI,
        PIVOT,
        HDMI,
        DISPLAYPORT,
        GLOBAL_MANAGEMENT,
        SERIAL_PORT,
        PARALLEL,
        USB,
        ETHERNET,
        WIFI,
        MEMORY,
        MIO,
        INITIAL_PAGE_COUNTER,
        PAGE_COUNTER,
        POWER_SUPPLY,
        HEADSET,
        SPEAKER,
        TECH_USER,
        COMMENTS,
        TEMPLATE_NAME,
        NUMBER_OF_UNITS,
        SERVER_ROOM,
        PUBLISHER,
        TECH_SOFTWARE,
        HELPDESK_VISIBLE,
        ADDS_VERSION,
        UPDATES_VERSION,
        PURGES_VERSION,
        INSTALLS_SOFTWARE,
    ];
}

/// File loaded when the one of a language is missing.
pub const FALLBACK_FILE: &str = "en_GB.mo";

const CATALOG: &str = include_str!("i18n/catalog.tsv");

/// Translations by file, then by text id.
fn catalog() -> &'static HashMap<&'static str, HashMap<&'static str, String>> {
    static PARSED: OnceLock<HashMap<&'static str, HashMap<&'static str, String>>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let mut files: HashMap<&str, HashMap<&str, String>> = HashMap::new();
        for line in CATALOG.lines().filter(|line| !line.is_empty()) {
            let mut columns = line.splitn(3, '\t');
            let (Some(file), Some(id), Some(value)) =
                (columns.next(), columns.next(), columns.next())
            else {
                continue;
            };
            files.entry(file).or_default().insert(id, unescape(value));
        }
        files
    })
}

/// A catalog value: `\t`, `\n` and `\\` escaped.
pub fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Translation file of a language (`Session::loadLanguage`): English when
/// the language or its file is unknown.
pub fn file_of(language: &str) -> &'static str {
    languages::find(language)
        .map(|language| language.file)
        .filter(|file| catalog().contains_key(file))
        .unwrap_or(FALLBACK_FILE)
}

/// A text in a language.
pub fn tr(language: &str, source: Source) -> String {
    catalog()
        .get(file_of(language))
        .and_then(|texts| texts.get(source.id()))
        .cloned()
        .unwrap_or_else(|| source.id().to_owned())
}

/// `sprintf` for the texts: `%s`, `%d`, positional `%1$s`, and `%%`.
pub fn sprintf(format: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(format.len() + 16);
    let mut next = 0;
    let mut rest = format;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('%') {
            out.push('%');
            rest = after;
            continue;
        }
        // Optional `<n>$`.
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let (index, spec_at) = if digits > 0 && rest[digits..].starts_with('$') {
            (
                rest[..digits]
                    .parse::<usize>()
                    .ok()
                    .map(|n| n.saturating_sub(1)),
                digits + 1,
            )
        } else {
            (None, 0)
        };
        match rest[spec_at..].chars().next() {
            Some(spec @ ('s' | 'd')) => {
                let index = index.unwrap_or_else(|| {
                    next += 1;
                    next - 1
                });
                let value = args.get(index).copied().unwrap_or_default();
                if spec == 'd' {
                    // `(int)` of a numeric prefix.
                    let number: String = value
                        .trim_start()
                        .chars()
                        .enumerate()
                        .take_while(|(i, c)| {
                            c.is_ascii_digit() || (*i == 0 && (*c == '-' || *c == '+'))
                        })
                        .map(|(_, c)| c)
                        .collect();
                    out.push_str(&number.parse::<i64>().unwrap_or(0).to_string());
                } else {
                    out.push_str(value);
                }
                rest = &rest[spec_at + 1..];
            }
            _ => out.push('%'),
        }
    }
    out.push_str(rest);
    out
}

/// Language of a request without session (`Session::getPreferredLanguage`):
/// the best `Accept-Language` the application knows, else the default.
pub fn preferred_language(accept_language: Option<&str>, default: &str) -> String {
    // A PHP array: a language given twice keeps its first place.
    let mut accepted: Vec<(String, f64)> = Vec::new();
    for value in accept_language.unwrap_or_default().split(',') {
        let mut parts = value.trim().split(";q=");
        let language = parts.next().unwrap_or_default().replace('-', "_");
        let quality = parts
            .next()
            .map_or(Some(1.0), crate::php::numeric)
            .unwrap_or(0.0);
        match accepted.iter_mut().find(|(known, _)| *known == language) {
            Some(entry) => entry.1 = quality,
            None => accepted.push((language, quality)),
        }
    }
    // `arsort`, stable.
    accepted.sort_by(|a, b| b.1.total_cmp(&a.1));
    accepted
        .into_iter()
        .map(|(language, _)| language)
        .find(|language| languages::is_known(language))
        .unwrap_or_else(|| default.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_sprintf() {
        assert_eq!(
            sprintf("%1$s log in from IP %2$s", &["itsm", "::1"]),
            "itsm log in from IP ::1"
        );
        assert_eq!(sprintf("%2$s, %1$s", &["a", "b"]), "b, a");
        assert_eq!(
            sprintf("%s updates an item", &["itsm"]),
            "itsm updates an item"
        );
        assert_eq!(sprintf("%d%% of %s", &["12abc", "x"]), "12% of x");
        assert_eq!(sprintf("50% sure %s", &["x"]), "50% sure x");
    }

    #[test]
    fn translations_follow_the_language() {
        assert_eq!(
            tr("fr_FR", text::LOGIN),
            "%1$s se connecte depuis l'IP %2$s"
        );
        assert_eq!(tr("fr_FR", text::ENTITY), "Entité");
        assert_eq!(tr("en_GB", text::LOGIN), "%1$s log in from IP %2$s");
        // Unknown language, or declared file missing: English.
        assert_eq!(file_of("xx_XX"), FALLBACK_FILE);
        assert_eq!(file_of("es_CL"), FALLBACK_FILE);
        assert_eq!(file_of("ta"), FALLBACK_FILE);
        assert_eq!(file_of("de_DE"), "de_DE.mo");
    }

    #[test]
    fn language_of_the_browser() {
        assert_eq!(
            preferred_language(Some("fr-FR,fr;q=0.9,en;q=0.8"), "en_GB"),
            "fr_FR"
        );
        assert_eq!(
            preferred_language(Some("de;q=0.9, pt-BR;q=0.95"), "en_GB"),
            "pt_BR"
        );
        assert_eq!(preferred_language(Some("fr"), "es_ES"), "es_ES");
        // Same quality: the order of the header, a repeated language
        // keeping its first place.
        assert_eq!(
            preferred_language(Some("de-DE;q=0.5, it-IT, de-DE"), "en_GB"),
            "de_DE"
        );
        assert_eq!(preferred_language(None, "fr_FR"), "fr_FR");
    }
}
