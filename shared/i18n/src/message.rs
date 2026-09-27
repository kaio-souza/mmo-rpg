//! Small ICU MessageFormat subset: `{name}`, `{n, number}`,
//! `{n, plural, =0 {...} one {...} other {...}}` (with `#`),
//! `{x, select, a {...} other {...}}`.
//!
//! Quoting follows ICU: `''` is one apostrophe; an apostrophe right before
//! `{`, `}` (or `#` inside a plural) starts a literal run up to the next
//! single apostrophe. Any other apostrophe is literal (`l'arma` just works).

use crate::PluralCategory;
use std::fmt;

/// Parsed message, with placeholders resolved to argument positions.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub(crate) parts: Vec<Part<u8>>,
}

/// Placeholder `A` is a name before resolution and a position after.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Part<A> {
    Lit(String),
    Arg(A),
    Number(A),
    Plural {
        arg: A,
        exact: Vec<(i64, Vec<Part<A>>)>,
        cases: Vec<(PluralCategory, Vec<Part<A>>)>,
    },
    /// Always has an `other` case.
    Select {
        arg: A,
        cases: Vec<(String, Vec<Part<A>>)>,
    },
    /// `#` inside a plural: the plural number.
    Hash,
}

/// Plural/select cases: key and body.
type Cases<K, A> = Vec<(K, Vec<Part<A>>)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawMessage {
    pub(crate) parts: Vec<Part<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageError {
    UnclosedBrace,
    UnexpectedCloseBrace { at: usize },
    UnterminatedQuote { at: usize },
    BadName { at: usize },
    Expected { what: &'static str, at: usize },
    UnknownType(String),
    UnknownCategory(String),
    DuplicateCase(String),
    MissingOther,
    UnknownPlaceholder(String),
    TooManyPlaceholders,
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedBrace => f.write_str("unclosed `{`"),
            Self::UnexpectedCloseBrace { at } => write!(f, "unexpected `}}` at {at}"),
            Self::UnterminatedQuote { at } => write!(f, "unterminated quote starting at {at}"),
            Self::BadName { at } => write!(f, "expected placeholder name at {at}"),
            Self::Expected { what, at } => write!(f, "expected {what} at {at}"),
            Self::UnknownType(t) => write!(f, "unknown placeholder type `{t}`"),
            Self::UnknownCategory(c) => write!(f, "unknown plural category `{c}`"),
            Self::DuplicateCase(c) => write!(f, "case `{c}` appears twice"),
            Self::MissingOther => f.write_str("plural/select without `other`"),
            Self::UnknownPlaceholder(n) => write!(f, "placeholder `{n}` does not exist in `en`"),
            Self::TooManyPlaceholders => f.write_str("more than 255 placeholders"),
        }
    }
}

impl std::error::Error for MessageError {}

impl Message {
    /// Parses `src`; `params` gives the position of each placeholder name.
    /// The fixed text outside placeholders, plurals and selects (P04: two
    /// languages with the same long sentence is a copy, not a translation).
    pub fn literal(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Lit(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn parse(src: &str, params: &[String]) -> Result<Message, MessageError> {
        RawMessage::parse(src)?.resolve(params)
    }
}

impl RawMessage {
    pub(crate) fn parse(src: &str) -> Result<RawMessage, MessageError> {
        let mut p = Parser {
            chars: src.chars().collect(),
            pos: 0,
        };
        let parts = p.message(false, false)?;
        Ok(RawMessage { parts })
    }

    /// Placeholder names in order of first appearance.
    pub(crate) fn names(&self) -> Vec<String> {
        let mut out = Vec::new();
        walk(&self.parts, &mut |part| {
            let name = match part {
                Part::Arg(a) | Part::Number(a) => a,
                Part::Plural { arg, .. } | Part::Select { arg, .. } => arg,
                Part::Lit(_) | Part::Hash => return,
            };
            if !out.contains(name) {
                out.push(name.clone());
            }
        });
        out
    }

    /// Each plural block: argument name and the categories it covers.
    pub(crate) fn plurals(&self) -> Vec<(String, Vec<PluralCategory>)> {
        let mut out = Vec::new();
        walk(&self.parts, &mut |part| {
            if let Part::Plural { arg, cases, .. } = part {
                out.push((arg.clone(), cases.iter().map(|(c, _)| *c).collect()));
            }
        });
        out
    }

    /// Visible length: literal characters of the longest plural/select branch.
    /// Placeholders count as zero (their value is not known here); `#` as one.
    pub(crate) fn visible_chars(&self) -> usize {
        fn len(parts: &[Part<String>]) -> usize {
            parts
                .iter()
                .map(|p| match p {
                    Part::Lit(s) => s.chars().count(),
                    Part::Hash => 1,
                    Part::Arg(_) | Part::Number(_) => 0,
                    Part::Plural { exact, cases, .. } => exact
                        .iter()
                        .map(|(_, b)| len(b))
                        .chain(cases.iter().map(|(_, b)| len(b)))
                        .max()
                        .unwrap_or(0),
                    Part::Select { cases, .. } => cases.iter().map(|(_, b)| len(b)).max().unwrap_or(0),
                })
                .sum()
        }
        len(&self.parts)
    }

    pub(crate) fn resolve(self, params: &[String]) -> Result<Message, MessageError> {
        Ok(Message {
            parts: resolve_parts(self.parts, params)?,
        })
    }
}

fn walk<A>(parts: &[Part<A>], f: &mut impl FnMut(&Part<A>)) {
    for part in parts {
        f(part);
        match part {
            Part::Plural { exact, cases, .. } => {
                exact.iter().for_each(|(_, p)| walk(p, f));
                cases.iter().for_each(|(_, p)| walk(p, f));
            }
            Part::Select { cases, .. } => cases.iter().for_each(|(_, p)| walk(p, f)),
            _ => {}
        }
    }
}

fn resolve_parts(parts: Vec<Part<String>>, params: &[String]) -> Result<Vec<Part<u8>>, MessageError> {
    let pos = |name: String| -> Result<u8, MessageError> {
        let i = params
            .iter()
            .position(|p| *p == name)
            .ok_or(MessageError::UnknownPlaceholder(name))?;
        u8::try_from(i).map_err(|_| MessageError::TooManyPlaceholders)
    };
    fn sub<K>(cases: Cases<K, String>, params: &[String]) -> Result<Cases<K, u8>, MessageError> {
        cases
            .into_iter()
            .map(|(k, p)| Ok((k, resolve_parts(p, params)?)))
            .collect()
    }
    parts
        .into_iter()
        .map(|part| {
            Ok(match part {
                Part::Lit(s) => Part::Lit(s),
                Part::Hash => Part::Hash,
                Part::Arg(a) => Part::Arg(pos(a)?),
                Part::Number(a) => Part::Number(pos(a)?),
                Part::Plural { arg, exact, cases } => Part::Plural {
                    arg: pos(arg)?,
                    exact: sub(exact, params)?,
                    cases: sub(cases, params)?,
                },
                Part::Select { arg, cases } => Part::Select {
                    arg: pos(arg)?,
                    cases: sub(cases, params)?,
                },
            })
        })
        .collect()
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, c: char, what: &'static str) -> Result<(), MessageError> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(MessageError::Expected { what, at: self.pos })
        }
    }

    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek().filter(|c| c.is_ascii_alphanumeric() || *c == '_') {
            s.push(c);
            self.pos += 1;
        }
        s
    }

    /// `nested`: inside a plural/select case, ends at `}` (not consumed).
    fn message(&mut self, in_plural: bool, nested: bool) -> Result<Vec<Part<String>>, MessageError> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let flush = |lit: &mut String, parts: &mut Vec<Part<String>>| {
            if !lit.is_empty() {
                parts.push(Part::Lit(std::mem::take(lit)));
            }
        };
        loop {
            match self.peek() {
                None if nested => return Err(MessageError::UnclosedBrace),
                None => break,
                Some('}') if nested => break,
                Some('}') => return Err(MessageError::UnexpectedCloseBrace { at: self.pos }),
                Some('{') => {
                    flush(&mut lit, &mut parts);
                    self.pos += 1;
                    parts.push(self.argument(in_plural)?);
                }
                Some('#') if in_plural => {
                    flush(&mut lit, &mut parts);
                    self.pos += 1;
                    parts.push(Part::Hash);
                }
                Some('\'') => {
                    let start = self.pos;
                    self.pos += 1;
                    match self.peek() {
                        Some('\'') => {
                            self.pos += 1;
                            lit.push('\'');
                        }
                        Some(c) if c == '{' || c == '}' || (c == '#' && in_plural) => {
                            self.quoted(&mut lit, start)?;
                        }
                        _ => lit.push('\''),
                    }
                }
                Some(c) => {
                    self.pos += 1;
                    lit.push(c);
                }
            }
        }
        flush(&mut lit, &mut parts);
        Ok(parts)
    }

    fn quoted(&mut self, lit: &mut String, start: usize) -> Result<(), MessageError> {
        loop {
            match self.bump() {
                None => return Err(MessageError::UnterminatedQuote { at: start }),
                Some('\'') if self.peek() == Some('\'') => {
                    self.pos += 1;
                    lit.push('\'');
                }
                Some('\'') => return Ok(()),
                Some(c) => lit.push(c),
            }
        }
    }

    /// After `{`.
    fn argument(&mut self, in_plural: bool) -> Result<Part<String>, MessageError> {
        self.skip_ws();
        let at = self.pos;
        let name = self.ident();
        if name.is_empty() {
            return Err(MessageError::BadName { at });
        }
        self.skip_ws();
        match self.bump() {
            Some('}') => return Ok(Part::Arg(name)),
            Some(',') => {}
            None => return Err(MessageError::UnclosedBrace),
            Some(_) => {
                return Err(MessageError::Expected {
                    what: "`,` or `}`",
                    at: self.pos - 1,
                });
            }
        }
        self.skip_ws();
        let ty = self.ident();
        self.skip_ws();
        match ty.as_str() {
            "number" => {
                self.expect('}', "`}`")?;
                Ok(Part::Number(name))
            }
            "plural" => {
                self.expect(',', "`,`")?;
                let mut exact = Vec::new();
                let mut cases = Vec::new();
                for (key, body) in self.cases(true)? {
                    if let Some(n) = key.strip_prefix('=') {
                        let n = n
                            .parse()
                            .map_err(|_| MessageError::UnknownCategory(key.clone()))?;
                        exact.push((n, body));
                    } else {
                        let c = PluralCategory::from_name(&key)
                            .ok_or_else(|| MessageError::UnknownCategory(key.clone()))?;
                        cases.push((c, body));
                    }
                }
                if !cases.iter().any(|(c, _)| *c == PluralCategory::Other) {
                    return Err(MessageError::MissingOther);
                }
                Ok(Part::Plural {
                    arg: name,
                    exact,
                    cases,
                })
            }
            "select" => {
                self.expect(',', "`,`")?;
                let cases = self.cases(in_plural)?;
                if !cases.iter().any(|(k, _)| k == "other") {
                    return Err(MessageError::MissingOther);
                }
                Ok(Part::Select { arg: name, cases })
            }
            _ => Err(MessageError::UnknownType(ty)),
        }
    }

    /// `key {body} key {body} ... }` (consumes the final `}`).
    fn cases(&mut self, in_plural: bool) -> Result<Cases<String, String>, MessageError> {
        let mut out: Cases<String, String> = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err(MessageError::UnclosedBrace),
                Some('}') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {}
            }
            let at = self.pos;
            let mut key = String::new();
            if self.peek() == Some('=') {
                self.pos += 1;
                key.push('=');
                if self.peek() == Some('-') {
                    self.pos += 1;
                    key.push('-');
                }
            }
            key.push_str(&self.ident());
            if key.is_empty() || key == "=" || key == "=-" {
                return Err(MessageError::Expected {
                    what: "case name",
                    at,
                });
            }
            if out.iter().any(|(k, _)| *k == key) {
                return Err(MessageError::DuplicateCase(key));
            }
            self.skip_ws();
            self.expect('{', "`{`")?;
            let body = self.message(in_plural, true)?;
            self.expect('}', "`}`")?;
            out.push((key, body));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(s: &str) -> RawMessage {
        RawMessage::parse(s).unwrap()
    }

    #[test]
    fn names_in_order_of_first_appearance() {
        let m = raw("{a} {b, number} {c, plural, one {# {a}} other {{d}}} {e, select, x {} other {}}");
        assert_eq!(m.names(), ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn quoting() {
        let lit = |s: &str| match raw(s).parts.as_slice() {
            [Part::Lit(l)] => l.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(lit("l'arma"), "l'arma");
        assert_eq!(lit("it''s"), "it's");
        assert_eq!(lit("'{literal}'"), "{literal}");
        assert_eq!(lit("'{it''s}'"), "{it's}");
        assert_eq!(lit("a # b"), "a # b");
    }

    #[test]
    fn errors() {
        use MessageError::*;
        let err = |s: &str| RawMessage::parse(s).unwrap_err();
        assert_eq!(err("{a"), UnclosedBrace);
        assert_eq!(err("a}"), UnexpectedCloseBrace { at: 1 });
        assert_eq!(err("{}"), BadName { at: 1 });
        assert_eq!(err("{a, date}"), UnknownType("date".into()));
        assert_eq!(err("{n, plural, one {x}}"), MissingOther);
        assert_eq!(
            err("{n, plural, lots {x} other {y}}"),
            UnknownCategory("lots".into())
        );
        assert_eq!(
            err("{x, select, a {1} other {2} a {3}}"),
            DuplicateCase("a".into())
        );
        assert_eq!(err("{x, select, a {1}}"), MissingOther);
        assert_eq!(err("'{open"), UnterminatedQuote { at: 0 });
    }

    #[test]
    fn plural_parts() {
        let m = raw("{n, plural, =0 {none} one {# coin} other {# coins}}");
        assert_eq!(
            m.plurals(),
            [("n".into(), vec![PluralCategory::One, PluralCategory::Other])]
        );
    }

    #[test]
    fn visible_chars() {
        assert_eq!(
            raw("{n, plural, one {# coin} other {# coins}}").visible_chars(),
            7
        );
        assert_eq!(raw("{name} rose to {v}.").visible_chars(), 10);
    }

    #[test]
    fn resolve_unknown_placeholder() {
        let params = vec!["a".to_owned()];
        assert!(Message::parse("{a}", &params).is_ok());
        assert_eq!(
            Message::parse("{b}", &params).unwrap_err(),
            MessageError::UnknownPlaceholder("b".into())
        );
    }
}
