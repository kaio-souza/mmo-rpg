use crate::locale::{decimal_separator, group_separator, min_grouping_digits};
use crate::message::Part;
use crate::plural::category_for_fixed;
use crate::{Bundle, Locale, plural_category};
use base::{Arg, Fixed, TextRef};

/// Nested `Arg::Ref` deeper than this is cut (guards against cycles).
const MAX_DEPTH: u8 = 8;

/// Formats a text reference in the bundle's locale. Never fails: a missing
/// text shows `[id]`, a missing argument shows `{?}`.
pub fn format(bundle: &Bundle, r: &TextRef) -> String {
    let mut out = String::new();
    write_ref(bundle, r, &mut out, 0);
    out
}

fn write_ref(b: &Bundle, r: &TextRef, out: &mut String, depth: u8) {
    if depth > MAX_DEPTH {
        out.push('…');
        return;
    }
    match b.get(r.id) {
        Some(m) => write_parts(b, &m.parts, &r.args, None, out, depth),
        None => {
            out.push('[');
            out.push_str(&r.id.get().to_string());
            out.push(']');
        }
    }
}

fn write_parts(
    b: &Bundle,
    parts: &[Part<u8>],
    args: &[Arg],
    hash: Option<&Arg>,
    out: &mut String,
    depth: u8,
) {
    for part in parts {
        match part {
            Part::Lit(s) => out.push_str(s),
            Part::Arg(i) | Part::Number(i) => write_arg(b, args.get(usize::from(*i)), out, depth),
            Part::Hash => write_arg(b, hash, out, depth),
            Part::Plural { arg, exact, cases } => {
                let a = args.get(usize::from(*arg));
                let (exact_n, category) = match a {
                    Some(Arg::Int(n)) => (Some(*n), plural_category(b.locale, *n)),
                    Some(Arg::Fixed(x)) => {
                        let whole = x.millis() % Fixed::SCALE == 0;
                        (
                            whole.then(|| i64::from(x.trunc())),
                            category_for_fixed(b.locale, *x),
                        )
                    }
                    _ => (None, crate::PluralCategory::Other),
                };
                let body = exact_n
                    .and_then(|n| exact.iter().find(|(e, _)| *e == n))
                    .map(|(_, p)| p)
                    .or_else(|| cases.iter().find(|(c, _)| *c == category).map(|(_, p)| p))
                    .or_else(|| {
                        cases
                            .iter()
                            .find(|(c, _)| *c == crate::PluralCategory::Other)
                            .map(|(_, p)| p)
                    });
                if let Some(body) = body {
                    write_parts(b, body, args, a, out, depth);
                }
            }
            Part::Select { arg, cases } => {
                let value = match args.get(usize::from(*arg)) {
                    Some(Arg::Str(s) | Arg::Resolved(s)) => s.clone(),
                    Some(Arg::Int(n)) => n.to_string(),
                    _ => String::new(),
                };
                let body = cases
                    .iter()
                    .find(|(k, _)| *k == value)
                    .or_else(|| cases.iter().find(|(k, _)| k == "other"));
                if let Some((_, body)) = body {
                    write_parts(b, body, args, hash, out, depth);
                }
            }
        }
    }
}

fn write_arg(b: &Bundle, arg: Option<&Arg>, out: &mut String, depth: u8) {
    match arg {
        Some(Arg::Str(s) | Arg::Resolved(s)) => out.push_str(s),
        Some(Arg::Int(n)) => out.push_str(&format_int(b.locale, *n)),
        Some(Arg::Fixed(x)) => out.push_str(&format_fixed(b.locale, *x)),
        Some(Arg::Ref(r)) => write_ref(b, r, out, depth + 1),
        None => out.push_str("{?}"),
    }
}

/// Integer with the locale's digit grouping: `1,234` (en), `1.234` (de), `1 234` (ru).
pub fn format_int(locale: Locale, n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + 4);
    if n < 0 {
        out.push('-');
    }
    group_into(locale, &digits, &mut out);
    out
}

/// Fixed point number with the locale's separators: `1,234.5` (en), `1.234,5` (de).
pub fn format_fixed(locale: Locale, x: Fixed) -> String {
    let text = x.to_string();
    let (sign, body) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text.as_str()),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let mut out = String::from(sign);
    group_into(locale, int, &mut out);
    if !frac.is_empty() {
        out.push(decimal_separator(locale));
        out.push_str(frac);
    }
    out
}

fn group_into(locale: Locale, digits: &str, out: &mut String) {
    let len = digits.len();
    if len < 4 + min_grouping_digits(locale) - 1 {
        out.push_str(digits);
        return;
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(group_separator(locale));
        }
        out.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyTable;

    fn setup(locale: Locale, text: &str) -> (KeyTable, Bundle) {
        let en = r#"{
            "loot.coins": "{count, plural, =0 {no coins} one {# coin} other {# coins}}",
            "system.raised": "{name} rose to {value}.",
            "system.wrap": "<{inner}>",
            "npc.greet": "{g, select, f {Hello madam} other {Hello sir}}",
        }"#;
        let keys = KeyTable::from_source(&[("en.ron", en)]).unwrap();
        let b = Bundle::from_text(locale, &keys, &[("x.ron", text)]).unwrap();
        (keys, b)
    }

    fn r(keys: &KeyTable, key: &str) -> TextRef {
        TextRef::new(keys.id(key).unwrap())
    }

    #[test]
    fn russian_plural_and_numbers() {
        let (k, b) = setup(
            Locale::from_static("ru"),
            r#"{
                "loot.coins": "{count, plural, =0 {нет монет} one {# монета} few {# монеты} many {# монет} other {# монеты}}",
                "system.raised": "{name}: {value}.",
                "system.wrap": "«{inner}»",
                "npc.greet": "{g, select, other {Здравствуйте}}",
            }"#,
        );
        let coins = |n| format(&b, &r(&k, "loot.coins").arg(Arg::Int(n)));
        assert_eq!(coins(0), "нет монет");
        assert_eq!(coins(1), "1 монета");
        assert_eq!(coins(3), "3 монеты");
        assert_eq!(coins(11), "11 монет");
        assert_eq!(coins(21), "21 монета");
        assert_eq!(coins(12345), "12\u{a0}345 монет");
    }

    #[test]
    fn args_follow_en_order_even_if_translation_reorders() {
        let (k, b) = setup(
            Locale::from_static("ja"),
            r#"{
                "loot.coins": "{count, plural, other {#枚}}",
                "system.raised": "{value}に上がった：{name}",
                "system.wrap": "「{inner}」",
                "npc.greet": "{g, select, other {こんにちは}}",
            }"#,
        );
        let t = r(&k, "system.raised")
            .arg(Arg::Str("Kai".into()))
            .arg(Arg::Int(5));
        assert_eq!(format(&b, &t), "5に上がった：Kai");
        let nested = r(&k, "system.wrap").arg(Arg::Ref(Box::new(t)));
        assert_eq!(format(&b, &nested), "「5に上がった：Kai」");
    }

    #[test]
    fn select_and_missing() {
        let (k, b) = setup(
            Locale::EN,
            r#"{
                "loot.coins": "{count, plural, =0 {no coins} one {# coin} other {# coins}}",
                "system.raised": "{name} rose to {value}.",
                "system.wrap": "<{inner}>",
                "npc.greet": "{g, select, f {Hello madam} other {Hello sir}}",
            }"#,
        );
        assert_eq!(
            format(&b, &r(&k, "npc.greet").arg(Arg::Str("f".into()))),
            "Hello madam"
        );
        assert_eq!(format(&b, &r(&k, "npc.greet")), "Hello sir");
        assert_eq!(format(&b, &r(&k, "system.raised")), "{?} rose to {?}.");
        assert_eq!(format(&b, &TextRef::new(base::TextId::new(99))), "[99]");
        let x: Fixed = "1.5".parse().unwrap();
        assert_eq!(format(&b, &r(&k, "loot.coins").arg(Arg::Fixed(x))), "1.5 coins");
    }

    #[test]
    fn numbers() {
        assert_eq!(format_int(Locale::EN, 1234567), "1,234,567");
        assert_eq!(format_int(Locale::from_static("de"), -1234), "-1.234");
        assert_eq!(format_int(Locale::from_static("es"), 1234), "1234");
        assert_eq!(format_int(Locale::from_static("es"), 12345), "12.345");
        assert_eq!(format_int(Locale::EN, 999), "999");
        assert_eq!(format_int(Locale::EN, i64::MIN), "-9,223,372,036,854,775,808");
        let x: Fixed = "-1234.5".parse().unwrap();
        assert_eq!(format_fixed(Locale::EN, x), "-1,234.5");
        assert_eq!(format_fixed(Locale::from_static("pt-BR"), x), "-1.234,5");
    }
}
