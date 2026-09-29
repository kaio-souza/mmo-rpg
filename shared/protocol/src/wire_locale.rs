//! `Locale` travels as its code (`"pt-BR"`), so a locale this side does not
//! know (newer client) becomes `en` instead of a decode error.

use base::Locale;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(l: &Locale, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(l.code())
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Locale, D::Error> {
    let code = String::deserialize(d)?;
    Ok(code.parse().unwrap_or(Locale::SOURCE))
}
