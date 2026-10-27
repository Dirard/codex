//! Fork: cached `readOnlyHint`s (review 2026-10-27, P2).

use std::collections::HashSet;
use std::sync::Arc;

use codex_config::McpServerConfig;
use codex_exec_server_test_support::environment_manager_without_environments;
use codex_protocol::mcp::ClientMcpExtensions;
use pretty_assertions::assert_eq;
use rmcp::model::ElicitationCapability;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use rmcp::model::ToolAnnotations;

use crate::ToolInfo;
use crate::runtime::McpRuntimeContext;
use crate::tool_catalog_cache::McpToolCatalogCache;

fn tool(name: &str, read_only_hint: Option<bool>) -> ToolInfo {
    let mut tool = Tool::new(
        name.to_string(),
        String::new(),
        Arc::new(JsonObject::default()),
    );
    tool.annotations = read_only_hint.map(|read_only| ToolAnnotations::new().read_only(read_only));
    ToolInfo {
        server_name: "docs".to_string(),
        supports_parallel_tool_calls: false,
        server_origin: None,
        callable_name: name.to_string(),
        callable_namespace: "docs".to_string(),
        namespace_description: None,
        tool,
        openai_file_input_optional_fields: Default::default(),
        connector_id: None,
        connector_name: None,
        plugin_display_names: Vec::new(),
    }
}

/// The cache still drops annotations from the tools it hands out, but keeps the names of the
/// tools the live list annotated `readOnlyHint: true`, for as long as the catalog is usable.
#[test]
fn cached_catalog_keeps_read_only_names_without_annotations() {
    let runtime_context = McpRuntimeContext::new(
        Arc::new(environment_manager_without_environments()),
        std::env::temp_dir(),
    );
    let config: McpServerConfig =
        serde_json::from_value(serde_json::json!({ "command": "docs-mcp" })).expect("config");
    let cache = McpToolCatalogCache::default();
    let context = cache
        .context(
            "docs",
            &config,
            &runtime_context,
            /*resolved_environment*/ None,
            (
                &ElicitationCapability::default(),
                &ClientMcpExtensions::default(),
            ),
            /*connection_identity*/ None,
        )
        .expect("stdio servers are cacheable");
    assert_eq!(context.fork_read_only_tool_names(), None);

    context.publish_if_newest(
        context.begin_fetch(),
        &[
            tool("peek", Some(true)),
            tool("poke", Some(false)),
            tool("plain", /*read_only_hint*/ None),
        ],
        /*meta*/ None,
    );
    let tools = context.current_tools().expect("cached tools");
    assert!(tools.iter().all(|tool| tool.tool.annotations.is_none()));
    assert_eq!(
        context.fork_read_only_tool_names().as_deref(),
        Some(&HashSet::from(["peek".to_string()]))
    );

    // A context that tracks tool-list changes cannot reuse this catalog, nor its hints.
    let mut tracking = context.clone();
    tracking.track_tool_list_changes = true;
    assert_eq!(tracking.fork_read_only_tool_names(), None);

    context.disable();
    assert_eq!(context.fork_read_only_tool_names(), None);
}
