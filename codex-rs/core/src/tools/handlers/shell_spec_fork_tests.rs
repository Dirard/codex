use pretty_assertions::assert_eq;

use super::unified_exec_output_schema;

/// Live recheck 4 R12: the Code Mode result type names the notification fields, described, only
/// while exit notifications are on; without them the schema is upstream's.
#[test]
fn the_notification_fields_are_declared_only_with_exit_notifications() {
    assert_eq!(
        unified_exec_output_schema(/*exit_notifications*/ false),
        super::super::unified_exec_output_schema()
    );
    let schema = unified_exec_output_schema(/*exit_notifications*/ true);
    for name in ["exit_notification", "output_watch", "notes"] {
        assert!(
            schema["properties"][name]["description"]
                .as_str()
                .is_some_and(|description| !description.is_empty()),
            "{name}: {schema}"
        );
    }
    assert_eq!(schema["properties"]["notes"]["type"], "array");
    assert_eq!(schema["properties"]["notes"]["items"]["type"], "string");
}
