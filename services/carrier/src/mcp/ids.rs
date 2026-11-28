//! Argument validation for identifiers that reach a URL or a database row.
//!
//! A tool argument is attacker-controlled text. The 3PL adapters interpolate
//! `booking_ref` and `tracking_number` straight into request paths, so an
//! unchecked `../internal/x` or `abc?tenant_id=..` would rewrite the upstream
//! request. Reject anything that is not a plain token before it gets there.

use serde_json::Value;
use uuid::Uuid;

/// Longest carrier reference accepted. Real ones are under 40 characters.
const MAX_REF_LEN: usize = 64;

/// A carrier-issued reference (booking ref or tracking number): 1..=64 ASCII
/// letters, digits, `-` or `_`. No `/`, `.`, `?`, `#`, `%` or whitespace, so it
/// can only ever be one path segment with no query.
pub fn carrier_ref<'a>(args: &'a Value, field: &str) -> Result<&'a str, String> {
    let v = args[field].as_str().ok_or_else(|| format!("{field} is required"))?;
    let ok = !v.is_empty()
        && v.len() <= MAX_REF_LEN
        && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(v)
    } else {
        Err(format!(
            "{field} must be 1-{MAX_REF_LEN} characters of letters, digits, '-' or '_'"
        ))
    }
}

/// An optional UUID argument. Absent or null is `None`; present but malformed
/// is an error, never silently dropped.
pub fn optional_uuid(args: &Value, field: &str) -> Result<Option<Uuid>, String> {
    match &args[field] {
        Value::Null => Ok(None),
        Value::String(s) => Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| format!("{field} must be a UUID")),
        _ => Err(format!("{field} must be a UUID")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_plain_carrier_references() {
        for v in ["1Z999AA10123456784", "JD014600003SE", "abc-123_DEF"] {
            assert_eq!(carrier_ref(&json!({"r": v}), "r"), Ok(v));
        }
    }

    #[test]
    fn rejects_path_and_query_smuggling() {
        let too_long = "a".repeat(65);
        for v in [
            "../internal/secrets", "abc/def", "abc?tenant_id=other", "abc#frag", "a%2e%2e",
            "abc def", "..", ".", "", "abc\n", "abc;x", too_long.as_str(),
        ] {
            assert!(carrier_ref(&json!({"r": v}), "r").is_err(), "{v:?}");
        }
        assert!(carrier_ref(&json!({}), "r").is_err());
        assert!(carrier_ref(&json!({"r": 5}), "r").is_err());
    }

    #[test]
    fn optional_uuid_distinguishes_absent_from_malformed() {
        let id = Uuid::new_v4();
        assert_eq!(optional_uuid(&json!({"s": id.to_string()}), "s"), Ok(Some(id)));
        assert_eq!(optional_uuid(&json!({}), "s"), Ok(None));
        assert_eq!(optional_uuid(&json!({"s": null}), "s"), Ok(None));
        assert!(optional_uuid(&json!({"s": "../x"}), "s").is_err());
        assert!(optional_uuid(&json!({"s": 7}), "s").is_err());
    }
}
