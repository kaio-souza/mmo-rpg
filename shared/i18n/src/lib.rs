//! Locales, text packs, message formatting, plural rules (CLDR) and numbers
//! per locale. Pure: never reads files.
//!
//! Design: and.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod bundle;
mod check;
mod error;
mod format;
mod keys;
mod locale;
mod message;
mod plural;
mod source;

pub use base::{Locale, ParseLocaleError};
pub use bundle::Bundle;
pub use check::{Severity, TextProblem, TextProblemKind, check_locale, check_placeholders};
pub use error::I18nError;
pub use format::{format, format_fixed, format_int};
pub use keys::KeyTable;
pub use locale::{PluralRule, Rules, known_rules, rules};
pub use message::{Message, MessageError};
pub use plural::{PluralCategory, plural_category, required_plurals};
