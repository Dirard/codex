//! Content keys of `agent()` calls (UC6, R6.2).

use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

/// Options of an `agent()` call that change its answer and therefore its key.
/// `label` and `phase` only describe the call and are excluded.
pub const KEYED_OPTIONS: &[&str] = &["schema", "model", "effort", "agent_type", "isolation"];

/// UC6: identify an `agent()` call by the SHA-256 of the canonical JSON of its prompt and
/// its result-affecting options.
pub fn call_key(prompt: &str, options: &serde_json::Map<String, Value>) -> String {
    let mut keyed = serde_json::Map::new();
    keyed.insert("prompt".to_string(), Value::String(prompt.to_string()));
    for name in KEYED_OPTIONS {
        if let Some(value) = options.get(*name).filter(|value| !value.is_null()) {
            keyed.insert((*name).to_string(), value.clone());
        }
    }
    sha256_hex(canonical_json(&Value::Object(keyed)).as_bytes())
}

/// Hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Compact JSON with object keys sorted at every level, independent of map ordering.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<(&String, &Value)> = object.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            out.push('{');
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(value, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}
