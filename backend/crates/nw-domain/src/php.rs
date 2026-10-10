//! PHP comparison semantics the legacy code relies on (PHP 8): loose
//! equality of strings, `empty()`, `trim()` and case-insensitive search.

/// Number of a PHP numeric string (`is_numeric`): optional surrounding
/// whitespace, sign, digits with an optional fraction and exponent.
pub fn numeric(value: &str) -> Option<f64> {
    let trimmed =
        value.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0B' | '\x0C'));
    let body = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let has_digits = !integer.is_empty() || fraction.is_some_and(|fraction| !fraction.is_empty());
    if !has_digits || !digits(integer) || !fraction.is_none_or(digits) {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !digits(exponent) {
            return None;
        }
    }
    trimmed.parse::<f64>().ok()
}

/// `$a == $b` for two strings: numeric strings are compared as numbers.
pub fn loose_eq(a: &str, b: &str) -> bool {
    match (numeric(a), numeric(b)) {
        (Some(x), Some(y)) => match (a.trim().parse::<i64>(), b.trim().parse::<i64>()) {
            (Ok(x), Ok(y)) => x == y,
            _ => x == y,
        },
        _ => a == b,
    }
}

/// `empty($value)` for a string.
pub fn is_empty(value: &str) -> bool {
    value.is_empty() || value == "0"
}

/// `trim()` with its default characters.
pub fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B'))
}

/// `mb_strtolower`.
pub fn lower(value: &str) -> String {
    value.to_lowercase()
}

/// Whether `mb_stripos($haystack, $needle)` finds the needle, and at the
/// start when `at_start`.
pub fn contains_ci(haystack: &str, needle: &str, at_start: bool) -> bool {
    let (haystack, needle) = (lower(haystack), lower(needle));
    if at_start {
        haystack.starts_with(&needle)
    } else {
        haystack.contains(&needle)
    }
}

/// `htmlentities($text, ENT_QUOTES, 'utf-8')`.
pub fn htmlentities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match ENTITIES.binary_search_by_key(&c, |(known, _)| *known) {
            Ok(index) => out.push_str(ENTITIES[index].1),
            Err(_) => out.push(c),
        }
    }
    out
}

/// Table of `htmlentities($text, ENT_QUOTES, 'utf-8')` (HTML 4.01), by code
/// point: `get_html_translation_table(HTML_ENTITIES, ENT_QUOTES, 'UTF-8')`.
const ENTITIES: [(char, &str); 253] = [
    ('\u{22}', "&quot;"),
    ('\u{26}', "&amp;"),
    ('\u{27}', "&#039;"),
    ('\u{3c}', "&lt;"),
    ('\u{3e}', "&gt;"),
    ('\u{a0}', "&nbsp;"),
    ('\u{a1}', "&iexcl;"),
    ('\u{a2}', "&cent;"),
    ('\u{a3}', "&pound;"),
    ('\u{a4}', "&curren;"),
    ('\u{a5}', "&yen;"),
    ('\u{a6}', "&brvbar;"),
    ('\u{a7}', "&sect;"),
    ('\u{a8}', "&uml;"),
    ('\u{a9}', "&copy;"),
    ('\u{aa}', "&ordf;"),
    ('\u{ab}', "&laquo;"),
    ('\u{ac}', "&not;"),
    ('\u{ad}', "&shy;"),
    ('\u{ae}', "&reg;"),
    ('\u{af}', "&macr;"),
    ('\u{b0}', "&deg;"),
    ('\u{b1}', "&plusmn;"),
    ('\u{b2}', "&sup2;"),
    ('\u{b3}', "&sup3;"),
    ('\u{b4}', "&acute;"),
    ('\u{b5}', "&micro;"),
    ('\u{b6}', "&para;"),
    ('\u{b7}', "&middot;"),
    ('\u{b8}', "&cedil;"),
    ('\u{b9}', "&sup1;"),
    ('\u{ba}', "&ordm;"),
    ('\u{bb}', "&raquo;"),
    ('\u{bc}', "&frac14;"),
    ('\u{bd}', "&frac12;"),
    ('\u{be}', "&frac34;"),
    ('\u{bf}', "&iquest;"),
    ('\u{c0}', "&Agrave;"),
    ('\u{c1}', "&Aacute;"),
    ('\u{c2}', "&Acirc;"),
    ('\u{c3}', "&Atilde;"),
    ('\u{c4}', "&Auml;"),
    ('\u{c5}', "&Aring;"),
    ('\u{c6}', "&AElig;"),
    ('\u{c7}', "&Ccedil;"),
    ('\u{c8}', "&Egrave;"),
    ('\u{c9}', "&Eacute;"),
    ('\u{ca}', "&Ecirc;"),
    ('\u{cb}', "&Euml;"),
    ('\u{cc}', "&Igrave;"),
    ('\u{cd}', "&Iacute;"),
    ('\u{ce}', "&Icirc;"),
    ('\u{cf}', "&Iuml;"),
    ('\u{d0}', "&ETH;"),
    ('\u{d1}', "&Ntilde;"),
    ('\u{d2}', "&Ograve;"),
    ('\u{d3}', "&Oacute;"),
    ('\u{d4}', "&Ocirc;"),
    ('\u{d5}', "&Otilde;"),
    ('\u{d6}', "&Ouml;"),
    ('\u{d7}', "&times;"),
    ('\u{d8}', "&Oslash;"),
    ('\u{d9}', "&Ugrave;"),
    ('\u{da}', "&Uacute;"),
    ('\u{db}', "&Ucirc;"),
    ('\u{dc}', "&Uuml;"),
    ('\u{dd}', "&Yacute;"),
    ('\u{de}', "&THORN;"),
    ('\u{df}', "&szlig;"),
    ('\u{e0}', "&agrave;"),
    ('\u{e1}', "&aacute;"),
    ('\u{e2}', "&acirc;"),
    ('\u{e3}', "&atilde;"),
    ('\u{e4}', "&auml;"),
    ('\u{e5}', "&aring;"),
    ('\u{e6}', "&aelig;"),
    ('\u{e7}', "&ccedil;"),
    ('\u{e8}', "&egrave;"),
    ('\u{e9}', "&eacute;"),
    ('\u{ea}', "&ecirc;"),
    ('\u{eb}', "&euml;"),
    ('\u{ec}', "&igrave;"),
    ('\u{ed}', "&iacute;"),
    ('\u{ee}', "&icirc;"),
    ('\u{ef}', "&iuml;"),
    ('\u{f0}', "&eth;"),
    ('\u{f1}', "&ntilde;"),
    ('\u{f2}', "&ograve;"),
    ('\u{f3}', "&oacute;"),
    ('\u{f4}', "&ocirc;"),
    ('\u{f5}', "&otilde;"),
    ('\u{f6}', "&ouml;"),
    ('\u{f7}', "&divide;"),
    ('\u{f8}', "&oslash;"),
    ('\u{f9}', "&ugrave;"),
    ('\u{fa}', "&uacute;"),
    ('\u{fb}', "&ucirc;"),
    ('\u{fc}', "&uuml;"),
    ('\u{fd}', "&yacute;"),
    ('\u{fe}', "&thorn;"),
    ('\u{ff}', "&yuml;"),
    ('\u{152}', "&OElig;"),
    ('\u{153}', "&oelig;"),
    ('\u{160}', "&Scaron;"),
    ('\u{161}', "&scaron;"),
    ('\u{178}', "&Yuml;"),
    ('\u{192}', "&fnof;"),
    ('\u{2c6}', "&circ;"),
    ('\u{2dc}', "&tilde;"),
    ('\u{391}', "&Alpha;"),
    ('\u{392}', "&Beta;"),
    ('\u{393}', "&Gamma;"),
    ('\u{394}', "&Delta;"),
    ('\u{395}', "&Epsilon;"),
    ('\u{396}', "&Zeta;"),
    ('\u{397}', "&Eta;"),
    ('\u{398}', "&Theta;"),
    ('\u{399}', "&Iota;"),
    ('\u{39a}', "&Kappa;"),
    ('\u{39b}', "&Lambda;"),
    ('\u{39c}', "&Mu;"),
    ('\u{39d}', "&Nu;"),
    ('\u{39e}', "&Xi;"),
    ('\u{39f}', "&Omicron;"),
    ('\u{3a0}', "&Pi;"),
    ('\u{3a1}', "&Rho;"),
    ('\u{3a3}', "&Sigma;"),
    ('\u{3a4}', "&Tau;"),
    ('\u{3a5}', "&Upsilon;"),
    ('\u{3a6}', "&Phi;"),
    ('\u{3a7}', "&Chi;"),
    ('\u{3a8}', "&Psi;"),
    ('\u{3a9}', "&Omega;"),
    ('\u{3b1}', "&alpha;"),
    ('\u{3b2}', "&beta;"),
    ('\u{3b3}', "&gamma;"),
    ('\u{3b4}', "&delta;"),
    ('\u{3b5}', "&epsilon;"),
    ('\u{3b6}', "&zeta;"),
    ('\u{3b7}', "&eta;"),
    ('\u{3b8}', "&theta;"),
    ('\u{3b9}', "&iota;"),
    ('\u{3ba}', "&kappa;"),
    ('\u{3bb}', "&lambda;"),
    ('\u{3bc}', "&mu;"),
    ('\u{3bd}', "&nu;"),
    ('\u{3be}', "&xi;"),
    ('\u{3bf}', "&omicron;"),
    ('\u{3c0}', "&pi;"),
    ('\u{3c1}', "&rho;"),
    ('\u{3c2}', "&sigmaf;"),
    ('\u{3c3}', "&sigma;"),
    ('\u{3c4}', "&tau;"),
    ('\u{3c5}', "&upsilon;"),
    ('\u{3c6}', "&phi;"),
    ('\u{3c7}', "&chi;"),
    ('\u{3c8}', "&psi;"),
    ('\u{3c9}', "&omega;"),
    ('\u{3d1}', "&thetasym;"),
    ('\u{3d2}', "&upsih;"),
    ('\u{3d6}', "&piv;"),
    ('\u{2002}', "&ensp;"),
    ('\u{2003}', "&emsp;"),
    ('\u{2009}', "&thinsp;"),
    ('\u{200c}', "&zwnj;"),
    ('\u{200d}', "&zwj;"),
    ('\u{200e}', "&lrm;"),
    ('\u{200f}', "&rlm;"),
    ('\u{2013}', "&ndash;"),
    ('\u{2014}', "&mdash;"),
    ('\u{2018}', "&lsquo;"),
    ('\u{2019}', "&rsquo;"),
    ('\u{201a}', "&sbquo;"),
    ('\u{201c}', "&ldquo;"),
    ('\u{201d}', "&rdquo;"),
    ('\u{201e}', "&bdquo;"),
    ('\u{2020}', "&dagger;"),
    ('\u{2021}', "&Dagger;"),
    ('\u{2022}', "&bull;"),
    ('\u{2026}', "&hellip;"),
    ('\u{2030}', "&permil;"),
    ('\u{2032}', "&prime;"),
    ('\u{2033}', "&Prime;"),
    ('\u{2039}', "&lsaquo;"),
    ('\u{203a}', "&rsaquo;"),
    ('\u{203e}', "&oline;"),
    ('\u{2044}', "&frasl;"),
    ('\u{20ac}', "&euro;"),
    ('\u{2111}', "&image;"),
    ('\u{2118}', "&weierp;"),
    ('\u{211c}', "&real;"),
    ('\u{2122}', "&trade;"),
    ('\u{2135}', "&alefsym;"),
    ('\u{2190}', "&larr;"),
    ('\u{2191}', "&uarr;"),
    ('\u{2192}', "&rarr;"),
    ('\u{2193}', "&darr;"),
    ('\u{2194}', "&harr;"),
    ('\u{21b5}', "&crarr;"),
    ('\u{21d0}', "&lArr;"),
    ('\u{21d1}', "&uArr;"),
    ('\u{21d2}', "&rArr;"),
    ('\u{21d3}', "&dArr;"),
    ('\u{21d4}', "&hArr;"),
    ('\u{2200}', "&forall;"),
    ('\u{2202}', "&part;"),
    ('\u{2203}', "&exist;"),
    ('\u{2205}', "&empty;"),
    ('\u{2207}', "&nabla;"),
    ('\u{2208}', "&isin;"),
    ('\u{2209}', "&notin;"),
    ('\u{220b}', "&ni;"),
    ('\u{220f}', "&prod;"),
    ('\u{2211}', "&sum;"),
    ('\u{2212}', "&minus;"),
    ('\u{2217}', "&lowast;"),
    ('\u{221a}', "&radic;"),
    ('\u{221d}', "&prop;"),
    ('\u{221e}', "&infin;"),
    ('\u{2220}', "&ang;"),
    ('\u{2227}', "&and;"),
    ('\u{2228}', "&or;"),
    ('\u{2229}', "&cap;"),
    ('\u{222a}', "&cup;"),
    ('\u{222b}', "&int;"),
    ('\u{2234}', "&there4;"),
    ('\u{223c}', "&sim;"),
    ('\u{2245}', "&cong;"),
    ('\u{2248}', "&asymp;"),
    ('\u{2260}', "&ne;"),
    ('\u{2261}', "&equiv;"),
    ('\u{2264}', "&le;"),
    ('\u{2265}', "&ge;"),
    ('\u{2282}', "&sub;"),
    ('\u{2283}', "&sup;"),
    ('\u{2284}', "&nsub;"),
    ('\u{2286}', "&sube;"),
    ('\u{2287}', "&supe;"),
    ('\u{2295}', "&oplus;"),
    ('\u{2297}', "&otimes;"),
    ('\u{22a5}', "&perp;"),
    ('\u{22c5}', "&sdot;"),
    ('\u{2308}', "&lceil;"),
    ('\u{2309}', "&rceil;"),
    ('\u{230a}', "&lfloor;"),
    ('\u{230b}', "&rfloor;"),
    ('\u{2329}', "&lang;"),
    ('\u{232a}', "&rang;"),
    ('\u{25ca}', "&loz;"),
    ('\u{2660}', "&spades;"),
    ('\u{2663}', "&clubs;"),
    ('\u{2665}', "&hearts;"),
    ('\u{2666}', "&diams;"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_strings() {
        for value in ["1", " 1", "1 ", "-1", "+1.5", ".5", "5.", "1e3", "1E-3"] {
            assert!(numeric(value).is_some(), "{value}");
        }
        for value in ["", " ", "abc", "1a", "1e", "--1", ".", "0x1A", "1 2"] {
            assert!(numeric(value).is_none(), "{value}");
        }
    }

    #[test]
    fn loose_equality_of_php_8() {
        assert!(loose_eq("1", "01"));
        assert!(loose_eq("1", "1.0"));
        assert!(loose_eq("abc", "abc"));
        assert!(!loose_eq("", "0"));
        assert!(!loose_eq("abc", "ABC"));
        assert!(!loose_eq("1", "1a"));
    }

    #[test]
    fn emptiness_and_search() {
        assert!(is_empty("") && is_empty("0"));
        assert!(!is_empty("00") && !is_empty(" "));
        assert_eq!(trim(" \tx\n"), "x");
        assert!(contains_ci("Bureau Siège", "SIÈGE", false));
        assert!(contains_ci("PC-01", "pc", true));
        assert!(!contains_ci("PC-01", "01", true));
    }

    #[test]
    fn php_htmlentities() {
        assert_eq!(
            htmlentities("é<'\"&amp;€ ok"),
            "&eacute;&lt;&#039;&quot;&amp;amp;&euro; ok"
        );
    }
}
