use crate::TextProblem;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum I18nError {
    /// Text files with errors (warnings are not included).
    Text(Vec<TextProblem>),
    /// Text pack that cannot be read or written.
    Pack(String),
}

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            I18nError::Text(problems) => {
                write!(f, "{} text problem(s)", problems.len())?;
                for p in problems {
                    write!(f, "\n  {p}")?;
                }
                Ok(())
            }
            I18nError::Pack(e) => write!(f, "text pack: {e}"),
        }
    }
}

impl std::error::Error for I18nError {}
