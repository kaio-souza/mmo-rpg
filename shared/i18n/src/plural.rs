//! CLDR plural rules by language (`locale.rs`) (integers; see `category_for_fixed`).

use crate::Locale;
use crate::locale::{PluralRule, rules};
use base::Fixed;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub const fn name(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }

    pub(crate) fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "zero" => PluralCategory::Zero,
            "one" => PluralCategory::One,
            "two" => PluralCategory::Two,
            "few" => PluralCategory::Few,
            "many" => PluralCategory::Many,
            "other" => PluralCategory::Other,
            _ => return None,
        })
    }
}

impl fmt::Display for PluralCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Categories every `plural` in this locale must have.
pub fn required_plurals(locale: Locale) -> &'static [PluralCategory] {
    use PluralCategory::*;
    match rules(locale).plural {
        PluralRule::One | PluralRule::ZeroOne => &[One, Other],
        PluralRule::EastSlavic | PluralRule::Polish => &[One, Few, Many, Other],
        PluralRule::Czech | PluralRule::Romanian => &[One, Few, Other],
        PluralRule::Other => &[Other],
    }
}

pub fn plural_category(locale: Locale, n: i64) -> PluralCategory {
    use PluralCategory::*;
    let i = n.unsigned_abs();
    let (m10, m100) = (i % 10, i % 100);
    match rules(locale).plural {
        PluralRule::One => {
            if i == 1 {
                One
            } else {
                Other
            }
        }
        // CLDR `pt`, `fr`: one for i = 0..1.
        PluralRule::ZeroOne => {
            if i <= 1 {
                One
            } else {
                Other
            }
        }
        PluralRule::EastSlavic => {
            if m10 == 1 && m100 != 11 {
                One
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                Few
            } else {
                Many
            }
        }
        PluralRule::Polish => {
            if i == 1 {
                One
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                Few
            } else {
                Many
            }
        }
        PluralRule::Czech => match i {
            1 => One,
            2..=4 => Few,
            _ => Other,
        },
        PluralRule::Romanian => {
            if i == 1 {
                One
            } else if i == 0 || (2..=19).contains(&m100) {
                Few
            } else {
                Other
            }
        }
        PluralRule::Other => Other,
    }
}

/// Numbers with a fractional part (`v > 0` in CLDR terms).
pub(crate) fn category_for_fixed(locale: Locale, x: Fixed) -> PluralCategory {
    if x.millis() % Fixed::SCALE == 0 {
        return plural_category(locale, i64::from(x.trunc()));
    }
    match rules(locale).plural {
        PluralRule::ZeroOne if x.trunc().unsigned_abs() <= 1 => PluralCategory::One,
        PluralRule::Czech | PluralRule::Polish | PluralRule::EastSlavic => PluralCategory::Other,
        PluralRule::Romanian => PluralCategory::Few,
        _ => PluralCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PluralCategory::*;

    #[test]
    fn russian() {
        let cases = [
            (0, Many),
            (1, One),
            (2, Few),
            (4, Few),
            (5, Many),
            (11, Many),
            (12, Many),
            (14, Many),
            (21, One),
            (22, Few),
            (101, One),
            (111, Many),
            (-3, Few),
        ];
        for (n, c) in cases {
            assert_eq!(plural_category(Locale::from_static("ru"), n), c, "{n}");
        }
    }

    #[test]
    fn one_other_locales() {
        for l in [
            Locale::EN,
            Locale::from_static("it"),
            Locale::from_static("de"),
            Locale::from_static("es"),
        ] {
            assert_eq!(plural_category(l, 1), One);
            assert_eq!(plural_category(l, 0), Other);
            assert_eq!(plural_category(l, 2), Other);
        }
        assert_eq!(plural_category(Locale::from_static("pt-BR"), 0), One);
        assert_eq!(plural_category(Locale::from_static("pt-BR"), 1), One);
        assert_eq!(plural_category(Locale::from_static("pt-BR"), 2), Other);
    }

    #[test]
    fn other_only_locales() {
        for l in [
            Locale::from_static("zh"),
            Locale::from_static("id"),
            Locale::from_static("ja"),
            Locale::from_static("ko"),
        ] {
            for n in [0, 1, 2, 5, 21] {
                assert_eq!(plural_category(l, n), Other);
            }
        }
    }

    #[test]
    fn every_category_returned_is_required() {
        for c in ["en", "pt-BR", "pt-PT", "fr", "ru", "pl", "cs", "ro", "zh", "xx"] {
            let l = Locale::from_static(c);
            for n in -200..200 {
                assert!(required_plurals(l).contains(&plural_category(l, n)));
            }
        }
    }

    #[test]
    fn decimals() {
        let x: Fixed = "1.5".parse().unwrap();
        assert_eq!(category_for_fixed(Locale::EN, x), Other);
        assert_eq!(category_for_fixed(Locale::from_static("pt-BR"), x), One);
        assert_eq!(category_for_fixed(Locale::from_static("ru"), x), Other);
        assert_eq!(category_for_fixed(Locale::EN, Fixed::ONE), One);
    }
}
