//! Languages of the legacy application (`$CFG_GLPI['languages']` in
//! `inc/define.php`), as `(code, native name)`.

pub const LANGUAGES: &[(&str, &str)] = &[
    ("ar_SA", "العَرَبِيَّةُ"),
    ("bg_BG", "Български"),
    ("id_ID", "Bahasa Indonesia"),
    ("ms_MY", "Bahasa Melayu"),
    ("ca_ES", "Català"),
    ("cs_CZ", "Čeština"),
    ("de_DE", "Deutsch"),
    ("da_DK", "Dansk"),
    ("et_EE", "Eesti"),
    ("en_GB", "English"),
    ("en_US", "English (US)"),
    ("es_AR", "Español (Argentina)"),
    ("es_CO", "Español (Colombia)"),
    ("es_ES", "Español (España)"),
    ("es_MX", "Español (Mexico)"),
    ("es_VE", "Español (Venezuela)"),
    ("eu_ES", "Euskara"),
    ("fr_FR", "Français"),
    ("fr_CA", "Français (Canada)"),
    ("fr_BE", "Français (Belgique)"),
    ("gl_ES", "Galego"),
    ("el_GR", "Ελληνικά"),
    ("he_IL", "עברית"),
    ("hi_IN", "हिन्दी"),
    ("hr_HR", "Hrvatski"),
    ("hu_HU", "Magyar"),
    ("it_IT", "Italiano"),
    ("lv_LV", "Latviešu"),
    ("lt_LT", "Lietuvių"),
    ("mn_MN", "Монгол хэл"),
    ("nl_NL", "Nederlands"),
    ("nl_BE", "Flemish"),
    ("nb_NO", "Norsk (Bokmål)"),
    ("nn_NO", "Norsk (Nynorsk)"),
    ("fa_IR", "فارسی"),
    ("pl_PL", "Polski"),
    ("pt_PT", "Português"),
    ("pt_BR", "Português do Brasil"),
    ("ro_RO", "Română"),
    ("ru_RU", "Русский"),
    ("sk_SK", "Slovenčina"),
    ("sl_SI", "Slovenščina"),
    ("sr_RS", "Srpski"),
    ("fi_FI", "Suomi"),
    ("sv_SE", "Svenska"),
    ("vi_VN", "Tiếng Việt"),
    ("th_TH", "ภาษาไทย"),
    ("tr_TR", "Türkçe"),
    ("uk_UA", "Українська"),
    ("ja_JP", "日本語"),
    ("zh_CN", "简体中文"),
    ("zh_TW", "繁體中文"),
    ("ko_KR", "한국/韓國"),
    ("zh_HK", "香港"),
    ("be_BY", "Belarussian"),
    ("is_IS", "íslenska"),
    ("es_CL", "Español chileno"),
];

/// Whether the code is a language the legacy application knows.
pub fn is_known(code: &str) -> bool {
    LANGUAGES.iter().any(|(known, _)| *known == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_languages() {
        assert!(is_known("fr_FR"));
        assert!(is_known("en_GB"));
        assert!(!is_known("fr"));
        assert!(!is_known("az_AZ"));
    }
}
