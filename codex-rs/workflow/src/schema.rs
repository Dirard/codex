//! JSON Schema subset for `agent({ schema })` (UC3, R3.5, R3.6).
//!
//! A schema must be compatible with strict structured output before the child is spawned:
//! a non-strict schema makes the Responses API reject the turn (`invalid_json_schema`).
//! The child's final message is then validated against the same schema.

use serde_json::Map;
use serde_json::Value;

/// Deepest accepted schema nesting.
pub const MAX_SCHEMA_DEPTH: usize = 10;
/// Largest accepted schema, in bytes of compact JSON.
pub const MAX_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_REPORTED_ERRORS: usize = 20;

const TYPES: &[&str] = &[
    "string", "number", "integer", "boolean", "object", "array", "null",
];
const KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "anyOf",
    "description",
    "title",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "pattern",
    "format",
    "minItems",
    "maxItems",
];

/// UC3: check before spawning that `schema` is an object schema accepted by strict
/// structured output: every object lists all properties in `required` and sets
/// `additionalProperties: false`, every array has `items`, every node has a type.
pub fn check_strict_compatible(schema: &Value) -> Result<(), String> {
    let size = serde_json::to_string(schema).map_or(usize::MAX, |text| text.len());
    if size > MAX_SCHEMA_BYTES {
        return Err(format!(
            "schema has {size} bytes; the limit is {MAX_SCHEMA_BYTES}"
        ));
    }
    let Value::Object(root) = schema else {
        return Err("schema must be a JSON Schema object".to_string());
    };
    if root.get("type") != Some(&Value::String("object".to_string())) {
        return Err("the root schema must have `type: \"object\"`".to_string());
    }
    check_node(root, "$", /*depth*/ 0)
}

fn check_node(node: &Map<String, Value>, path: &str, depth: usize) -> Result<(), String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!(
            "{path}: schema is nested deeper than {MAX_SCHEMA_DEPTH} levels"
        ));
    }
    if let Some(keyword) = node.keys().find(|key| !KEYWORDS.contains(&key.as_str())) {
        return Err(format!(
            "{path}: keyword `{keyword}` is not supported in workflow schemas (use only {})",
            KEYWORDS.join(", ")
        ));
    }
    let types = node_types(node, path)?;
    if types.is_empty()
        && !node.contains_key("anyOf")
        && !node.contains_key("enum")
        && !node.contains_key("const")
    {
        return Err(format!(
            "{path}: every schema needs `type`, `anyOf`, `enum` or `const`"
        ));
    }
    if types.contains(&"object") || node.contains_key("properties") {
        let Some(Value::Object(properties)) = node.get("properties") else {
            return Err(format!("{path}: an object schema needs `properties`"));
        };
        if node.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Err(format!(
                "{path}: strict schemas need `additionalProperties: false` on every object"
            ));
        }
        let required: Vec<&str> = match node.get("required") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
            None => Vec::new(),
            Some(_) => return Err(format!("{path}: `required` must be an array of names")),
        };
        if let Some(missing) = properties
            .keys()
            .find(|key| !required.contains(&key.as_str()))
        {
            return Err(format!(
                "{path}: strict schemas list every property in `required`; `{missing}` is missing (use a type like [\"string\", \"null\"] for optional values)"
            ));
        }
        if let Some(unknown) = required.iter().find(|key| !properties.contains_key(**key)) {
            return Err(format!(
                "{path}: `required` names `{unknown}`, which is not in `properties`"
            ));
        }
        for (key, child) in properties {
            check_child(child, &format!("{path}.{key}"), depth)?;
        }
    }
    if types.contains(&"array") {
        let Some(items) = node.get("items") else {
            return Err(format!("{path}: an array schema needs `items`"));
        };
        check_child(items, &format!("{path}[]"), depth)?;
    }
    if let Some(any_of) = node.get("anyOf") {
        let Value::Array(variants) = any_of else {
            return Err(format!("{path}: `anyOf` must be an array of schemas"));
        };
        if variants.is_empty() {
            return Err(format!("{path}: `anyOf` must not be empty"));
        }
        for (index, variant) in variants.iter().enumerate() {
            check_child(variant, &format!("{path}.anyOf[{index}]"), depth)?;
        }
    }
    if let Some(enum_values) = node.get("enum")
        && !matches!(enum_values, Value::Array(values) if !values.is_empty())
    {
        return Err(format!("{path}: `enum` must be a non-empty array"));
    }
    Ok(())
}

fn check_child(child: &Value, path: &str, depth: usize) -> Result<(), String> {
    let Value::Object(child) = child else {
        return Err(format!("{path}: subschemas must be objects"));
    };
    check_node(child, path, depth + 1)
}

fn node_types<'a>(node: &'a Map<String, Value>, path: &str) -> Result<Vec<&'a str>, String> {
    let types: Vec<&str> = match node.get("type") {
        None => Vec::new(),
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        Some(_) => return Err(format!("{path}: `type` must be a string or an array")),
    };
    if let Some(unknown) = types.iter().find(|name| !TYPES.contains(name)) {
        return Err(format!("{path}: unknown type `{unknown}`"));
    }
    Ok(types)
}

/// UC3: validate a parsed final message against a schema accepted by
/// [`check_strict_compatible`]. Returns at most twenty errors, each prefixed with the
/// JSON path of the offending value.
pub fn validate_value(schema: &Value, value: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    validate_node(schema, value, "$", &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        errors.truncate(MAX_REPORTED_ERRORS);
        Err(errors)
    }
}

fn validate_node(schema: &Value, value: &Value, path: &str, errors: &mut Vec<String>) {
    if errors.len() >= MAX_REPORTED_ERRORS {
        return;
    }
    let Value::Object(node) = schema else {
        return;
    };
    let types = node_types(node, path).unwrap_or_default();
    if !types.is_empty() && !types.iter().any(|name| has_type(value, name)) {
        errors.push(format!(
            "{path}: expected {}, got {}",
            types.join(" or "),
            type_name(value)
        ));
        return;
    }
    if let Some(Value::Array(variants)) = node.get("anyOf") {
        let matched = variants.iter().any(|variant| {
            let mut variant_errors = Vec::new();
            validate_node(variant, value, path, &mut variant_errors);
            variant_errors.is_empty()
        });
        if !matched {
            errors.push(format!("{path}: value matches none of `anyOf`"));
            return;
        }
    }
    if let Some(Value::Array(allowed)) = node.get("enum")
        && !allowed.contains(value)
    {
        errors.push(format!("{path}: value is not one of the `enum` values"));
    }
    if let Some(expected) = node.get("const")
        && expected != value
    {
        errors.push(format!("{path}: value differs from `const`"));
    }
    match value {
        Value::Object(object) => validate_object(node, object, path, errors),
        Value::Array(items) => {
            check_count(
                node,
                "minItems",
                "maxItems",
                items.len(),
                path,
                "items",
                errors,
            );
            if let Some(item_schema) = node.get("items") {
                for (index, item) in items.iter().enumerate() {
                    validate_node(item_schema, item, &format!("{path}[{index}]"), errors);
                }
            }
        }
        Value::String(text) => {
            check_count(
                node,
                "minLength",
                "maxLength",
                text.chars().count(),
                path,
                "characters",
                errors,
            );
            if let Some(Value::String(pattern)) = node.get("pattern")
                && let Ok(regex) = regex_lite::Regex::new(pattern)
                && !regex.is_match(text)
            {
                errors.push(format!("{path}: value does not match `pattern`"));
            }
        }
        Value::Number(number) => {
            if let Some(number) = number.as_f64() {
                validate_number(node, number, path, errors);
            }
        }
        Value::Bool(_) | Value::Null => {}
    }
}

fn validate_object(
    node: &Map<String, Value>,
    object: &Map<String, Value>,
    path: &str,
    errors: &mut Vec<String>,
) {
    let properties = match node.get("properties") {
        Some(Value::Object(properties)) => Some(properties),
        _ => None,
    };
    if let Some(Value::Array(required)) = node.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                errors.push(format!("{path}: missing required property `{name}`"));
            }
        }
    }
    // Sorted, so error messages do not depend on map ordering features.
    let mut keys: Vec<&String> = object.keys().collect();
    keys.sort();
    for key in keys {
        let value = &object[key.as_str()];
        match properties.and_then(|properties| properties.get(key)) {
            Some(property) => validate_node(property, value, &format!("{path}.{key}"), errors),
            None if node.get("additionalProperties") == Some(&Value::Bool(false)) => {
                errors.push(format!("{path}: unexpected property `{key}`"));
            }
            None => {}
        }
    }
}

fn validate_number(node: &Map<String, Value>, number: f64, path: &str, errors: &mut Vec<String>) {
    let bound = |key: &str| node.get(key).and_then(Value::as_f64);
    if let Some(minimum) = bound("minimum")
        && number < minimum
    {
        errors.push(format!(
            "{path}: {number} is less than the minimum {minimum}"
        ));
    }
    if let Some(maximum) = bound("maximum")
        && number > maximum
    {
        errors.push(format!(
            "{path}: {number} is greater than the maximum {maximum}"
        ));
    }
    if let Some(minimum) = bound("exclusiveMinimum")
        && number <= minimum
    {
        errors.push(format!("{path}: {number} must be greater than {minimum}"));
    }
    if let Some(maximum) = bound("exclusiveMaximum")
        && number >= maximum
    {
        errors.push(format!("{path}: {number} must be less than {maximum}"));
    }
    if let Some(step) = bound("multipleOf")
        && step > 0.0
        && ((number / step).round() * step - number).abs() > f64::EPSILON * number.abs().max(1.0)
    {
        errors.push(format!("{path}: {number} is not a multiple of {step}"));
    }
}

fn check_count(
    node: &Map<String, Value>,
    min_key: &str,
    max_key: &str,
    count: usize,
    path: &str,
    unit: &str,
    errors: &mut Vec<String>,
) {
    let count = count as u64;
    if let Some(min) = node.get(min_key).and_then(Value::as_u64)
        && count < min
    {
        errors.push(format!(
            "{path}: has {count} {unit}; at least {min} required"
        ));
    }
    if let Some(max) = node.get(max_key).and_then(Value::as_u64)
        && count > max
    {
        errors.push(format!("{path}: has {count} {unit}; at most {max} allowed"));
    }
}

fn has_type(value: &Value, name: &str) -> bool {
    match name {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => match value {
            Value::Number(number) => {
                number.is_i64()
                    || number.is_u64()
                    || number.as_f64().is_some_and(|number| number.fract() == 0.0)
            }
            _ => false,
        },
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// R3.12: the text appended to an agent's task when the schema goes in the prompt instead of
/// `final_output_json_schema`.
pub fn prompt_schema_suffix(schema: &Value) -> String {
    format!(
        "Your final message must be only a JSON object that matches the JSON schema below, with no other text and no code fence.\nJSON schema:\n{schema}"
    )
}

/// R3.12: the follow-up asking an agent whose native structured-output request the provider
/// rejected to answer with the schema in the prompt.
pub fn native_rejected_follow_up(schema: &Value) -> String {
    format!(
        "The model provider rejected the structured-output format of the previous request. Complete the task above. {}",
        prompt_schema_suffix(schema)
    )
}

/// Request parameters that carry the answer format: `text.format` of the Responses API and
/// `response_format` of Chat Completions-style providers.
const FORMAT_PARAMS: [&str; 2] = ["text.format", "response_format"];

/// R3.12: whether a failed agent turn is the provider refusing the structured-output format,
/// not another client error such as an invalid tool schema.
///
/// The turn must have failed with a client error: HTTP 400 or 422, a bad request, or a
/// message that carries the provider's `invalid_request_error` body or a 400/422 status
/// (Codex reports a plain HTTP 400 as an invalid request whose text is the provider's error
/// body). When that body names the rejected parameter (`param`), the parameter decides: the
/// answer format (`text.format…`, `response_format…`) or nothing. Otherwise the text must
/// name the answer format (`text.format`, `response_format`, `json_schema` or "structured
/// output") and must not name a function or a tool (`function`, `tools[`), whose schemas
/// providers reject with the same words.
pub fn is_schema_format_rejection(
    http_status: Option<u16>,
    bad_request: bool,
    message: &str,
) -> bool {
    let lower = message.to_ascii_lowercase();
    let client_error = bad_request
        || matches!(http_status, Some(400 | 422))
        || ["invalid_request_error", "status 400", "status 422"]
            .iter()
            .any(|marker| lower.contains(marker));
    if !client_error {
        return false;
    }
    if let Some(param) = provider_error_param(message) {
        let param = param.to_ascii_lowercase();
        return FORMAT_PARAMS
            .iter()
            .any(|format| param == *format || param.starts_with(&format!("{format}.")));
    }
    let names_format = [
        "text.format",
        "response_format",
        "json_schema",
        "structured output",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let names_tool = ["function", "tools["]
        .iter()
        .any(|needle| lower.contains(needle));
    names_format && !names_tool
}

/// The `param` of the provider's JSON error body in `message`, when it names one. The body
/// is the whole message or follows a prefix such as `unexpected status 400 Bad Request: `;
/// both `{"error": {...}}` and a bare error object are accepted.
fn provider_error_param(message: &str) -> Option<String> {
    let start = message.find('{')?;
    let body = serde_json::Deserializer::from_str(&message[start..])
        .into_iter::<Value>()
        .next()?
        .ok()?;
    let error = body
        .get("error")
        .filter(|error| error.is_object())
        .unwrap_or(&body);
    error
        .get("param")
        .and_then(Value::as_str)
        .filter(|param| !param.is_empty())
        .map(str::to_string)
}

/// Parses a final message as JSON, tolerating a single surrounding Markdown code fence.
pub fn parse_final_message(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .map_or(trimmed, str::trim);
    serde_json::from_str(unfenced)
        .map_err(|error| format!("the final message is not JSON: {error}"))
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
