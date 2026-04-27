//! CQL string helpers. CoreDB has no prepared statements, so we serialize
//! values directly. All string inputs must pass `check_identifier` first.

use super::RepoError;

pub fn quote_text(s: &str) -> String {
    let escaped = s.replace('\'', "''");
    format!("'{}'", escaped)
}

/// Format an f64 so CoreDB's parser always reads it as Double. Rust's default
/// `{}` drops trailing zeros (e.g. `58.0` → `"58"`), which CoreDB then
/// interprets as Int and stores under the wrong tag. Force at least one
/// decimal digit here.
pub fn fmt_double(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        // Rust's default already uses the shortest lossless form.
        format!("{}", v)
    }
}

/// Permit only safe identifier characters for values that get embedded into CQL.
/// Anything that could break out of a string literal or inject extra statements
/// is rejected. Length is capped at 128 bytes.
pub fn check_identifier(s: &str) -> Result<(), RepoError> {
    if s.is_empty() || s.len() > 128 {
        return Err(RepoError::InvalidIdentifier(s.to_string()));
    }
    let ok = s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b':');
    if !ok {
        return Err(RepoError::InvalidIdentifier(s.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_escapes_single_quote() {
        assert_eq!(quote_text("ab'c"), "'ab''c'");
        assert_eq!(quote_text("plain"), "'plain'");
    }

    #[test]
    fn identifier_bounds() {
        assert!(check_identifier("HD-A120").is_ok());
        assert!(check_identifier("edge-line-01").is_ok());
        assert!(check_identifier("01HV0123456789ABCDEFGHJKMN").is_ok());
        assert!(check_identifier("with.dots").is_ok());
        assert!(check_identifier("with:colons").is_ok());
        assert!(check_identifier("").is_err());
        assert!(check_identifier("bad; DROP TABLE").is_err());
        assert!(check_identifier("quote'here").is_err());
        assert!(check_identifier(&"x".repeat(129)).is_err());
    }
}
