use super::*;
use pretty_assertions::assert_eq;

fn kinds(source: &str) -> Vec<(TokenKind, String, usize)> {
    scan(source)
        .expect("scan")
        .into_iter()
        .map(|token| (token.kind, token.text.to_string(), token.line))
        .collect()
}

#[test]
fn comments_are_dropped_and_strings_are_opaque() {
    assert_eq!(
        kinds("// Math.random()\nconst s = 'Date.now()'; /* x\n y */ f(s)"),
        vec![
            (TokenKind::Ident, "const".to_string(), 2),
            (TokenKind::Ident, "s".to_string(), 2),
            (TokenKind::Punct, "=".to_string(), 2),
            (TokenKind::String, "'Date.now()'".to_string(), 2),
            (TokenKind::Punct, ";".to_string(), 2),
            (TokenKind::Ident, "f".to_string(), 3),
            (TokenKind::Punct, "(".to_string(), 3),
            (TokenKind::Ident, "s".to_string(), 3),
            (TokenKind::Punct, ")".to_string(), 3),
        ]
    );
}

#[test]
fn template_expressions_are_code() {
    let tokens = kinds("`a ${ {b: 1}.b } c\n${x}`");
    assert_eq!(
        tokens,
        vec![
            (TokenKind::Template, "a ".to_string(), 1),
            (TokenKind::Punct, "{".to_string(), 1),
            (TokenKind::Ident, "b".to_string(), 1),
            (TokenKind::Punct, ":".to_string(), 1),
            (TokenKind::Number, "1".to_string(), 1),
            (TokenKind::Punct, "}".to_string(), 1),
            (TokenKind::Punct, ".".to_string(), 1),
            (TokenKind::Ident, "b".to_string(), 1),
            (TokenKind::Template, " c\n".to_string(), 1),
            (TokenKind::Ident, "x".to_string(), 2),
            (TokenKind::Template, "".to_string(), 2),
        ]
    );
}

#[test]
fn regex_literals_and_division_are_distinguished() {
    let tokens = kinds("const r = /Math.random()/g; const d = a / b / c;");
    assert!(tokens.contains(&(TokenKind::Regex, "/Math.random()/g".to_string(), 1)));
    assert_eq!(
        tokens
            .iter()
            .filter(|(kind, text, _)| *kind == TokenKind::Punct && text == "/")
            .count(),
        2
    );
}

#[test]
fn optional_chaining_and_spread_are_single_tokens() {
    let tokens = kinds("a?.b ?.5 : [...c]");
    assert!(tokens.contains(&(TokenKind::Punct, "?.".to_string(), 1)));
    assert!(tokens.contains(&(TokenKind::Number, ".5".to_string(), 1)));
    assert!(tokens.contains(&(TokenKind::Punct, "...".to_string(), 1)));
}

#[test]
fn unterminated_literals_report_their_line() {
    assert_eq!(
        scan("x\n'abc").unwrap_err(),
        ScanError {
            line: 2,
            message: "unterminated string literal".to_string(),
        }
    );
    assert_eq!(scan("x\n/* never").unwrap_err().line, 2);
    assert!(scan("`abc ${").is_err());
}
