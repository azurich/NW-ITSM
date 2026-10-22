//! `src/i18n/catalog.tsv` holds the translations of the texts of the API,
//! extracted from the gettext files of the application (`locales/*.mo`) as
//! Laminas and the GLPI functions (`__`, `_n`, `_x`) resolve them. It must
//! match those files: regenerate it with
//! `UPDATE_TRANSLATIONS=1 cargo test -p nw-domain --test translations`.

use std::{collections::HashMap, fs, path::Path};

use nw_domain::i18n::{Source, escape, text};

#[test]
fn catalog_matches_the_gettext_files() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let locales = manifest.join("../../../locales");
    let mut files: Vec<_> = fs::read_dir(&locales)
        .expect("locales directory of the application")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "mo"))
        .collect();
    files.sort();

    let mut catalog = String::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let domain = TextDomain::load(&fs::read(&path).unwrap());
        for source in text::ALL {
            let value = domain.resolve(source);
            catalog.push_str(&format!(
                "{name}\t{}\t{}\n",
                escape(source.id()),
                escape(&value)
            ));
        }
    }

    let target = manifest.join("src/i18n/catalog.tsv");
    if std::env::var_os("UPDATE_TRANSLATIONS").is_some() {
        fs::write(&target, &catalog).unwrap();
        return;
    }
    let committed = fs::read_to_string(&target).unwrap_or_default();
    assert!(
        committed == catalog,
        "src/i18n/catalog.tsv is outdated: run `UPDATE_TRANSLATIONS=1 cargo test -p nw-domain --test translations`"
    );
}

/// An entry of a gettext file as Laminas keeps it.
#[derive(Debug, Clone)]
enum Entry {
    Text(String),
    Plural(Vec<String>),
}

/// `Laminas\I18n\Translator\TextDomain` loaded by its `Gettext` loader.
struct TextDomain {
    entries: HashMap<String, Entry>,
    plural: Expr,
}

impl TextDomain {
    fn load(bytes: &[u8]) -> Self {
        let little = match &bytes[..4] {
            [0xde, 0x12, 0x04, 0x95] => true,
            [0x95, 0x04, 0x12, 0xde] => false,
            _ => panic!("not a gettext file"),
        };
        let int = |at: usize| {
            let raw = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
            (if little {
                u32::from_le_bytes(raw)
            } else {
                u32::from_be_bytes(raw)
            }) as usize
        };
        let count = int(8);
        let (originals, translations) = (int(12), int(16));
        let string = |table: usize, index: usize| {
            let (length, offset) = (int(table + index * 8), int(table + index * 8 + 4));
            String::from_utf8_lossy(&bytes[offset..offset + length]).into_owned()
        };

        let mut entries = HashMap::new();
        for index in 0..count {
            let original = string(originals, index);
            let translation = string(translations, index);
            // Empty translations are not loaded.
            if translation.is_empty() {
                continue;
            }
            let original: Vec<&str> = original.split('\0').collect();
            let translation: Vec<&str> = translation.split('\0').collect();
            if original.len() > 1 && translation.len() > 1 {
                entries.insert(
                    original[0].to_owned(),
                    Entry::Plural(translation.iter().map(|text| (*text).to_owned()).collect()),
                );
                for other in &original[1..] {
                    entries
                        .entry((*other).to_owned())
                        .or_insert(Entry::Text(String::new()));
                }
            } else {
                entries.insert(
                    original[0].to_owned(),
                    Entry::Text(translation[0].to_owned()),
                );
            }
        }

        // Header: `Plural-Forms: nplurals=2; plural=(n > 1);`.
        let mut plural = Expr::parse("n != 1");
        if let Some(Entry::Text(header)) = entries.remove("") {
            for line in header.trim().lines() {
                if let Some((name, content)) = line.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("plural-forms") {
                        let rule = content
                            .split(';')
                            .find_map(|part| part.trim().strip_prefix("plural="))
                            .expect("plural rule");
                        plural = Expr::parse(rule);
                    }
                }
            }
        }
        Self { entries, plural }
    }

    /// `Translator::translate` through `__()`: an array gives its first form.
    fn translate(&self, message: &str) -> String {
        match self.entries.get(message) {
            Some(Entry::Text(text)) if !text.is_empty() => text.clone(),
            Some(Entry::Plural(forms)) => forms[0].clone(),
            _ => message.to_owned(),
        }
    }

    fn resolve(&self, source: Source) -> String {
        match source {
            Source::Text(text) => self.translate(text),
            // `_x`: the context joined by EOT; untranslated gives the text.
            Source::Context(context, text) => {
                let message = format!("{context}\u{4}{text}");
                let translated = self.translate(&message);
                if translated == message {
                    text.to_owned()
                } else {
                    translated
                }
            }
            // `Translator::translatePlural` for one item.
            Source::One(singular, plural) => {
                let forms = match self.entries.get(singular) {
                    Some(Entry::Text(text)) => vec![text.clone()],
                    Some(Entry::Plural(forms)) => forms.clone(),
                    None => Vec::new(),
                };
                let index = self.plural.eval(1);
                match forms.get(index as usize) {
                    Some(form) if !form.is_empty() => form.clone(),
                    _ if index == 0 => singular.to_owned(),
                    _ => plural.to_owned(),
                }
            }
        }
    }
}

/// A plural rule (`Laminas\I18n\Translator\Plural\Rule`): C expressions on
/// `n`.
#[derive(Debug, Clone)]
enum Expr {
    N,
    Number(i64),
    Not(Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn parse(source: &str) -> Self {
        let tokens = tokenize(source);
        let mut parser = Parser { tokens, at: 0 };
        let expr = parser.ternary();
        assert_eq!(parser.at, parser.tokens.len(), "plural rule {source}");
        expr
    }

    fn eval(&self, n: i64) -> i64 {
        match self {
            Self::N => n,
            Self::Number(value) => *value,
            Self::Not(inner) => i64::from(inner.eval(n) == 0),
            Self::Ternary(condition, yes, no) => {
                if condition.eval(n) != 0 {
                    yes.eval(n)
                } else {
                    no.eval(n)
                }
            }
            Self::Binary(op, left, right) => {
                let (a, b) = (left.eval(n), right.eval(n));
                match op.as_str() {
                    "||" => i64::from(a != 0 || b != 0),
                    "&&" => i64::from(a != 0 && b != 0),
                    "==" => i64::from(a == b),
                    "!=" => i64::from(a != b),
                    "<" => i64::from(a < b),
                    "<=" => i64::from(a <= b),
                    ">" => i64::from(a > b),
                    ">=" => i64::from(a >= b),
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => a / b,
                    "%" => a % b,
                    other => panic!("operator {other}"),
                }
            }
        }
    }
}

fn tokenize(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        } else {
            let pair: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            if ["||", "&&", "==", "!=", "<=", ">="].contains(&pair.as_str()) {
                tokens.push(pair);
                i += 2;
            } else {
                tokens.push(c.to_string());
                i += 1;
            }
        }
    }
    tokens
}

struct Parser {
    tokens: Vec<String>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn next(&mut self) -> String {
        self.at += 1;
        self.tokens[self.at - 1].clone()
    }

    fn ternary(&mut self) -> Expr {
        let condition = self.binary(0);
        if self.peek() == Some("?") {
            self.next();
            let yes = self.ternary();
            assert_eq!(self.next(), ":");
            let no = self.ternary();
            return Expr::Ternary(Box::new(condition), Box::new(yes), Box::new(no));
        }
        condition
    }

    /// Binary operators by increasing precedence.
    fn binary(&mut self, level: usize) -> Expr {
        const LEVELS: [&[&str]; 6] = [
            &["||"],
            &["&&"],
            &["==", "!="],
            &["<", "<=", ">", ">="],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1);
        while let Some(op) = self
            .peek()
            .filter(|op| LEVELS[level].contains(op))
            .map(str::to_owned)
        {
            self.next();
            let right = self.binary(level + 1);
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        left
    }

    fn unary(&mut self) -> Expr {
        match self.next().as_str() {
            "!" => Expr::Not(Box::new(self.unary())),
            "(" => {
                let inner = self.ternary();
                assert_eq!(self.next(), ")");
                inner
            }
            "n" => Expr::N,
            number => Expr::Number(
                number
                    .parse()
                    .unwrap_or_else(|_| panic!("plural rule token {number}")),
            ),
        }
    }
}

#[test]
fn plural_rules() {
    assert_eq!(Expr::parse("(n > 1)").eval(1), 0);
    let arabic = Expr::parse(
        "(n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 && n%100<=99 ? 4 : 5)",
    );
    assert_eq!(
        (
            arabic.eval(0),
            arabic.eval(1),
            arabic.eval(5),
            arabic.eval(50),
            arabic.eval(102)
        ),
        (0, 1, 3, 4, 5)
    );
    let russian = Expr::parse(
        "(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2)",
    );
    assert_eq!(
        (russian.eval(1), russian.eval(3), russian.eval(11)),
        (0, 1, 2)
    );
}
