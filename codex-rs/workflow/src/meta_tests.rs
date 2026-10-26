use super::*;
use pretty_assertions::assert_eq;

const VALID: &str = "// Find bugs.\nexport const meta = {\n  name: 'find-bugs',\n  description: 'Fan-out search',\n  phases: ['Find', 'Verify',],\n};\nreturn await agent('hi');\n";

/// R1.2: a meta literal with single quotes and a trailing comma parses, and the body
/// keeps its line layout without the `export` keyword.
#[test]
fn valid_meta_is_extracted_and_export_is_blanked() {
    let parsed = parse_script(VALID).expect("valid script");
    assert_eq!(
        parsed.meta,
        WorkflowMeta {
            name: "find-bugs".to_string(),
            description: "Fan-out search".to_string(),
            phases: vec!["Find".to_string(), "Verify".to_string()],
        }
    );
    assert_eq!(
        parsed.body,
        VALID.replacen("export const", "       const", 1)
    );
    assert_eq!(parsed.body.lines().count(), VALID.lines().count());
}

/// R1.3: `Math.random()` in code is rejected with its line, the same text in strings,
/// templates, regular expressions and comments is not.
#[test]
fn determinism_rule_ignores_strings_and_comments() {
    let script = format!(
        "{}\n// Math.random()\nconst a = 'Date.now()';\nconst b = `new Date() ${{1}}`;\nconst c = /Math.random/;\nconst d = new Date(0);\nconst e = x.Math.random;\n",
        VALID.trim_end()
    );
    parse_script(&script).expect("only strings and comments mention forbidden calls");

    for (line, forbidden) in [
        ("const r = Math.random();", "Math.random()"),
        ("const t = Date.now();", "Date.now()"),
        ("const t = performance?.now();", "performance.now()"),
        ("const t = new Date();", "new Date()"),
        ("const t = new Date;", "new Date"),
        ("const t = Date();", "Date()"),
        // Without `new`, Date returns the current time whatever its arguments are.
        ("const t = Date(1);", "Date()"),
        ("const t = `${Date.now()}`;", "Date.now()"),
    ] {
        let script = format!("{}\n\n{line}\n", VALID.trim_end());
        let error = parse_script(&script).unwrap_err();
        assert_eq!(
            (error.rule, error.line),
            (ScriptRule::Determinism, Some(9)),
            "{line}"
        );
        assert!(error.message.starts_with(forbidden), "{line}: {error}");
    }
}

/// R1.3: missing meta, a second export, imports and oversized scripts are rejected.
#[test]
fn structural_rules_are_enforced() {
    let error = parse_script("const x = 1;\n").unwrap_err();
    assert_eq!(error.rule, ScriptRule::Meta);

    let error = parse_script(&format!("{VALID}export const y = 1;\n")).unwrap_err();
    assert_eq!((error.rule, error.line), (ScriptRule::Export, Some(8)));

    let error = parse_script(&format!("{VALID}import x from 'y';\n")).unwrap_err();
    assert_eq!((error.rule, error.line), (ScriptRule::Import, Some(8)));

    let error = parse_script(&format!("{VALID}const m = await import('y');\n")).unwrap_err();
    assert_eq!(error.rule, ScriptRule::Import);

    let error = parse_script(&format!("{VALID}{}", " ".repeat(MAX_SCRIPT_BYTES))).unwrap_err();
    assert_eq!(error.rule, ScriptRule::Size);

    let error = parse_script(&format!("{VALID}const s = 'open;\n")).unwrap_err();
    assert_eq!((error.rule, error.line), (ScriptRule::Syntax, Some(8)));

    assert_eq!(
        check_args_size(&"x".repeat(MAX_ARGS_BYTES + 1))
            .unwrap_err()
            .rule,
        ScriptRule::ArgsSize
    );
}

#[test]
fn meta_fields_are_validated() {
    for (meta, expected) in [
        ("{ name: 'bad name', description: 'd' }", "meta.name"),
        ("{ name: 'ok', description: '  ' }", "meta.description"),
        (
            "{ name: 'ok', description: 'd', phases: [''] }",
            "meta.phases",
        ),
        (
            "{ name: 'ok', description: 'd', phases: 'x' }",
            "meta.phases",
        ),
        (
            "{ name: 'ok', description: compute() }",
            "plain object literal",
        ),
    ] {
        let error = parse_script(&format!("export const meta = {meta};\n")).unwrap_err();
        assert_eq!(error.rule, ScriptRule::Meta, "{meta}");
        assert!(error.message.contains(expected), "{meta}: {error}");
    }
    let phases = (0..=MAX_PHASES)
        .map(|index| format!("'p{index}'"))
        .collect::<Vec<_>>()
        .join(",");
    let error = parse_script(&format!(
        "export const meta = {{ name: 'ok', description: 'd', phases: [{phases}] }};\n"
    ))
    .unwrap_err();
    assert!(error.message.contains("limit is 32"), "{error}");
}

#[test]
fn names_follow_the_identifier_rule() {
    assert!(is_valid_name("a"));
    assert!(is_valid_name("Find_bugs-2"));
    assert!(!is_valid_name(""));
    assert!(!is_valid_name("-x"));
    assert!(!is_valid_name("a.b"));
    assert!(!is_valid_name(&"a".repeat(65)));
}
