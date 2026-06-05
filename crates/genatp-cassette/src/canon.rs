//! JSON Canonicalization Scheme (JCS) — RFC 8785.
//!
//! Provides deterministic JSON serialization so that the same logical
//! value always produces the same byte sequence. This is critical for
//! cassette signing: the signature covers the canonical bytes, and any
//! non-determinism in serialization would break verification.
//!
//! Our implementation follows the RFC 8785 subset that `serde_json`
//! naturally produces when we sort object keys:
//!
//! 1. No whitespace between tokens.
//! 2. Object keys sorted lexicographically (Unicode code-point order).
//! 3. Numbers serialized per ES2015 `Number.prototype.toString()`.
//! 4. Strings serialized with minimal escaping.
//!
//! For our use case (cassette payloads are simple nested
//! structs with no exotic floats), this is sufficient.

/// Produce canonical JSON bytes for a `serde_json::Value`.
///
/// Object keys are sorted recursively. The output contains no
/// insignificant whitespace.
#[must_use]
pub fn canonical_json(value: &serde_json::Value) -> Vec<u8> {
    let canonical = canonicalize(value);
    // `to_vec` without pretty-print gives compact JSON.
    serde_json::to_vec(&canonical).expect("canonical value is always serializable")
}

/// Recursively sort object keys (depth-first).
fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            // BTreeMap gives us sorted keys.
            let sorted: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize(v)))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect();
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize).collect())
        }
        // Scalars are already canonical.
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_object_keys() {
        let input = serde_json::json!({"z": 1, "a": 2, "m": 3});
        let bytes = canonical_json(&input);
        let expected = br#"{"a":2,"m":3,"z":1}"#;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn sorts_nested_objects() {
        let input = serde_json::json!({"b": {"z": 1, "a": 2}, "a": 0});
        let bytes = canonical_json(&input);
        let expected = br#"{"a":0,"b":{"a":2,"z":1}}"#;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn preserves_array_order() {
        let input = serde_json::json!({"items": [3, 1, 2]});
        let bytes = canonical_json(&input);
        let expected = br#"{"items":[3,1,2]}"#;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn deterministic_across_calls() {
        let input =
            serde_json::json!({"url": "https://example.com", "action": "click", "ref": "@e3"});
        let a = canonical_json(&input);
        let b = canonical_json(&input);
        assert_eq!(a, b, "canonical_json must be deterministic");
    }

    #[test]
    fn handles_null_bool_string_number() {
        let input = serde_json::json!({"n": null, "b": true, "s": "hello", "i": 42, "f": 3.14});
        let bytes = canonical_json(&input);
        // Keys sorted: b, f, i, n, s
        let expected = br#"{"b":true,"f":3.14,"i":42,"n":null,"s":"hello"}"#;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn empty_object() {
        let input = serde_json::json!({});
        let bytes = canonical_json(&input);
        assert_eq!(bytes, b"{}");
    }

    #[test]
    fn deeply_nested_arrays_in_objects() {
        let input = serde_json::json!({"z": [{"b": 2, "a": 1}], "a": []});
        let bytes = canonical_json(&input);
        let expected = br#"{"a":[],"z":[{"a":1,"b":2}]}"#;
        assert_eq!(bytes, expected);
    }
}
