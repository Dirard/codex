//! A small JavaScript scanner that separates code from string, template, comment and
//! regular-expression text, so static checks only look at code.
//!
//! It is deliberately approximate: the runtime checks in the prelude stay authoritative
//! (`specs/dynamic-workflow-2026-10-25.md`, UC1 risk «false positives»).

/// Kind of a scanned token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident,
    Number,
    Punct,
    /// A quoted string literal.
    String,
    /// Literal text of a template, without its `${...}` expressions.
    Template,
    Regex,
}

/// One token with its byte span and 1-based line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub(crate) kind: TokenKind,
    pub(crate) text: &'a str,
    pub(crate) start: usize,
    pub(crate) line: usize,
}

impl Token<'_> {
    pub(crate) fn end(&self) -> usize {
        self.start + self.text.len()
    }

    pub(crate) fn is_ident(&self, name: &str) -> bool {
        self.kind == TokenKind::Ident && self.text == name
    }

    pub(crate) fn is_punct(&self, punct: &str) -> bool {
        self.kind == TokenKind::Punct && self.text == punct
    }

    /// Member access: `.` or optional chaining `?.`.
    pub(crate) fn is_member_access(&self) -> bool {
        self.is_punct(".") || self.is_punct("?.")
    }
}

/// A scanning failure, such as an unterminated string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScanError {
    pub(crate) line: usize,
    pub(crate) message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Nesting {
    Brace,
    TemplateExpression,
}

const REGEX_PRECEDING_KEYWORDS: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
];

struct Scanner<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    tokens: Vec<Token<'a>>,
    nesting: Vec<Nesting>,
}

/// Splits `source` into code tokens, string-like tokens and nothing for comments.
pub(crate) fn scan(source: &str) -> Result<Vec<Token<'_>>, ScanError> {
    let mut scanner = Scanner {
        source,
        bytes: source.as_bytes(),
        pos: 0,
        line: 1,
        tokens: Vec::new(),
        nesting: Vec::new(),
    };
    scanner.run()?;
    Ok(scanner.tokens)
}

impl<'a> Scanner<'a> {
    fn run(&mut self) -> Result<(), ScanError> {
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            match byte {
                b'\n' => {
                    self.line += 1;
                    self.pos += 1;
                }
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'/' if self.peek(1) == Some(b'/') => self.skip_line_comment(),
                b'/' if self.peek(1) == Some(b'*') => self.skip_block_comment()?,
                b'/' if self.regex_allowed() => self.scan_regex()?,
                b'\'' | b'"' => self.scan_string(byte)?,
                b'`' => {
                    self.pos += 1;
                    self.scan_template_chunk()?;
                }
                b'{' => {
                    self.nesting.push(Nesting::Brace);
                    self.push(TokenKind::Punct, self.pos, self.pos + 1);
                    self.pos += 1;
                }
                b'}' => {
                    if self.nesting.pop() == Some(Nesting::TemplateExpression) {
                        self.pos += 1;
                        self.scan_template_chunk()?;
                    } else {
                        self.push(TokenKind::Punct, self.pos, self.pos + 1);
                        self.pos += 1;
                    }
                }
                b'0'..=b'9' => self.scan_number(),
                b'.' if self.peek(1).is_some_and(|next| next.is_ascii_digit()) => {
                    self.scan_number();
                }
                b'?' if self.peek(1) == Some(b'.')
                    && !self.peek(2).is_some_and(|next| next.is_ascii_digit()) =>
                {
                    self.push(TokenKind::Punct, self.pos, self.pos + 2);
                    self.pos += 2;
                }
                b'.' if self.peek(1) == Some(b'.') && self.peek(2) == Some(b'.') => {
                    self.push(TokenKind::Punct, self.pos, self.pos + 3);
                    self.pos += 3;
                }
                _ if is_ident_start(self.char_at(self.pos)) => self.scan_ident(),
                _ => {
                    let len = self.char_at(self.pos).len_utf8();
                    self.push(TokenKind::Punct, self.pos, self.pos + len);
                    self.pos += len;
                }
            }
        }
        if self.nesting.contains(&Nesting::TemplateExpression) {
            return Err(self.error("unterminated template literal"));
        }
        Ok(())
    }

    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn char_at(&self, pos: usize) -> char {
        self.source[pos..].chars().next().unwrap_or('\0')
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        let line = self.line - self.source[start..end].matches('\n').count();
        self.tokens.push(Token {
            kind,
            text: &self.source[start..end],
            start,
            line,
        });
    }

    fn error(&self, message: &str) -> ScanError {
        ScanError {
            line: self.line,
            message: message.to_string(),
        }
    }

    fn skip_line_comment(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), ScanError> {
        let start_line = self.line;
        self.pos += 2;
        while self.pos < self.bytes.len() {
            if self.bytes[self.pos] == b'*' && self.peek(1) == Some(b'/') {
                self.pos += 2;
                return Ok(());
            }
            if self.bytes[self.pos] == b'\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        Err(ScanError {
            line: start_line,
            message: "unterminated block comment".to_string(),
        })
    }

    fn regex_allowed(&self) -> bool {
        match self.tokens.last() {
            None => true,
            Some(token) => match token.kind {
                TokenKind::Number | TokenKind::String | TokenKind::Template | TokenKind::Regex => {
                    false
                }
                TokenKind::Ident => REGEX_PRECEDING_KEYWORDS.contains(&token.text),
                TokenKind::Punct => !matches!(token.text, ")" | "]" | "}"),
            },
        }
    }

    fn scan_regex(&mut self) -> Result<(), ScanError> {
        let start = self.pos;
        self.pos += 1;
        let mut in_class = false;
        loop {
            let Some(&byte) = self.bytes.get(self.pos) else {
                return Err(self.error("unterminated regular expression"));
            };
            match byte {
                b'\n' => return Err(self.error("unterminated regular expression")),
                b'\\' => self.pos += 2,
                b'[' => {
                    in_class = true;
                    self.pos += 1;
                }
                b']' => {
                    in_class = false;
                    self.pos += 1;
                }
                b'/' if !in_class => {
                    self.pos += 1;
                    break;
                }
                _ => self.pos += 1,
            }
        }
        while self.pos < self.bytes.len() && is_ident_part(self.char_at(self.pos)) {
            self.pos += self.char_at(self.pos).len_utf8();
        }
        self.push(TokenKind::Regex, start, self.pos.min(self.bytes.len()));
        Ok(())
    }

    fn scan_string(&mut self, quote: u8) -> Result<(), ScanError> {
        let start = self.pos;
        let start_line = self.line;
        self.pos += 1;
        loop {
            let Some(&byte) = self.bytes.get(self.pos) else {
                return Err(ScanError {
                    line: start_line,
                    message: "unterminated string literal".to_string(),
                });
            };
            match byte {
                b'\\' => {
                    if self.peek(1) == Some(b'\n') {
                        self.line += 1;
                    }
                    self.pos += 2;
                }
                b'\n' => {
                    return Err(ScanError {
                        line: start_line,
                        message: "unterminated string literal".to_string(),
                    });
                }
                _ if byte == quote => {
                    self.pos += 1;
                    break;
                }
                _ => self.pos += 1,
            }
        }
        self.push(TokenKind::String, start, self.pos);
        Ok(())
    }

    /// Scans template text after a backtick or after the `}` closing an expression.
    fn scan_template_chunk(&mut self) -> Result<(), ScanError> {
        let start = self.pos;
        let start_line = self.line;
        loop {
            let Some(&byte) = self.bytes.get(self.pos) else {
                return Err(ScanError {
                    line: start_line,
                    message: "unterminated template literal".to_string(),
                });
            };
            match byte {
                b'\\' => {
                    if self.peek(1) == Some(b'\n') {
                        self.line += 1;
                    }
                    self.pos += 2;
                }
                b'\n' => {
                    self.line += 1;
                    self.pos += 1;
                }
                b'`' => {
                    self.push(TokenKind::Template, start, self.pos);
                    self.pos += 1;
                    return Ok(());
                }
                b'$' if self.peek(1) == Some(b'{') => {
                    self.push(TokenKind::Template, start, self.pos);
                    self.pos += 2;
                    self.nesting.push(Nesting::TemplateExpression);
                    return Ok(());
                }
                _ => self.pos += 1,
            }
        }
    }

    fn scan_number(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            let exponent_sign = matches!(byte, b'+' | b'-')
                && matches!(self.bytes[self.pos - 1], b'e' | b'E')
                && !self.source[start..self.pos].starts_with("0x");
            if byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || exponent_sign {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.push(TokenKind::Number, start, self.pos);
    }

    fn scan_ident(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_ident_part(self.char_at(self.pos)) {
            self.pos += self.char_at(self.pos).len_utf8();
        }
        self.push(TokenKind::Ident, start, self.pos);
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_alphabetic()
}

fn is_ident_part(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_alphanumeric()
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
