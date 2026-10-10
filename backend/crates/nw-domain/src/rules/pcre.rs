//! PHP `preg_*` patterns (PCRE) run with the `regex` crate.
//!
//! A pattern is `<delimiter><body><delimiter><modifiers>`. The body is
//! translated to the `regex` syntax where both engines agree; anything else
//! (backreferences, lookarounds, `\Q`, possessive quantifiers...) is
//! reported as unsupported rather than matched differently.

use regex::bytes::{Captures, Regex, RegexBuilder};
use regex_automata::{Anchored, Input, MatchKind, meta, util::syntax};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// PHP itself rejects the pattern: `preg_match` returns `false`.
    Invalid,
    /// PHP accepts the pattern but it cannot be run identically here.
    Unsupported(String),
}

fn unsupported(what: impl Into<String>) -> Error {
    Error::Unsupported(what.into())
}

#[derive(Debug, Clone)]
pub struct Pattern {
    regex: Regex,
    /// Same pattern reporting the longest match: tells whether PCRE would
    /// find a non-empty match where `regex` prefers an empty one.
    longest: meta::Regex,
    /// `$` outside multiline mode: PCRE also matches it before a final
    /// newline, which `regex` does not.
    dollar_before_newline: bool,
    utf8: bool,
}

impl Pattern {
    /// Compiles a PHP pattern, like `preg_match` does.
    pub fn compile(pattern: &str) -> Result<Self, Error> {
        let (body, modifiers) = split(pattern)?;
        let mut flags = Flags::default();
        for modifier in modifiers.chars() {
            match modifier {
                'i' => flags.case_insensitive = true,
                'm' => flags.multi_line = true,
                's' => flags.dot_all = true,
                'U' => flags.ungreedy = true,
                'u' => flags.utf8 = true,
                'D' => flags.dollar_end_only = true,
                // No effect on the result.
                'S' | 'X' | ' ' | '\n' | '\r' => {}
                'x' | 'A' | 'J' | 'n' => return Err(unsupported(format!("modifier {modifier}"))),
                _ => return Err(Error::Invalid),
            }
        }
        let (translated, has_dollar) = translate(body)?;
        let unsupported_pattern =
            |error: String| unsupported(format!("pattern {pattern}: {error}"));
        let regex = RegexBuilder::new(&translated)
            .case_insensitive(flags.case_insensitive)
            .multi_line(flags.multi_line)
            .dot_matches_new_line(flags.dot_all)
            .swap_greed(flags.ungreedy)
            .unicode(flags.utf8)
            .build()
            .map_err(|error| unsupported_pattern(error.to_string()))?;
        let longest = meta::Regex::builder()
            .configure(
                meta::Regex::config()
                    .match_kind(MatchKind::All)
                    .utf8_empty(false),
            )
            .syntax(
                syntax::Config::new()
                    .case_insensitive(flags.case_insensitive)
                    .multi_line(flags.multi_line)
                    .dot_matches_new_line(flags.dot_all)
                    .swap_greed(flags.ungreedy)
                    .unicode(flags.utf8)
                    .utf8(false),
            )
            .build(&translated)
            .map_err(|error| unsupported_pattern(error.to_string()))?;
        Ok(Self {
            regex,
            longest,
            dollar_before_newline: has_dollar && !flags.multi_line && !flags.dollar_end_only,
            utf8: flags.utf8,
        })
    }

    fn check_subject(&self, subject: &str) -> Result<(), Error> {
        if self.dollar_before_newline && subject.ends_with('\n') {
            return Err(unsupported("`$` on a value ending with a newline"));
        }
        Ok(())
    }

    /// `preg_match($pattern, $subject) > 0`.
    pub fn is_match(&self, subject: &str) -> Result<bool, Error> {
        self.check_subject(subject)?;
        Ok(self.regex.is_match(subject.as_bytes()))
    }

    /// The captured groups of every match, like `preg_match_all` once the
    /// whole matches are dropped: the values of group 1 for each match,
    /// then those of group 2... `None` when nothing matches.
    pub fn captures_all(&self, subject: &str) -> Result<Option<Vec<String>>, Error> {
        self.check_subject(subject)?;
        let matches = self.all_matches(subject)?;
        if matches.is_empty() {
            return Ok(None);
        }
        let mut values = Vec::new();
        for group in 1..self.regex.captures_len() {
            for captures in &matches {
                let value = captures
                    .get(group)
                    .map(|found| found.as_bytes())
                    .unwrap_or_default();
                values.push(String::from_utf8_lossy(value).into_owned());
            }
        }
        Ok(Some(values))
    }
}

impl Pattern {
    /// Successive matches, found like `php_pcre_match_impl`: after an empty
    /// match, PCRE first looks for a non-empty match at the same place, then
    /// moves one character forward. Unlike `regex`, an empty match right
    /// after a non-empty one is kept (`/(.*)/` matches "jdoe" then "").
    fn all_matches<'h>(&self, subject: &'h str) -> Result<Vec<Captures<'h>>, Error> {
        let haystack = subject.as_bytes();
        let mut matches = Vec::new();
        let mut at = 0;
        let mut retry_non_empty = None;
        loop {
            if let Some(position) = retry_non_empty.take() {
                if position >= haystack.len() {
                    break;
                }
                let input = Input::new(haystack)
                    .range(position..)
                    .anchored(Anchored::Yes);
                if self
                    .longest
                    .find(input)
                    .is_some_and(|found| found.end() > position)
                {
                    // PCRE would pick one of the non-empty matches here.
                    return Err(unsupported("empty match where a longer one exists"));
                }
                at = position + self.unit_len(subject, position);
            }
            let Some(captures) = self.regex.captures_at(haystack, at) else {
                break;
            };
            let whole = captures.get(0).expect("group 0 is the whole match");
            if whole.is_empty() {
                retry_non_empty = Some(whole.end());
            } else {
                at = whole.end();
            }
            matches.push(captures);
        }
        Ok(matches)
    }

    /// Size of the character at `position`: one byte, or one UTF-8
    /// character with the `u` modifier.
    fn unit_len(&self, subject: &str, position: usize) -> usize {
        if self.utf8 {
            subject[position..].chars().next().map_or(1, char::len_utf8)
        } else {
            1
        }
    }
}

#[derive(Debug, Default)]
struct Flags {
    case_insensitive: bool,
    multi_line: bool,
    dot_all: bool,
    ungreedy: bool,
    utf8: bool,
    dollar_end_only: bool,
}

/// Body and modifiers of a pattern, as found by PHP
/// (`pcre_get_compiled_regex_cache`).
fn split(pattern: &str) -> Result<(&str, &str), Error> {
    let pattern = pattern.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\x0B');
    let mut chars = pattern.char_indices();
    let Some((_, delimiter)) = chars.next() else {
        return Err(Error::Invalid);
    };
    if delimiter.is_ascii_alphanumeric() || delimiter == '\\' || delimiter == '\0' {
        return Err(Error::Invalid);
    }
    let closing = match delimiter {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        other => other,
    };
    let start = delimiter.len_utf8();
    let mut depth = 1;
    let mut escaped = false;
    for (at, c) in chars {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == closing && closing != delimiter {
            depth -= 1;
            if depth == 0 {
                return Ok((&pattern[start..at], &pattern[at + c.len_utf8()..]));
            }
        } else if c == delimiter && closing != delimiter {
            depth += 1;
        } else if c == delimiter {
            return Ok((&pattern[start..at], &pattern[at + c.len_utf8()..]));
        }
    }
    Err(Error::Invalid)
}

/// Escapes PCRE and `regex` both understand the same way.
const SAME_ESCAPES: &str = "dDwWsSbBAzntrfapP";

/// Translates a PCRE body to the `regex` syntax. Also tells whether it
/// uses `$` outside character classes.
fn translate(body: &str) -> Result<(String, bool), Error> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len() + 8);
    let mut has_dollar = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                i += 1;
                let Some(&next) = chars.get(i) else {
                    return Err(Error::Invalid);
                };
                escape(next, false, &mut out)?;
            }
            '[' => i = class(&chars, i, &mut out)?,
            '$' => {
                has_dollar = true;
                out.push('$');
            }
            '(' if chars.get(i + 1) == Some(&'?') => i = group(&chars, i, &mut out)?,
            '*' | '+' | '?' if chars.get(i + 1) == Some(&'+') => {
                return Err(unsupported("possessive quantifier"));
            }
            '{' => match quantifier(&chars, i) {
                Some(end) if chars.get(end + 1) == Some(&'+') => {
                    return Err(unsupported("possessive quantifier"));
                }
                Some(end) => {
                    out.extend(&chars[i..=end]);
                    i = end;
                }
                // Not a quantifier: a literal brace for PCRE.
                None => out.push_str("\\{"),
            },
            '}' | ']' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    Ok((out, has_dollar))
}

/// An escaped character, outside (`in_class` false) or inside a class.
fn escape(c: char, in_class: bool, out: &mut String) -> Result<(), Error> {
    if c.is_ascii_alphanumeric() {
        if c == 'x' {
            out.push_str("\\x");
        } else if SAME_ESCAPES.contains(c) && !(in_class && matches!(c, 'b' | 'B' | 'A' | 'z')) {
            out.push('\\');
            out.push(c);
        } else {
            return Err(unsupported(format!("escape \\{c}")));
        }
    } else if c.is_ascii() {
        // Any other escaped ASCII character is itself.
        out.push_str(&format!("\\x{:02X}", c as u32));
    } else {
        out.push(c);
    }
    Ok(())
}

/// Copies a character class starting at `start` (`[`); returns the index
/// of its closing `]`.
fn class(chars: &[char], start: usize, out: &mut String) -> Result<usize, Error> {
    out.push('[');
    let mut i = start + 1;
    if chars.get(i) == Some(&'^') {
        out.push('^');
        i += 1;
    }
    // A `]` first is a literal.
    if chars.get(i) == Some(&']') {
        out.push_str("\\]");
        i += 1;
    }
    while i < chars.len() {
        let c = chars[i];
        match c {
            ']' => {
                out.push(']');
                return Ok(i);
            }
            '\\' => {
                i += 1;
                let Some(&next) = chars.get(i) else {
                    return Err(Error::Invalid);
                };
                escape(next, true, out)?;
            }
            '[' if chars.get(i + 1) == Some(&':') => {
                // POSIX class `[:alpha:]`.
                let end = (i + 2..chars.len().saturating_sub(1))
                    .find(|&at| chars[at] == ':' && chars[at + 1] == ']')
                    .ok_or_else(|| unsupported("POSIX class"))?;
                out.extend(&chars[i..=end + 1]);
                i = end + 1;
            }
            '[' | '&' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '-' if chars.get(i + 1) == Some(&'-') => return Err(unsupported("`--` in a class")),
            _ => out.push(c),
        }
        i += 1;
    }
    // Missing `]`.
    Err(Error::Invalid)
}

/// Copies a `(?...` group opening, when both engines read it the same way;
/// returns the index of its last character.
fn group(chars: &[char], start: usize, out: &mut String) -> Result<usize, Error> {
    let rest: String = chars[start + 2..].iter().take(3).collect();
    // Non-capturing group.
    if rest.starts_with(':') {
        out.push_str("(?:");
        return Ok(start + 2);
    }
    // Named group.
    for prefix in ["P<", "<"] {
        if let Some(after) = rest.strip_prefix(prefix) {
            if after.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                out.push_str("(?");
                out.push_str(prefix);
                return Ok(start + 1 + prefix.len());
            }
        }
    }
    // Inline flags: `(?i)`, `(?i-s:`.
    let mut i = start + 2;
    while let Some(&c) = chars.get(i) {
        match c {
            'i' | 'm' | 's' | 'U' | '-' => i += 1,
            ')' | ':' if i > start + 2 => {
                out.extend(&chars[start..=i]);
                return Ok(i);
            }
            _ => break,
        }
    }
    Err(unsupported("group syntax `(?`"))
}

/// End index of a `{n}`, `{n,}` or `{n,m}` quantifier starting at `start`.
fn quantifier(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let digits = |i: &mut usize| {
        let from = *i;
        while chars.get(*i).is_some_and(char::is_ascii_digit) {
            *i += 1;
        }
        *i > from
    };
    if !digits(&mut i) {
        return None;
    }
    if chars.get(i) == Some(&',') {
        i += 1;
        digits(&mut i);
    }
    (chars.get(i) == Some(&'}')).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captures(pattern: &str, subject: &str) -> Option<Vec<String>> {
        Pattern::compile(pattern)
            .unwrap()
            .captures_all(subject)
            .unwrap()
    }

    #[test]
    fn default_rules_of_a_fresh_install() {
        // "Domain user assignation", "Multiple users", "One user": #0 is
        // the first group of the first match.
        assert_eq!(captures("/(.*)@/i", "jdoe@example.org").unwrap(), ["jdoe"]);
        assert_eq!(
            captures("/(.*),/i", "jdoe,asmith,").unwrap(),
            ["jdoe,asmith"]
        );
        // `.*` also matches the empty end: PHP adds an empty value too.
        assert_eq!(captures("/(.*)/i", "jdoe").unwrap(), ["jdoe", ""]);
        assert!(captures("/(.*)@/i", "jdoe").is_none());
    }

    #[test]
    fn groups_are_listed_group_by_group() {
        assert_eq!(
            captures("/(\\d)-(\\w)/", "1-a 2-b").unwrap(),
            ["1", "2", "a", "b"]
        );
        assert_eq!(captures("/(a)|(b)/", "b").unwrap(), ["", "b"]);
    }

    #[test]
    fn delimiters_and_modifiers() {
        assert!(
            Pattern::compile("#^pc-\\d+$#i")
                .unwrap()
                .is_match("PC-12")
                .unwrap()
        );
        assert!(Pattern::compile("{^pc}").unwrap().is_match("pc1").unwrap());
        assert!(
            Pattern::compile("(^a(b)c)")
                .unwrap()
                .is_match("abc")
                .unwrap()
        );
        assert!(!Pattern::compile("/^pc/").unwrap().is_match("PC").unwrap());
        assert!(
            Pattern::compile("/^pc$/m")
                .unwrap()
                .is_match("x\npc\ny")
                .unwrap()
        );
        // PHP rejects these.
        for invalid in ["", "abc", "/abc", "/abc/k", "/abc/e", "\\abc\\", "/[a/"] {
            assert_eq!(
                Pattern::compile(invalid).unwrap_err(),
                Error::Invalid,
                "{invalid}"
            );
        }
    }

    #[test]
    fn pcre_only_syntax_is_unsupported() {
        for pattern in [
            "/(a)\\1/",
            "/a(?=b)/",
            "/(?<!a)b/",
            "/a++/",
            "/a?+/",
            "/a{2}+/",
            "/(?>a)/",
            "/\\Qa.b\\E/",
            "/a\\Z/",
            "/\\h/",
            "/\\v/",
            "/a/x",
            "/(?#comment)a/",
            "/(?x)a/",
        ] {
            assert!(
                matches!(Pattern::compile(pattern), Err(Error::Unsupported(_))),
                "{pattern}"
            );
        }
    }

    #[test]
    fn literals_are_literals() {
        let pattern = Pattern::compile("/a{b}\\<c\\>[]x[]/").unwrap();
        assert!(pattern.is_match("a{b}<c>]").unwrap());
        assert!(Pattern::compile("/[a&&b]/").unwrap().is_match("&").unwrap());
        assert!(
            Pattern::compile("/[[:digit:]]{2,}/")
                .unwrap()
                .is_match("a12")
                .unwrap()
        );
        assert!(
            Pattern::compile("/x{,2}/")
                .unwrap()
                .is_match("x{,2}")
                .unwrap()
        );
    }

    #[test]
    fn bytes_unless_utf8() {
        // Without `u`, `.` is one byte: `^.$` does not match "é".
        assert!(!Pattern::compile("/^.$/").unwrap().is_match("é").unwrap());
        assert!(Pattern::compile("/^.$/u").unwrap().is_match("é").unwrap());
        assert!(!Pattern::compile("/\\w/").unwrap().is_match("é").unwrap());
        assert!(Pattern::compile("/\\w/u").unwrap().is_match("é").unwrap());
        assert!(
            Pattern::compile("/siège/")
                .unwrap()
                .is_match("Le siège")
                .unwrap()
        );
    }

    #[test]
    fn empty_matches_like_preg_match_all() {
        // preg_match_all('/(x*)/', 'ab'): an empty match at 0, 1 and 2.
        assert_eq!(captures("/(x*)/", "ab").unwrap(), ["", "", ""]);
        assert_eq!(captures("/(\\d*)/", "a12").unwrap(), ["", "12", ""]);
        // PCRE would retry with "y" after the empty `x*`.
        assert!(matches!(
            Pattern::compile("/x*|y/").unwrap().captures_all("y"),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn dollar_before_a_final_newline() {
        let pattern = Pattern::compile("/a$/").unwrap();
        assert!(pattern.is_match("a").unwrap());
        assert!(matches!(
            pattern.is_match("a\n"),
            Err(Error::Unsupported(_))
        ));
        assert!(!Pattern::compile("/a$/D").unwrap().is_match("a\n").unwrap());
        assert!(
            Pattern::compile("/a[$]/")
                .unwrap()
                .is_match("a$\n")
                .unwrap()
        );
    }
}
