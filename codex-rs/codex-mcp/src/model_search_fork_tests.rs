use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use codex_protocol::mcp::McpServerInfo;
use codex_rmcp_client::InProcessTransportFactory;
use codex_rmcp_client::RmcpClient;
use futures::FutureExt;
use pretty_assertions::assert_eq;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use serde_json::json;
use tokio::io::DuplexStream;

use crate::CODEX_APPS_MCP_SERVER_NAME;
use crate::McpBinding;
use crate::McpServerPermissions;
use crate::McpServerRegistration;
use crate::PreparedMcpCall;
use crate::ResolvedMcpCatalog;
use crate::SEARCH_CONNECTOR;
use crate::SearchDiscovery;
use crate::ToolInfo;
use crate::binding_clients::McpBindingClients;
use crate::client_tool_catalog::ClientToolCatalog;
use crate::connection_manager::McpConnectionSet;
use crate::rmcp_client::ManagedClient;
use crate::server::McpServerMetadata;

const SEARCH_TOOL: &str = "search_service.web_run";

struct IdleTransport;

impl InProcessTransportFactory for IdleTransport {
    fn open(&self) -> futures::future::BoxFuture<'static, io::Result<DuplexStream>> {
        async {
            let (client_stream, _server_stream) = tokio::io::duplex(1);
            Ok(client_stream)
        }
        .boxed()
    }
}

async fn apps_transport() -> Arc<RmcpClient> {
    Arc::new(
        RmcpClient::new_in_process_client(Arc::new(IdleTransport))
            .await
            .expect("create in-process Apps client"),
    )
}

fn search_tool(description: &str) -> ToolInfo {
    ToolInfo {
        server_name: CODEX_APPS_MCP_SERVER_NAME.to_string(),
        supports_parallel_tool_calls: false,
        server_origin: None,
        callable_name: "_web_run".to_string(),
        callable_namespace: "mcp__codex_apps__search_service".to_string(),
        namespace_description: None,
        tool: Tool::new(
            SEARCH_TOOL.to_string(),
            description.to_string(),
            Arc::new(JsonObject::default()),
        ),
        openai_file_input_optional_fields: Default::default(),
        connector_id: Some(SEARCH_CONNECTOR.to_string()),
        connector_name: Some("search_service".to_string()),
        plugin_display_names: Vec::new(),
    }
}

/// What a scoped `tools/list` returned for the turn: a pinned declaration.
fn listed_search_tool() -> ToolInfo {
    let mut tool = search_tool("scoped search");
    tool.namespace_description = Some("Scoped search".to_string());
    let mut meta = rmcp::model::MetaObject::new();
    meta.insert(
        "_codex_apps".to_string(),
        json!({"invocation_context": {"tool_type": "SonicBrowserToolV99"}}),
    );
    tool.tool.meta = Some(meta);
    tool
}

fn docs_tool() -> ToolInfo {
    ToolInfo {
        server_name: "docs".to_string(),
        callable_name: "search".to_string(),
        callable_namespace: "mcp__docs".to_string(),
        connector_id: None,
        connector_name: None,
        tool: Tool::new(
            "search".to_string(),
            "docs".to_string(),
            Arc::new(JsonObject::default()),
        ),
        ..search_tool("docs")
    }
}

/// A step binding whose Apps calls go through `transport`; other tools come from a cache.
async fn step_binding(transport: &Arc<RmcpClient>, tools: Vec<ToolInfo>) -> Arc<McpBinding> {
    let apps_tools: Vec<_> = tools
        .iter()
        .filter(|tool| tool.server_name == CODEX_APPS_MCP_SERVER_NAME)
        .cloned()
        .collect();
    let tool_catalog = Arc::new(ClientToolCatalog::new(
        apps_tools.clone(),
        /*meta*/ None,
        /*updates*/ None,
    ));
    // Every capture wraps the shared transport in its own managed client.
    let managed_client = Arc::new(ManagedClient {
        connection_sharing: Default::default(),
        shared_elicitation: None,
        _auth_change_notifications: None,
        client: Arc::clone(transport),
        server_info: McpServerInfo {
            name: CODEX_APPS_MCP_SERVER_NAME.to_string(),
            title: None,
            version: "1.0.0".to_string(),
            description: None,
            icons: None,
            website_url: None,
        },
        tool_catalog: Arc::clone(&tool_catalog),
        tool_timeout: None,
        server_instructions: None,
        server_supports_sandbox_state_meta_capability: false,
        client_supports_network_policy_meta_capability: false,
        codex_apps_tools_cache_context: None,
    });
    let mut config = crate::mcp::tests::test_mcp_config(std::env::temp_dir());
    let mut catalog = ResolvedMcpCatalog::builder();
    catalog.register(McpServerRegistration::from_hosted_apps(
        "host",
        /*contribution_order*/ 0,
        serde_json::from_value(json!({"url": "https://apps.example/mcp"}))
            .expect("Apps MCP config"),
    ));
    config.mcp_server_catalog = catalog.build();
    config.server_permissions.insert(
        CODEX_APPS_MCP_SERVER_NAME.to_string(),
        McpServerPermissions::Thread,
    );
    let config = Arc::new(config);
    let connections = Arc::new(McpConnectionSet::empty(/*prefix_mcp_tool_names*/ true));
    let snapshot = tool_catalog.read(Arc::new).await;
    let calls = apps_tools
        .into_iter()
        .map(|tool| {
            let call = PreparedMcpCall::new(
                Arc::clone(&connections),
                Arc::clone(&managed_client),
                Arc::clone(&config),
                Arc::clone(&snapshot),
                tool.clone(),
                McpServerMetadata {
                    environment_id: String::new(),
                    pollutes_memory: false,
                    origin: None,
                    supports_parallel_tool_calls: false,
                    default_tools_approval_mode: None,
                    tool_approval_modes: HashMap::new(),
                },
                /*plugin_id*/ None,
                /*selected_plugin_server*/ false,
            )
            .expect("Apps call should have a permission source");
            assert!(call.is_host_owned_apps());
            ((tool.server_name, tool.tool.name.to_string()), call)
        })
        .collect();
    let clients = Arc::new(McpBindingClients::new(HashMap::from([(
        CODEX_APPS_MCP_SERVER_NAME.to_string(),
        managed_client,
    )])));
    Arc::new(McpBinding::new(
        connections,
        clients,
        config,
        /*plugins_available*/ false,
        tools,
        calls,
    ))
}

#[tokio::test]
async fn later_step_keeps_the_turn_discovery_on_the_same_apps_connection() {
    let transport = apps_transport().await;
    let first = step_binding(&transport, vec![search_tool("catalog")]).await;
    let discovered = step_binding(&transport, vec![listed_search_tool()]).await;
    // The Apps server finished starting: the next step recaptures, now with a cached tool too.
    let later_search = ToolInfo {
        server_origin: Some("https://apps.example".to_string()),
        ..search_tool("catalog")
    };
    let later = step_binding(&transport, vec![later_search, docs_tool()]).await;

    let Some(SearchDiscovery::Ready(rebased)) =
        later.fork_rebase_model_search(&first, &SearchDiscovery::Ready(discovered))
    else {
        panic!("the turn's discovery should carry over to the recaptured binding");
    };
    // The declaration comes from the discovery, the rest from the later step's catalog.
    let projected = ToolInfo {
        server_origin: Some("https://apps.example".to_string()),
        ..listed_search_tool()
    };
    assert_eq!(rebased.tools(), [projected.clone(), docs_tool()].as_slice());
    let call = rebased
        .prepare_call(CODEX_APPS_MCP_SERVER_NAME, SEARCH_TOOL)
        .expect("projected Search call");
    assert_eq!(call.tool_info(), &projected);
    let live = later
        .prepare_call(CODEX_APPS_MCP_SERVER_NAME, SEARCH_TOOL)
        .expect("live Search call");
    assert_eq!(
        rebased.search_invocation_context(&live),
        Some(&json!({"tool_type": "SonicBrowserToolV99"}))
    );

    assert!(matches!(
        later.fork_rebase_model_search(&first, &SearchDiscovery::Critical("discovery_error")),
        Some(SearchDiscovery::Critical("discovery_error"))
    ));
    assert!(matches!(
        later.fork_rebase_model_search(&first, &SearchDiscovery::Unavailable),
        Some(SearchDiscovery::Unavailable)
    ));
}

#[tokio::test]
async fn another_apps_connection_discovers_again() {
    let transport = apps_transport().await;
    let first = step_binding(&transport, vec![search_tool("catalog")]).await;
    let discovered = step_binding(&transport, vec![listed_search_tool()]).await;
    let reconnected = step_binding(&apps_transport().await, vec![search_tool("catalog")]).await;
    let without_search = step_binding(&transport, vec![docs_tool()]).await;

    for discovery in [
        SearchDiscovery::Ready(discovered),
        SearchDiscovery::Critical("discovery_error"),
        SearchDiscovery::Unavailable,
    ] {
        assert!(
            reconnected
                .fork_rebase_model_search(&first, &discovery)
                .is_none(),
            "a pin must not move to another Apps connection"
        );
        assert!(
            without_search
                .fork_rebase_model_search(&first, &discovery)
                .is_none()
        );
        assert!(
            reconnected
                .fork_rebase_model_search(&without_search, &discovery)
                .is_none()
        );
    }
}
