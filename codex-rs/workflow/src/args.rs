//! `meta.args`: the JSON Schema of a script's `args`, checked before a run is created
//! (`specs/builtin-workflows-2026-11-07.md`, UC3, R3.1–R3.5).
//!
//! The schema uses the keywords of the workflow schema subset, but unlike an `agent()`
//! schema it may leave properties optional and allow extra ones. `args` themselves are not
//! changed: defaults are applied by the script (R3.5).

use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use crate::meta::WorkflowMeta;
use crate::schema::check_schema;
use crate::schema::validate_value;

/// Longest usage text of a script's args, in characters (R3.4).
pub const MAX_ARGS_USAGE_CHARS: usize = 2000;

/// `meta.args` of a script: the schema and the order of its top-level properties in the
/// script, so the usage lists them as the author wrote them whatever order JSON maps keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Value", into = "Value")]
pub struct ArgsSchema {
    pub schema: Value,
    pub order: Vec<String>,
}

impl ArgsSchema {
    /// Property names in source order, then the ones `order` misses, by name.
    fn property_names(&self) -> Vec<String> {
        let Some(Value::Object(properties)) = self.schema.get("properties") else {
            return Vec::new();
        };
        let mut names: Vec<String> = self
            .order
            .iter()
            .filter(|name| properties.contains_key(name.as_str()))
            .cloned()
            .collect();
        let mut rest: Vec<String> = properties
            .keys()
            .filter(|key| !names.contains(key))
            .cloned()
            .collect();
        rest.sort();
        names.extend(rest);
        names
    }
}

impl From<Value> for ArgsSchema {
    fn from(schema: Value) -> Self {
        Self {
            schema,
            order: Vec::new(),
        }
    }
}

impl From<ArgsSchema> for Value {
    fn from(args: ArgsSchema) -> Self {
        args.schema
    }
}

/// R3.1: `meta.args` must be an object schema of the workflow subset with `properties`,
/// within the size and depth limits of agent schemas; `required` may list some properties.
pub fn check_args_schema(schema: &Value) -> Result<(), String> {
    check_schema(schema, /*strict*/ false)
}

/// R3.2, R3.3: check `args_json` against the script's `meta.args`; absent or null args are
/// checked as `{}`. A script without `meta.args` accepts any args. The error lists the
/// violations and the usage (R3.4).
pub fn validate_args(meta: &WorkflowMeta, args_json: &str) -> Result<(), String> {
    let Some(args) = &meta.args else {
        return Ok(());
    };
    let value: Value =
        serde_json::from_str(args_json).map_err(|error| format!("args are not JSON: {error}"))?;
    let value = if value.is_null() {
        Value::Object(Map::new())
    } else {
        value
    };
    validate_value(&args.schema, &value).map_err(|errors| {
        format!(
            "args of workflow \"{}\" do not match its `meta.args`: {}\n{}",
            meta.name,
            errors.join("; "),
            args_usage(args, &meta.name)
        )
    })
}

/// R3.4: one line per property of `meta.args` — name, type or allowed values, `required`
/// when listed, description — within [`MAX_ARGS_USAGE_CHARS`].
pub fn args_usage(args: &ArgsSchema, name: &str) -> String {
    let required: Vec<&str> = match args.schema.get("required") {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let mut usage = format!("args of workflow \"{name}\":");
    let properties = args.schema.get("properties");
    for property in args.property_names() {
        let Some(schema) = properties.and_then(|properties| properties.get(&property)) else {
            continue;
        };
        usage.push_str("\n- ");
        usage.push_str(&property);
        usage.push_str(": ");
        usage.push_str(&render_type(schema));
        if required.contains(&property.as_str()) {
            usage.push_str(", required");
        }
        if let Some(Value::String(description)) = schema.get("description")
            && !description.trim().is_empty()
        {
            usage.push_str(" — ");
            usage.push_str(description.trim());
        }
    }
    if usage.chars().count() > MAX_ARGS_USAGE_CHARS {
        usage = usage.chars().take(MAX_ARGS_USAGE_CHARS - 1).collect();
        usage.push('…');
    }
    usage
}

/// The type of a schema node in words: allowed values, alternatives, array items and
/// numeric bounds; `any` without a constraint.
fn render_type(schema: &Value) -> String {
    let Value::Object(node) = schema else {
        return "any".to_string();
    };
    if let Some(value) = node.get("const") {
        return value.to_string();
    }
    if let Some(Value::Array(values)) = node.get("enum") {
        return values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    if let Some(Value::Array(variants)) = node.get("anyOf") {
        return variants
            .iter()
            .map(render_type)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    let types: Vec<&str> = match node.get("type") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => return "any".to_string(),
    };
    types
        .iter()
        .map(|name| match *name {
            "array" => match node.get("items") {
                Some(items) => format!("array of {}", render_type(items)),
                None => "array".to_string(),
            },
            "integer" | "number" => {
                let mut text = (*name).to_string();
                for (keyword, operator) in [
                    ("minimum", ">="),
                    ("exclusiveMinimum", ">"),
                    ("maximum", "<="),
                    ("exclusiveMaximum", "<"),
                ] {
                    if let Some(bound) = node.get(keyword) {
                        text.push_str(&format!(" {operator} {bound}"));
                    }
                }
                text
            }
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
