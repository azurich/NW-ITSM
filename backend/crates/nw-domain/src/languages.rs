//! Languages of the legacy application (`$CFG_GLPI['languages']` in
//! `inc/define.php`), in the same order.

/// A language: code, native name and translation file (`locales/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
    /// As declared: a missing file means English (`Session::loadLanguage`).
    pub file: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language {
        code: "ar_SA",
        name: "العَرَبِيَّةُ",
        file: "ar_SA.mo",
    },
    Language {
        code: "bg_BG",
        name: "Български",
        file: "bg_BG.mo",
    },
    Language {
        code: "id_ID",
        name: "Bahasa Indonesia",
        file: "id_ID.mo",
    },
    Language {
        code: "ms_MY",
        name: "Bahasa Melayu",
        file: "ms_MY.mo",
    },
    Language {
        code: "ca_ES",
        name: "Català",
        file: "ca_ES.mo",
    },
    Language {
        code: "cs_CZ",
        name: "Čeština",
        file: "cs_CZ.mo",
    },
    Language {
        code: "de_DE",
        name: "Deutsch",
        file: "de_DE.mo",
    },
    Language {
        code: "da_DK",
        name: "Dansk",
        file: "da_DK.mo",
    },
    Language {
        code: "et_EE",
        name: "Eesti",
        file: "et_EE.mo",
    },
    Language {
        code: "en_GB",
        name: "English",
        file: "en_GB.mo",
    },
    Language {
        code: "en_US",
        name: "English (US)",
        file: "en_US.mo",
    },
    Language {
        code: "es_AR",
        name: "Español (Argentina)",
        file: "es_AR.mo",
    },
    Language {
        code: "es_CO",
        name: "Español (Colombia)",
        file: "es_CO.mo",
    },
    Language {
        code: "es_ES",
        name: "Español (España)",
        file: "es_ES.mo",
    },
    Language {
        code: "es_419",
        name: "Español (América Latina)",
        file: "es_419.mo",
    },
    Language {
        code: "es_MX",
        name: "Español (Mexico)",
        file: "es_MX.mo",
    },
    Language {
        code: "es_VE",
        name: "Español (Venezuela)",
        file: "es_VE.mo",
    },
    Language {
        code: "eu_ES",
        name: "Euskara",
        file: "eu_ES.mo",
    },
    Language {
        code: "fr_FR",
        name: "Français",
        file: "fr_FR.mo",
    },
    Language {
        code: "fr_CA",
        name: "Français (Canada)",
        file: "fr_CA.mo",
    },
    Language {
        code: "fr_BE",
        name: "Français (Belgique)",
        file: "fr_BE.mo",
    },
    Language {
        code: "gl_ES",
        name: "Galego",
        file: "gl_ES.mo",
    },
    Language {
        code: "el_GR",
        name: "Ελληνικά",
        file: "el_GR.mo",
    },
    Language {
        code: "he_IL",
        name: "עברית",
        file: "he_IL.mo",
    },
    Language {
        code: "hi_IN",
        name: "हिन्दी",
        file: "hi_IN.mo",
    },
    Language {
        code: "hr_HR",
        name: "Hrvatski",
        file: "hr_HR.mo",
    },
    Language {
        code: "hu_HU",
        name: "Magyar",
        file: "hu_HU.mo",
    },
    Language {
        code: "it_IT",
        name: "Italiano",
        file: "it_IT.mo",
    },
    Language {
        code: "kn",
        name: "ಕನ್ನಡ",
        file: "kn.mo",
    },
    Language {
        code: "lv_LV",
        name: "Latviešu",
        file: "lv_LV.mo",
    },
    Language {
        code: "lt_LT",
        name: "Lietuvių",
        file: "lt_LT.mo",
    },
    Language {
        code: "mn_MN",
        name: "Монгол хэл",
        file: "mn_MN.mo",
    },
    Language {
        code: "nl_NL",
        name: "Nederlands",
        file: "nl_NL.mo",
    },
    Language {
        code: "nl_BE",
        name: "Flemish",
        file: "nl_BE.mo",
    },
    Language {
        code: "nb_NO",
        name: "Norsk (Bokmål)",
        file: "nb_NO.mo",
    },
    Language {
        code: "nn_NO",
        name: "Norsk (Nynorsk)",
        file: "nn_NO.mo",
    },
    Language {
        code: "fa_IR",
        name: "فارسی",
        file: "fa_IR.mo",
    },
    Language {
        code: "pl_PL",
        name: "Polski",
        file: "pl_PL.mo",
    },
    Language {
        code: "pt_PT",
        name: "Português",
        file: "pt_PT.mo",
    },
    Language {
        code: "pt_BR",
        name: "Português do Brasil",
        file: "pt_BR.mo",
    },
    Language {
        code: "ro_RO",
        name: "Română",
        file: "ro_RO.mo",
    },
    Language {
        code: "ru_RU",
        name: "Русский",
        file: "ru_RU.mo",
    },
    Language {
        code: "sk_SK",
        name: "Slovenčina",
        file: "sk_SK.mo",
    },
    Language {
        code: "sl_SI",
        name: "Slovenščina",
        file: "sl_SI.mo",
    },
    Language {
        code: "sr_RS",
        name: "Srpski",
        file: "sr_RS.mo",
    },
    Language {
        code: "fi_FI",
        name: "Suomi",
        file: "fi_FI.mo",
    },
    Language {
        code: "sv_SE",
        name: "Svenska",
        file: "sv_SE.mo",
    },
    Language {
        code: "vi_VN",
        name: "Tiếng Việt",
        file: "vi_VN.mo",
    },
    Language {
        code: "th_TH",
        name: "ภาษาไทย",
        file: "th_TH.mo",
    },
    Language {
        code: "tr_TR",
        name: "Türkçe",
        file: "tr_TR.mo",
    },
    Language {
        code: "uk_UA",
        name: "Українська",
        file: "uk_UA.mo",
    },
    Language {
        code: "ja_JP",
        name: "日本語",
        file: "ja_JP.mo",
    },
    Language {
        code: "zh_CN",
        name: "简体中文",
        file: "zh_CN.mo",
    },
    Language {
        code: "zh_TW",
        name: "繁體中文",
        file: "zh_TW.mo",
    },
    Language {
        code: "ko_KR",
        name: "한국/韓國",
        file: "ko_KR.mo",
    },
    Language {
        code: "zh_HK",
        name: "香港",
        file: "zh_HK.mo",
    },
    Language {
        code: "be_BY",
        name: "Belarussian",
        file: "be_BY.mo",
    },
    Language {
        code: "is_IS",
        name: "íslenska",
        file: "is_IS.mo",
    },
    Language {
        code: "eo",
        name: "Esperanto",
        file: "eo.mo",
    },
    Language {
        code: "es_CL",
        name: "Español chileno",
        file: "es_CL",
    },
    Language {
        code: "ta",
        name: "தமிழ்",
        file: "ta_IN.mo",
    },
];

/// Whether the code is a language the legacy application knows.
pub fn is_known(code: &str) -> bool {
    find(code).is_some()
}

pub fn find(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|language| language.code == code)
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
        // Declared in GLPI 9.5 too.
        for code in ["es_419", "kn", "eo", "ta"] {
            assert!(is_known(code), "{code}");
        }
        assert_eq!(LANGUAGES.len(), 61);
    }
}
