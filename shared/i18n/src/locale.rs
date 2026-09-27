//! Plural and number rules per language (CLDR), looked up by the locale
//! code: the exact code first (`pt-PT`), then its language (`pt`). Which
//! locales the game has is data (`content/text/locales.ron`); a
//! language missing here takes the English rules and `validate` warns.

use base::Locale;

/// How a language picks a plural category for an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluralRule {
    /// No plural forms (zh, ja, ko, id, vi, th).
    Other,
    /// `one` for 1 (en, de, es, it, nl, the Nordic languages...).
    One,
    /// `one` for 0 and 1 (pt, pt-BR, fr).
    ZeroOne,
    /// `one` 1, 21...; `few` 2-4, 22-24...; `many` the rest (ru, uk, be).
    EastSlavic,
    /// `one` 1; `few` 2-4, 22-24...; `many` the rest (pl).
    Polish,
    /// `one` 1; `few` 2-4; `other` the rest (cs, sk).
    Czech,
    /// `one` 1; `few` 0 and 2-19 (mod 100); `other` the rest (ro).
    Romanian,
}

/// What a locale writes numbers and plurals with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub plural: PluralRule,
    pub decimal: char,
    pub group: char,
    /// CLDR `minimumGroupingDigits`: Spanish writes `1234` but `12.345`.
    pub min_group: usize,
}

const NBSP: char = '\u{a0}';

const fn r(plural: PluralRule, decimal: char, group: char, min_group: usize) -> Rules {
    Rules {
        plural,
        decimal,
        group,
        min_group,
    }
}

/// Codes with rules of their own, exact codes before languages.
const TABLE: &[(&str, Rules)] = {
    use PluralRule::*;
    &[
        ("pt-PT", r(One, ',', NBSP, 2)),
        ("en", r(One, '.', ',', 1)),
        ("pt", r(ZeroOne, ',', '.', 1)),
        ("es", r(One, ',', '.', 2)),
        ("fr", r(ZeroOne, ',', NBSP, 1)),
        ("it", r(One, ',', '.', 1)),
        ("de", r(One, ',', '.', 1)),
        ("nl", r(One, ',', '.', 1)),
        ("sv", r(One, ',', NBSP, 1)),
        ("da", r(One, ',', '.', 1)),
        ("nb", r(One, ',', NBSP, 1)),
        ("fi", r(One, ',', NBSP, 1)),
        ("el", r(One, ',', '.', 1)),
        ("hu", r(One, ',', NBSP, 1)),
        ("tr", r(One, ',', '.', 1)),
        ("ro", r(Romanian, ',', '.', 1)),
        ("ru", r(EastSlavic, ',', NBSP, 1)),
        ("uk", r(EastSlavic, ',', NBSP, 1)),
        ("be", r(EastSlavic, ',', NBSP, 1)),
        ("pl", r(Polish, ',', NBSP, 2)),
        ("cs", r(Czech, ',', NBSP, 1)),
        ("sk", r(Czech, ',', NBSP, 1)),
        ("zh", r(Other, '.', ',', 1)),
        ("ja", r(Other, '.', ',', 1)),
        ("ko", r(Other, '.', ',', 1)),
        ("id", r(Other, ',', '.', 1)),
        ("ms", r(Other, '.', ',', 1)),
        ("vi", r(Other, ',', '.', 1)),
        ("th", r(Other, '.', ',', 1)),
    ]
};

/// The rules of a locale, or `None` when neither its code nor its
/// language is in the table.
pub fn known_rules(locale: Locale) -> Option<Rules> {
    let find = |c: &str| TABLE.iter().find(|(k, _)| *k == c).map(|(_, r)| *r);
    find(locale.code()).or_else(|| find(locale.language()))
}

/// The rules of a locale; English ones when it is not in the table.
pub fn rules(locale: Locale) -> Rules {
    known_rules(locale).unwrap_or(r(PluralRule::One, '.', ',', 1))
}

pub(crate) fn decimal_separator(locale: Locale) -> char {
    rules(locale).decimal
}

pub(crate) fn group_separator(locale: Locale) -> char {
    rules(locale).group
}

pub(crate) fn min_grouping_digits(locale: Locale) -> usize {
    rules(locale).min_group
}
