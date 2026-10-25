//! Parses a JavaScript literal (objects, arrays, strings, numbers, booleans and `null`)
//! into JSON, without executing code.
//!
//! The `meta` export of a workflow script is read this way (UC1): single quotes, unquoted
//! keys, trailing commas and comments are accepted; expressions are not.

use serde_json::Map;
use serde_json::Number;
use serde_json::Value;

/// A literal parsing failure at a byte offset of the parsed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiteralError {
    pub(crate) offset: usize,
    pub(crate) message: String,
}

const MAX_DEPTH: usize = 32;

/// Parses `text`, which must contain exactly one literal surrounded by optional
/// whitespace and comments.
pub(crate) fn parse_literal(text: &str) -> Result<Value, LiteralError> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
    };
    let value = parser.value(/*depth*/ 0)?;
    parser.skip_trivia()?;
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("unexpected text after the literal"));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> LiteralError {
        LiteralError {
            offset: self.pos,
            message: message.to_string(),
        }
    }

    fn skip_trivia(&mut self) -> Result<(), LiteralError> {
        loop {
            match self.bytes.get(self.pos) {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.pos += 1,
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'/') => {
                    while self.bytes.get(self.pos).is_some_and(|byte| *byte != b'\n') {
                        self.pos += 1;
                    }
                }
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'*') => {
                    let Some(end) = self.text[self.pos + 2..].find("*/") else {
                        return Err(self.error("unterminated block comment"));
                    };
                    self.pos += 2 + end + 2;
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, LiteralError> {
        if depth > MAX_DEPTH {
            return Err(self.error("literal is nested too deeply"));
        }
        self.skip_trivia()?;
        match self.bytes.get(self.pos) {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'\'' | b'"') => self.string().map(Value::String),
            Some(b'`') => self.template().map(Value::String),
            Some(b'-' | b'+' | b'.' | b'0'..=b'9') => self.number(),
            Some(_) => {
                let word = self.word().to_string();
                match word.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    "" => Err(self.error("expected a literal value")),
                    _ => {
                        self.pos -= word.len();
                        Err(self.error(&format!(
                            "`{word}` is not a literal; meta must contain only strings, numbers, booleans, null, arrays and objects"
                        )))
                    }
                }
            }
            None => Err(self.error("expected a literal value")),
        }
    }

    fn word(&mut self) -> &str {
        let start = self.pos;
        while let Some(ch) = self.text[self.pos..].chars().next() {
            if ch == '_' || ch == '$' || ch.is_alphanumeric() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        &self.text[start..self.pos]
    }

    fn object(&mut self, depth: usize) -> Result<Value, LiteralError> {
        self.pos += 1;
        let mut map = Map::new();
        loop {
            self.skip_trivia()?;
            match self.bytes.get(self.pos) {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(map));
                }
                Some(b'\'' | b'"') => {
                    let key = self.string()?;
                    self.entry(&mut map, key, depth)?;
                }
                Some(b'0'..=b'9') => {
                    let key = self.word().to_string();
                    self.entry(&mut map, key, depth)?;
                }
                Some(_) => {
                    let key = self.word().to_string();
                    if key.is_empty() {
                        return Err(self.error("expected a property name"));
                    }
                    self.entry(&mut map, key, depth)?;
                }
                None => return Err(self.error("unterminated object literal")),
            }
            self.skip_trivia()?;
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err(self.error("expected `,` or `}` in object literal")),
            }
        }
    }

    fn entry(
        &mut self,
        map: &mut Map<String, Value>,
        key: String,
        depth: usize,
    ) -> Result<(), LiteralError> {
        self.skip_trivia()?;
        if self.bytes.get(self.pos) != Some(&b':') {
            return Err(self.error(&format!("expected `:` after property `{key}`")));
        }
        self.pos += 1;
        let value = self.value(depth + 1)?;
        if map.insert(key.clone(), value).is_some() {
            return Err(self.error(&format!("duplicate property `{key}`")));
        }
        Ok(())
    }

    fn array(&mut self, depth: usize) -> Result<Value, LiteralError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.bytes.get(self.pos) {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                Some(b',') => return Err(self.error("array holes are not allowed")),
                None => return Err(self.error("unterminated array literal")),
                Some(_) => items.push(self.value(depth + 1)?),
            }
            self.skip_trivia()?;
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {}
                _ => return Err(self.error("expected `,` or `]` in array literal")),
            }
        }
    }

    fn string(&mut self) -> Result<String, LiteralError> {
        let quote = self.bytes[self.pos];
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(ch) = self.text[self.pos..].chars().next() else {
                return Err(self.error("unterminated string literal"));
            };
            match ch {
                '\\' => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                '\n' => return Err(self.error("unterminated string literal")),
                _ if ch as u32 == u32::from(quote) => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn template(&mut self) -> Result<String, LiteralError> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(ch) = self.text[self.pos..].chars().next() else {
                return Err(self.error("unterminated template literal"));
            };
            match ch {
                '\\' => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                '`' => {
                    self.pos += 1;
                    return Ok(out);
                }
                '$' if self.bytes.get(self.pos + 1) == Some(&b'{') => {
                    return Err(self.error("template literals in meta must not contain `${}`"));
                }
                _ => {
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), LiteralError> {
        let Some(ch) = self.text[self.pos..].chars().next() else {
            return Err(self.error("unterminated escape sequence"));
        };
        self.pos += ch.len_utf8();
        match ch {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'v' => out.push('\u{b}'),
            '0' => out.push('\0'),
            '\n' => {}
            'x' => {
                let code = self.hex_digits(2)?;
                out.push(char::from_u32(code).ok_or_else(|| self.error("invalid escape"))?);
            }
            'u' if self.bytes.get(self.pos) == Some(&b'{') => {
                let Some(end) = self.text[self.pos..].find('}') else {
                    return Err(self.error("invalid unicode escape"));
                };
                let code = u32::from_str_radix(&self.text[self.pos + 1..self.pos + end], 16)
                    .map_err(|_| self.error("invalid unicode escape"))?;
                self.pos += end + 1;
                out.push(char::from_u32(code).ok_or_else(|| self.error("invalid unicode escape"))?);
            }
            'u' => {
                let code = self.hex_digits(4)?;
                if (0xD800..0xDC00).contains(&code) && self.text[self.pos..].starts_with("\\u") {
                    self.pos += 2;
                    let low = self.hex_digits(4)?;
                    let combined = 0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00));
                    out.push(
                        char::from_u32(combined)
                            .ok_or_else(|| self.error("invalid surrogate pair"))?,
                    );
                } else {
                    out.push(
                        char::from_u32(code).ok_or_else(|| self.error("invalid unicode escape"))?,
                    );
                }
            }
            other => out.push(other),
        }
        Ok(())
    }

    fn hex_digits(&mut self, count: usize) -> Result<u32, LiteralError> {
        let digits = self
            .text
            .get(self.pos..self.pos + count)
            .ok_or_else(|| self.error("invalid hexadecimal escape"))?;
        let code = u32::from_str_radix(digits, 16)
            .map_err(|_| self.error("invalid hexadecimal escape"))?;
        self.pos += count;
        Ok(code)
    }

    fn number(&mut self) -> Result<Value, LiteralError> {
        let start = self.pos;
        if matches!(self.bytes.get(self.pos), Some(b'-' | b'+')) {
            self.pos += 1;
        }
        while let Some(byte) = self.bytes.get(self.pos) {
            let exponent_sign = matches!(byte, b'+' | b'-')
                && matches!(self.bytes.get(self.pos - 1), Some(b'e' | b'E'));
            if byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'_') || exponent_sign {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = self.text[start..self.pos].replace('_', "");
        let text = text.strip_prefix('+').unwrap_or(&text);
        if let Ok(integer) = text.parse::<i64>() {
            return Ok(Value::Number(integer.into()));
        }
        text.parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| LiteralError {
                offset: start,
                message: format!("invalid number `{text}`"),
            })
    }
}

#[cfg(test)]
#[path = "literal_tests.rs"]
mod tests;
