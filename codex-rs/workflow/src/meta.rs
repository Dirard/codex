//! Static validation of a workflow script before a run is created (UC1, R1.2, R1.3).

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::literal::parse_literal;
use crate::scan::Token;
use crate::scan::TokenKind;
use crate::scan::scan;

/// Largest accepted script, in bytes.
pub const MAX_SCRIPT_BYTES: usize = 256 * 1024;
/// Largest accepted `args`, in bytes of compact JSON.
pub const MAX_ARGS_BYTES: usize = 1024 * 1024;
/// Most phases a script may declare in `meta.phases`.
pub const MAX_PHASES: usize = 32;
/// Longest phase title, in characters.
pub const MAX_PHASE_TITLE_CHARS: usize = 120;
/// Longest `meta.description`, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 2000;

/// The `export const meta` object of a workflow script.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub phases: Vec<String>,
}

/// A script that passed static validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedScript {
    pub meta: WorkflowMeta,
    /// The script with its `export` keyword blanked, so line and column numbers of the
    /// original script stay valid inside the generated cell.
    pub body: String,
}

/// The validation rule a script violated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptRule {
    Size,
    ArgsSize,
    Syntax,
    Meta,
    Export,
    Import,
    Determinism,
}

impl ScriptRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::ArgsSize => "args_size",
            Self::Syntax => "syntax",
            Self::Meta => "meta",
            Self::Export => "export",
            Self::Import => "import",
            Self::Determinism => "determinism",
        }
    }
}

/// A static validation failure with the violated rule and the script line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub rule: ScriptRule,
    pub line: Option<usize>,
    pub message: String,
}

impl ScriptError {
    fn new(rule: ScriptRule, line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            rule,
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(
                formatter,
                "workflow script rejected (rule `{}`, line {line}): {}",
                self.rule.as_str(),
                self.message
            ),
            None => write!(
                formatter,
                "workflow script rejected (rule `{}`): {}",
                self.rule.as_str(),
                self.message
            ),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Checks the serialized size of `args` (R1.3).
pub fn check_args_size(args_json: &str) -> Result<(), ScriptError> {
    if args_json.len() > MAX_ARGS_BYTES {
        return Err(ScriptError::new(
            ScriptRule::ArgsSize,
            None,
            format!(
                "args take {} bytes after serialization; the limit is {MAX_ARGS_BYTES}",
                args_json.len()
            ),
        ));
    }
    Ok(())
}

/// UC1: validate a script statically and extract its `meta` without running it.
///
/// The script must start, after optional comments, with
/// `export const meta = <object literal>;`. Any other `export`, any `import`, and calls of
/// `Date.now`, `Math.random`, `performance.now`, `Date()` or `new Date()` without
/// arguments in code (not in strings or comments) are rejected with their line.
pub fn parse_script(source: &str) -> Result<ParsedScript, ScriptError> {
    if source.len() > MAX_SCRIPT_BYTES {
        return Err(ScriptError::new(
            ScriptRule::Size,
            None,
            format!(
                "the script has {} bytes; the limit is {MAX_SCRIPT_BYTES}",
                source.len()
            ),
        ));
    }
    let tokens = scan(source)
        .map_err(|error| ScriptError::new(ScriptRule::Syntax, Some(error.line), error.message))?;
    let (meta, export_token) = extract_meta(source, &tokens)?;
    check_tokens(&tokens)?;
    let mut body = String::with_capacity(source.len());
    body.push_str(&source[..export_token.start]);
    body.push_str(&" ".repeat(export_token.text.len()));
    body.push_str(&source[export_token.end()..]);
    Ok(ParsedScript { meta, body })
}

const META_PREFIX: &str =
    "the script must start with `export const meta = { name, description, phases };`";

fn extract_meta<'a>(
    source: &'a str,
    tokens: &[Token<'a>],
) -> Result<(WorkflowMeta, Token<'a>), ScriptError> {
    let line = tokens.first().map(|token| token.line);
    let meta_error = |line: Option<usize>, message: &str| {
        ScriptError::new(ScriptRule::Meta, line, format!("{message}; {META_PREFIX}"))
    };
    let [export, keyword, name, equals, open, ..] = tokens else {
        return Err(meta_error(line, "missing meta export"));
    };
    if !export.is_ident("export")
        || !keyword.is_ident("const")
        || !name.is_ident("meta")
        || !equals.is_punct("=")
        || !open.is_punct("{")
    {
        return Err(meta_error(line, "missing meta export"));
    }
    let mut depth = 0usize;
    let mut close = None;
    for token in &tokens[4..] {
        if token.kind != TokenKind::Punct {
            continue;
        }
        match token.text {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    close = Some(*token);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return Err(meta_error(Some(open.line), "unterminated meta object"));
    };
    let literal = &source[open.start..close.end()];
    let value = parse_literal(literal).map_err(|error| {
        let line = open.line + literal[..error.offset].matches('\n').count();
        ScriptError::new(
            ScriptRule::Meta,
            Some(line),
            format!("meta must be a plain object literal: {}", error.message),
        )
    })?;
    let meta = validate_meta(value, open.line)?;
    Ok((meta, *export))
}

fn validate_meta(value: Value, line: usize) -> Result<WorkflowMeta, ScriptError> {
    let error = |message: String| ScriptError::new(ScriptRule::Meta, Some(line), message);
    let Value::Object(object) = value else {
        return Err(error("meta must be an object".to_string()));
    };
    let name = match object.get("name") {
        Some(Value::String(name)) => name.clone(),
        _ => return Err(error("meta.name must be a string".to_string())),
    };
    if !is_valid_name(&name) {
        return Err(error(format!(
            "meta.name `{name}` must match ^[A-Za-z0-9][A-Za-z0-9_-]{{0,63}}$"
        )));
    }
    let description = match object.get("description") {
        Some(Value::String(description)) if !description.trim().is_empty() => description.clone(),
        _ => {
            return Err(error(
                "meta.description must be a non-empty string".to_string(),
            ));
        }
    };
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(error(format!(
            "meta.description is longer than {MAX_DESCRIPTION_CHARS} characters"
        )));
    }
    let phases = match object.get("phases") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => {
            if items.len() > MAX_PHASES {
                return Err(error(format!(
                    "meta.phases has {} entries; the limit is {MAX_PHASES}",
                    items.len()
                )));
            }
            let mut phases = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(phase)
                        if !phase.trim().is_empty()
                            && phase.chars().count() <= MAX_PHASE_TITLE_CHARS =>
                    {
                        phases.push(phase.clone());
                    }
                    _ => {
                        return Err(error(format!(
                            "meta.phases must contain non-empty strings of at most {MAX_PHASE_TITLE_CHARS} characters"
                        )));
                    }
                }
            }
            phases
        }
        Some(_) => return Err(error("meta.phases must be an array of strings".to_string())),
    };
    Ok(WorkflowMeta {
        name,
        description,
        phases,
    })
}

/// Whether `name` matches `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && name.len() <= 64
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn check_tokens(tokens: &[Token<'_>]) -> Result<(), ScriptError> {
    let code: Vec<&Token<'_>> = tokens.iter().collect();
    for (index, token) in code.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let previous = index.checked_sub(1).map(|index| code[index]);
        if previous.is_some_and(|previous| previous.is_member_access()) {
            continue;
        }
        let next = code.get(index + 1).copied();
        let after_next = code.get(index + 2).copied();
        match token.text {
            "export" if index > 0 => {
                return Err(ScriptError::new(
                    ScriptRule::Export,
                    Some(token.line),
                    "only `export const meta` is allowed; remove other exports",
                ));
            }
            "import" => {
                return Err(ScriptError::new(
                    ScriptRule::Import,
                    Some(token.line),
                    "imports are not available in workflow scripts",
                ));
            }
            "Date" | "Math" | "performance" => {
                let member = match (next, after_next) {
                    (Some(access), Some(member))
                        if access.is_member_access() && member.kind == TokenKind::Ident =>
                    {
                        Some(member.text)
                    }
                    _ => None,
                };
                let forbidden = match (token.text, member) {
                    ("Date", Some("now")) => Some("Date.now()"),
                    ("Math", Some("random")) => Some("Math.random()"),
                    ("performance", Some("now")) => Some("performance.now()"),
                    ("Date", None) => {
                        date_constructor_without_arguments(previous, next, after_next)
                    }
                    _ => None,
                };
                if let Some(forbidden) = forbidden {
                    return Err(ScriptError::new(
                        ScriptRule::Determinism,
                        Some(token.line),
                        format!(
                            "{forbidden} is not allowed: workflow scripts must be deterministic; pass times or random values in args"
                        ),
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `new Date()`, `new Date` and `Date()` read the current time; `new Date(x)` does not.
fn date_constructor_without_arguments(
    previous: Option<&Token<'_>>,
    next: Option<&Token<'_>>,
    after_next: Option<&Token<'_>>,
) -> Option<&'static str> {
    let constructed = previous.is_some_and(|previous| previous.is_ident("new"));
    let called = next.is_some_and(|next| next.is_punct("("));
    let empty_call = called && after_next.is_some_and(|after| after.is_punct(")"));
    match (constructed, called) {
        (true, true) if empty_call => Some("new Date()"),
        (true, false) => Some("new Date"),
        (false, true) => Some("Date()"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "meta_tests.rs"]
mod tests;
