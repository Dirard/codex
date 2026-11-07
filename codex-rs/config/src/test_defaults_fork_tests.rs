use super::*;
use pretty_assertions::assert_eq;

fn default_on_keys() -> Vec<&'static str> {
    fork_default_on_features()
        .into_iter()
        .map(codex_features::Feature::key)
        .collect()
}

/// Whether each default-on fork feature is on after `text` is pinned and loaded.
fn resolved(text: &str) -> Vec<(&'static str, bool)> {
    let pinned = pin_default_on_fork_features_off(text).expect("valid TOML");
    let value = toml::from_str::<toml::Value>(&pinned).expect("pinned TOML parses");
    let features: codex_features::FeaturesToml = value
        .get("features")
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(Default::default()))
        .try_into()
        .expect("features table");
    let mut resolved = codex_features::Features::with_defaults();
    resolved.apply_map(&features.entries());
    fork_default_on_features()
        .into_iter()
        .map(|feature| (feature.key(), resolved.enabled(feature)))
        .collect()
}

/// Fork-features-default-on R6.1, R6.2: the app-server writer turns off every default-on fork
/// feature the document does not set; a value the document sets is kept, and a settings table
/// without `enabled` gets `enabled = false` and keeps its settings. The boolean-only `workflows`
/// is pinned only inside a `[features]` table the document defines.
#[test]
fn pins_only_the_default_on_fork_features_a_document_does_not_set() {
    let keys = default_on_keys();
    assert!(keys.contains(&"workflows"), "{keys:?}");
    assert!(keys.contains(&"exec_exit_notifications"), "{keys:?}");

    assert_eq!(
        resolved(""),
        keys.iter()
            .map(|key| (*key, *key == "workflows"))
            .collect::<Vec<_>>()
    );

    let configured = "model = \"m\"\n\n[features]\nworkflows = true\nmulti_agent_v2 = true\n\n[features.exec_exit_notifications]\nexec_wait_timeout_ms = 1\n\n[model_providers.mock]\nname = \"mock\"\n";
    assert_eq!(
        resolved(configured),
        keys.iter()
            .map(|key| (*key, *key == "workflows"))
            .collect::<Vec<_>>()
    );
    let pinned = toml::from_str::<toml::Value>(
        &pin_default_on_fork_features_off(configured).expect("valid TOML"),
    )
    .expect("pinned TOML parses");
    assert_eq!(
        pinned["features"]["exec_exit_notifications"],
        toml::from_str::<toml::Value>("enabled = false\nexec_wait_timeout_ms = 1\n").unwrap()
    );
    assert_eq!(
        pinned["features"]["multi_agent_v2"],
        toml::Value::Boolean(true)
    );

    let explicit_off = "[features]\nmulti_agent_v2 = true\n";
    assert_eq!(
        resolved(explicit_off),
        keys.iter().map(|key| (*key, false)).collect::<Vec<_>>()
    );

    // A settings table that sets `enabled` is kept.
    let enabled = "[features.multi_agent_v2]\nenabled = true\n\n[features.exec_exit_notifications]\nenabled = true\n";
    assert_eq!(
        resolved(enabled),
        keys.iter()
            .map(|key| (
                *key,
                matches!(*key, "workflows" | "exec_exit_notifications")
            ))
            .collect::<Vec<_>>()
    );
}

/// A test may append its own `[features]` table to what the app-server writer produced.
#[test]
fn a_test_can_append_a_features_table_after_the_pins() {
    for text in [
        "model = \"m\"\n",
        "[features.multi_agent_v2]\nenabled = true\n",
    ] {
        let pinned = pin_default_on_fork_features_off(text).expect("valid TOML");
        let appended = format!("{pinned}\n[features]\nplugins = true\n");
        let value = toml::from_str::<toml::Value>(&appended)
            .unwrap_or_else(|err| panic!("{appended}: {err}"));
        assert_eq!(value["features"]["plugins"], toml::Value::Boolean(true));
    }
}

/// The pins keep the rest of the document, its text included, and the packaged defaults for the
/// harnesses are the embedded ones with the pins.
#[test]
fn pins_keep_the_document_and_the_packaged_defaults() {
    let text = "# comment\nmodel = \"m\" # trailing\n\n[model_providers.mock]\nname = \"mock\"\n";
    let pinned = pin_default_on_fork_features_off(text).expect("valid TOML");
    assert!(pinned.starts_with(text), "{pinned}");

    let packaged = toml::from_str::<toml::Value>(&upstream_test_packaged_defaults_toml())
        .expect("packaged defaults parse");
    let embedded = toml::from_str::<toml::Value>(EMBEDDED_PACKAGED_DEFAULTS).expect("embedded");
    let mut without_features = packaged.clone();
    without_features
        .as_table_mut()
        .expect("table")
        .remove("features");
    assert_eq!(without_features, embedded);
    for key in default_on_keys() {
        assert_eq!(
            packaged["features"][key],
            toml::Value::Boolean(false),
            "{key}"
        );
    }
}

/// The keys a `-c` harness turns off: every default-on fork feature the document does not set,
/// including one whose settings table has no `enabled`.
#[test]
fn unset_keys_skip_what_a_document_sets() {
    let keys = default_on_keys();
    assert_eq!(default_on_fork_features_unset_in("").expect("valid"), keys);
    let unset = default_on_fork_features_unset_in(
        "[features]\nworkflows = false\n\n[features.exec_exit_notifications]\nexec_wait_timeout_ms = 1\n\n[features.explorer_profile]\nenabled = true\n",
    )
    .expect("valid");
    assert_eq!(
        unset,
        keys.iter()
            .copied()
            .filter(|key| !matches!(*key, "workflows" | "explorer_profile"))
            .collect::<Vec<_>>()
    );
    assert!(default_on_fork_features_unset_in("[features").is_err());
}

/// The TUI harness overrides turn off exactly the default-on fork features.
#[test]
fn feature_overrides_turn_off_the_default_on_fork_features() {
    assert_eq!(
        upstream_test_feature_overrides(),
        default_on_keys()
            .into_iter()
            .map(|key| (format!("features.{key}"), toml::Value::Boolean(false)))
            .collect::<Vec<_>>()
    );
}
