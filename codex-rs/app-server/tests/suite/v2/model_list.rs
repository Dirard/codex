use std::time::Duration;

use anyhow::Error;
use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::TestAppServer;
use app_test_support::to_response;
use app_test_support::write_chatgpt_auth;
use app_test_support::write_models_cache;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::Model;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::ModelServiceTier;
use codex_app_server_protocol::ModelUpgradeInfo;
use codex_app_server_protocol::ReasoningEffortOption;
use codex_app_server_protocol::RequestId;
use codex_config::types::AuthCredentialsStoreMode;
use codex_model_provider_info::OPENAI_PROVIDER_ID;
use codex_protocol::openai_models::InputModality;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::MockServer;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const INVALID_REQUEST_ERROR_CODE: i64 = -32600;

fn model_from_preset(preset: &ModelPreset) -> Model {
    Model {
        id: preset.id.clone(),
        model: preset.model.clone(),
        model_provider: Some(
            preset
                .model_provider
                .clone()
                .unwrap_or_else(|| OPENAI_PROVIDER_ID.to_string()),
        ),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: preset.upgrade.as_ref().map(|upgrade| ModelUpgradeInfo {
            model: upgrade.id.clone(),
            upgrade_copy: upgrade.upgrade_copy.clone(),
            model_link: upgrade.model_link.clone(),
            migration_markdown: upgrade.migration_markdown.clone(),
        }),
        availability_nux: preset.availability_nux.clone().map(Into::into),
        display_name: preset.display_name.clone(),
        description: preset.description.clone(),
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: preset
            .supported_reasoning_efforts
            .iter()
            .map(|preset| ReasoningEffortOption {
                reasoning_effort: preset.effort.clone(),
                description: preset.description.clone(),
            })
            .collect(),
        default_reasoning_effort: preset.default_reasoning_effort.clone(),
        input_modalities: preset.input_modalities.clone(),
        // `write_models_cache()` round-trips through a simplified ModelInfo fixture that does not
        // preserve personality placeholders in base instructions, so app-server list results from
        // cache report `supports_personality = false`.
        // todo(sayan): fix, maybe make roundtrip use ModelInfo only
        supports_personality: false,
        additional_speed_tiers: preset.additional_speed_tiers.clone(),
        service_tiers: preset
            .service_tiers
            .iter()
            .map(|service_tier| ModelServiceTier {
                id: service_tier.id.clone(),
                name: service_tier.name.clone(),
                description: service_tier.description.clone(),
            })
            .collect(),
        default_service_tier: preset.default_service_tier.clone(),
        is_default: preset.is_default,
    }
}

fn expected_visible_models() -> Vec<Model> {
    // Filter by supported_in_api to support testing with both ChatGPT and non-ChatGPT auth modes.
    let mut presets = ModelPreset::filter_by_auth(
        codex_core::test_support::all_model_presets().clone(),
        /*chatgpt_mode*/ false,
    );

    // Mirror `ModelsManager::build_available_models()` default selection after auth filtering.
    ModelPreset::mark_default_by_picker_visibility(&mut presets);

    presets
        .iter()
        .filter(|preset| preset.show_in_picker)
        .map(model_from_preset)
        .collect()
}

#[tokio::test]
async fn list_models_includes_configured_provider_models() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        r#"
model = "gpt-5-codex"
approval_policy = "never"
sandbox_mode = "read-only"

[model_providers.zai]
name = "Z.ai"
base_url = "https://api.z.ai/api/coding/paas/v4"
wire_api = "chat"
models = ["glm-4.6"]
"#,
    )?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;
    let custom_model = items
        .into_iter()
        .find(|item| item.model == "glm-4.6" && item.model_provider.as_deref() == Some("zai"))
        .expect("configured provider model should be listed");

    assert_eq!(
        custom_model,
        Model {
            id: "zai/glm-4.6".to_string(),
            model: "glm-4.6".to_string(),
            model_provider: Some("zai".to_string()),
            upgrade: None,
            upgrade_info: None,
            availability_nux: None,
            display_name: "glm-4.6".to_string(),
            description: "Custom provider: Z.ai".to_string(),
            hidden: false,
            supported_reasoning_efforts: vec![ReasoningEffortOption {
                reasoning_effort: ReasoningEffort::None,
                description: "No reasoning".to_string(),
            }],
            default_reasoning_effort: ReasoningEffort::None,
            input_modalities: vec![InputModality::Text],
            supports_personality: false,
            additional_speed_tiers: Vec::new(),
            service_tiers: Vec::new(),
            default_service_tier: None,
            is_default: false,
        }
    );
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_reloads_configured_provider_models_from_latest_config() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        r#"
model = "gpt-5-codex"
approval_policy = "never"
sandbox_mode = "read-only"
"#,
    )?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    std::fs::write(
        codex_home.path().join("config.toml"),
        r#"
model = "gpt-5-codex"
approval_policy = "never"
sandbox_mode = "read-only"

[model_providers.zai]
name = "Z.ai"
base_url = "https://api.z.ai/api/coding/paas/v4"
wire_api = "chat"
models = ["glm-5.1"]
"#,
    )?;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;
    let custom_model = items
        .into_iter()
        .find(|item| item.model == "glm-5.1" && item.model_provider.as_deref() == Some("zai"))
        .expect("configured provider model should be listed after config reload");

    assert_eq!(
        custom_model,
        Model {
            id: "zai/glm-5.1".to_string(),
            model: "glm-5.1".to_string(),
            model_provider: Some("zai".to_string()),
            upgrade: None,
            upgrade_info: None,
            availability_nux: None,
            display_name: "glm-5.1".to_string(),
            description: "Custom provider: Z.ai".to_string(),
            hidden: false,
            supported_reasoning_efforts: vec![ReasoningEffortOption {
                reasoning_effort: ReasoningEffort::None,
                description: "No reasoning".to_string(),
            }],
            default_reasoning_effort: ReasoningEffort::None,
            input_modalities: vec![InputModality::Text],
            supports_personality: false,
            additional_speed_tiers: Vec::new(),
            service_tiers: Vec::new(),
            default_service_tier: None,
            is_default: false,
        }
    );
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_uses_only_explicit_custom_provider_models_without_discovery() -> Result<()> {
    let codex_home = TempDir::new()?;
    let provider_server = MockServer::start().await;
    let auth_marker = codex_home.path().join("custom-provider-auth-command-ran");
    let auth_marker_arg = auth_marker.to_string_lossy().to_string();
    let (auth_command, auth_args) = if cfg!(windows) {
        (
            "cmd".to_string(),
            vec![
                "/C".to_string(),
                format!("echo ran>\"{}\"", auth_marker.display()),
            ],
        )
    } else {
        (
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "printf ran > \"$1\"".to_string(),
                "codex-auth-marker".to_string(),
                auth_marker_arg,
            ],
        )
    };
    let auth_args_toml = format!(
        "[{}]",
        auth_args
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    );
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model_provider = "zai"
model = "zai-runtime-only"
approval_policy = "never"
sandbox_mode = "read-only"

[model_providers.no_picker_models]
name = "No Picker Models"
base_url = "https://no-picker.example/v1"
wire_api = "chat"

[model_providers.zai]
name = "Z.ai"
base_url = {}
wire_api = "chat"
models = ["glm-4.6", " ", "glm-4.6", "", "glm-4.7"]
auth = {{ command = {}, args = {auth_args_toml}, timeout_ms = 1 }}
"#,
            serde_json::to_string(&provider_server.uri())?,
            serde_json::to_string(&auth_command)?,
        ),
    )?;
    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(200),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;
    let custom_models = items
        .iter()
        .filter(|item| item.model_provider.as_deref() != Some(OPENAI_PROVIDER_ID))
        .map(|item| {
            (
                item.model_provider.clone().unwrap_or_default(),
                item.model.clone(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        custom_models,
        vec![
            ("zai".to_string(), "glm-4.6".to_string()),
            ("zai".to_string(), "glm-4.7".to_string()),
        ]
    );
    assert!(next_cursor.is_none());
    assert!(
        !items.iter().any(|item| item.model == "zai-runtime-only"),
        "active config.model must not be synthesized into model/list"
    );
    assert!(
        !items
            .iter()
            .any(|item| item.model_provider.as_deref() == Some("no_picker_models")),
        "provider without models must not contribute picker entries"
    );
    let requests = provider_server
        .received_requests()
        .await
        .unwrap_or_default();
    assert_eq!(
        requests.len(),
        0,
        "custom provider /models must not be called"
    );
    assert!(
        !auth_marker.exists(),
        "custom provider auth.command must not run for model/list"
    );
    Ok(())
}

#[tokio::test]
async fn list_models_returns_all_models_with_large_limit() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;

    let expected_models = expected_visible_models();

    assert_eq!(items, expected_models);
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_includes_hidden_models() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: Some(true),
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;

    assert!(items.iter().any(|item| item.hidden));
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_uses_configured_catalog_as_source_of_truth() -> Result<()> {
    let remote_model: ModelInfo = serde_json::from_value(json!({
        "slug": "chatgpt-remote-only",
        "display_name": "ChatGPT Remote Only",
        "description": "Remote-only model for app-server model/list coverage",
        "default_reasoning_level": "max",
        "supported_reasoning_levels": [
            {"effort": "max", "description": "Maximum"},
            {"effort": "low", "description": "Low"},
            {"effort": "focused", "description": "Focused"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "minimal_client_version": [0, 1, 0],
        "supported_in_api": true,
        "priority": 0,
        "upgrade": null,
        "base_instructions": "base instructions",
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10_000},
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 272_000,
        "max_context_window": 272_000,
        "experimental_supported_tools": [],
    }))?;

    let codex_home = TempDir::new()?;
    let catalog_path = codex_home.path().join("catalog.json");
    std::fs::write(
        &catalog_path,
        serde_json::to_string(&ModelsResponse {
            models: vec![remote_model.clone()],
        })?,
    )?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"
model_catalog_json = '{}'
"#,
            catalog_path.display()
        ),
    )?;
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-access-token").plan_type("pro"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp =
        TestAppServer::new_with_env(codex_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: Some(100),
            cursor: None,
            include_hidden: None,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let ModelListResponse {
        data: items,
        next_cursor,
    } = to_response::<ModelListResponse>(response)?;
    let mut expected_presets: Vec<ModelPreset> = vec![remote_model.into()];
    ModelPreset::mark_default_by_picker_visibility(&mut expected_presets);
    let mut expected_items = expected_presets
        .iter()
        .map(model_from_preset)
        .collect::<Vec<_>>();
    expected_items[0].supported_reasoning_efforts = vec![
        ReasoningEffortOption {
            reasoning_effort: "max".parse().map_err(Error::msg)?,
            description: "Maximum".to_string(),
        },
        ReasoningEffortOption {
            reasoning_effort: "low".parse().map_err(Error::msg)?,
            description: "Low".to_string(),
        },
        ReasoningEffortOption {
            reasoning_effort: "focused".parse().map_err(Error::msg)?,
            description: "Focused".to_string(),
        },
    ];

    assert_eq!(items, expected_items);
    assert!(next_cursor.is_none());
    Ok(())
}

#[tokio::test]
async fn list_models_pagination_works() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let expected_models = expected_visible_models();
    let mut cursor = None;
    let mut items = Vec::new();

    for _ in 0..expected_models.len() {
        let request_id = mcp
            .send_list_models_request(ModelListParams {
                limit: Some(1),
                cursor: cursor.clone(),
                include_hidden: None,
            })
            .await?;

        let response: JSONRPCResponse = timeout(
            DEFAULT_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
        )
        .await??;

        let ModelListResponse {
            data: page_items,
            next_cursor,
        } = to_response::<ModelListResponse>(response)?;

        assert_eq!(page_items.len(), 1);
        items.extend(page_items);

        if let Some(next_cursor) = next_cursor {
            cursor = Some(next_cursor);
        } else {
            assert_eq!(items, expected_models);
            return Ok(());
        }
    }

    panic!(
        "model pagination did not terminate after {} pages",
        expected_models.len()
    );
}

#[tokio::test]
async fn list_models_paginates_configured_provider_models_in_deterministic_order() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    std::fs::write(
        codex_home.path().join("config.toml"),
        r#"
model = "gpt-5-codex"
approval_policy = "never"
sandbox_mode = "read-only"

[model_providers.zai]
name = "Z.ai"
base_url = "https://api.z.ai/api/coding/paas/v4"
wire_api = "chat"
models = ["zai-first", "zai-second"]

[model_providers.alpha]
name = "Alpha"
base_url = "https://alpha.example/v1"
wire_api = "responses"
models = ["alpha-first"]
"#,
    )?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let mut cursor = None;
    let mut custom_models = Vec::new();

    loop {
        let request_id = mcp
            .send_list_models_request(ModelListParams {
                limit: Some(1),
                cursor: cursor.clone(),
                include_hidden: None,
            })
            .await?;

        let response: JSONRPCResponse = timeout(
            DEFAULT_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
        )
        .await??;

        let ModelListResponse {
            data: page_items,
            next_cursor,
        } = to_response::<ModelListResponse>(response)?;
        custom_models.extend(
            page_items
                .into_iter()
                .filter(|item| item.model_provider.as_deref() != Some(OPENAI_PROVIDER_ID))
                .map(|item| (item.model_provider.unwrap_or_default(), item.model)),
        );

        if next_cursor.is_none() {
            break;
        }
        cursor = next_cursor;
    }

    assert_eq!(
        custom_models,
        vec![
            ("alpha".to_string(), "alpha-first".to_string()),
            ("zai".to_string(), "zai-first".to_string()),
            ("zai".to_string(), "zai-second".to_string()),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn list_models_rejects_invalid_cursor() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_models_cache(codex_home.path())?;
    let mut mcp = TestAppServer::new(codex_home.path()).await?;

    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_list_models_request(ModelListParams {
            limit: None,
            cursor: Some("invalid".to_string()),
            include_hidden: None,
        })
        .await?;

    let error: JSONRPCError = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;

    assert_eq!(error.id, RequestId::Integer(request_id));
    assert_eq!(error.error.code, INVALID_REQUEST_ERROR_CODE);
    assert_eq!(error.error.message, "invalid cursor: invalid");
    Ok(())
}
