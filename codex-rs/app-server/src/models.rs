use std::collections::HashSet;
use std::sync::Arc;

use codex_app_server_protocol::Model;
use codex_app_server_protocol::ModelServiceTier;
use codex_app_server_protocol::ModelUpgradeInfo;
use codex_app_server_protocol::ReasoningEffortOption;
use codex_core::ThreadManager;
use codex_core::build_models_manager;
use codex_core::config::Config;
use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::OPENAI_PROVIDER_ID;
use codex_model_provider_info::WireApi;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::openai_models::InputModality;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::openai_models::default_input_modalities;

pub async fn supported_models(
    thread_manager: Arc<ThreadManager>,
    config: &Config,
    include_hidden: bool,
) -> Vec<Model> {
    let mut presets = openai_model_presets(thread_manager, config).await;
    add_configured_provider_model_presets(&mut presets, config);

    presets
        .into_iter()
        .filter(|preset| include_hidden || preset.show_in_picker)
        .map(model_from_preset)
        .collect()
}

async fn openai_model_presets(
    thread_manager: Arc<ThreadManager>,
    config: &Config,
) -> Vec<ModelPreset> {
    if config.model_provider_id == OPENAI_PROVIDER_ID {
        thread_manager
            .list_models(RefreshStrategy::OnlineIfUncached)
            .await
    } else {
        let Some(openai_provider) = config.model_providers.get(OPENAI_PROVIDER_ID) else {
            return Vec::new();
        };

        let mut openai_config = config.clone();
        openai_config.model_provider_id = OPENAI_PROVIDER_ID.to_string();
        openai_config.model_provider = openai_provider.clone();
        build_models_manager(&openai_config, thread_manager.auth_manager())
            .list_models(RefreshStrategy::OnlineIfUncached)
            .await
    }
}

fn add_configured_provider_model_presets(presets: &mut Vec<ModelPreset>, config: &Config) {
    let mut seen: HashSet<(String, String)> = presets
        .iter()
        .map(|preset| {
            (
                preset
                    .model_provider
                    .clone()
                    .unwrap_or_else(|| OPENAI_PROVIDER_ID.to_string()),
                preset.model.clone(),
            )
        })
        .collect();

    let mut providers = config.model_providers.iter().collect::<Vec<_>>();
    providers.sort_by_key(|(provider_id, _)| provider_id.as_str());

    for (provider_id, provider) in providers {
        for model in provider
            .models
            .iter()
            .map(|model| model.trim())
            .filter(|model| !model.is_empty())
        {
            if seen.insert((provider_id.clone(), model.to_string())) {
                presets.push(configured_provider_model_preset(
                    provider_id,
                    provider,
                    model,
                ));
            }
        }
    }
}

fn configured_provider_model_preset(
    provider_id: &str,
    provider: &ModelProviderInfo,
    model: &str,
) -> ModelPreset {
    let provider_name = provider.name.as_str();
    let provider_label = if provider_name.is_empty() {
        provider_id
    } else {
        provider_name
    };
    ModelPreset {
        id: format!("{provider_id}/{model}"),
        model: model.to_string(),
        model_provider: Some(provider_id.to_string()),
        display_name: model.to_string(),
        description: format!("Custom provider: {provider_label}"),
        default_reasoning_effort: ReasoningEffort::None,
        supported_reasoning_efforts: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::None,
            description: "No reasoning".to_string(),
        }],
        supports_personality: false,
        additional_speed_tiers: Vec::new(),
        service_tiers: Vec::new(),
        default_service_tier: None,
        is_default: false,
        upgrade: None,
        show_in_picker: true,
        availability_nux: None,
        supported_in_api: true,
        input_modalities: input_modalities_for_provider(provider),
    }
}

fn input_modalities_for_provider(provider: &ModelProviderInfo) -> Vec<InputModality> {
    match provider.wire_api {
        WireApi::Responses => default_input_modalities(),
        WireApi::Chat => vec![InputModality::Text],
    }
}

fn model_from_preset(preset: ModelPreset) -> Model {
    Model {
        id: preset.id.to_string(),
        model: preset.model.to_string(),
        model_provider: Some(
            preset
                .model_provider
                .unwrap_or_else(|| OPENAI_PROVIDER_ID.to_string()),
        ),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: preset.upgrade.as_ref().map(|upgrade| ModelUpgradeInfo {
            model: upgrade.id.clone(),
            upgrade_copy: upgrade.upgrade_copy.clone(),
            model_link: upgrade.model_link.clone(),
            migration_markdown: upgrade.migration_markdown.clone(),
        }),
        availability_nux: preset.availability_nux.map(Into::into),
        display_name: preset.display_name.to_string(),
        description: preset.description.to_string(),
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: reasoning_efforts_from_preset(
            preset.supported_reasoning_efforts,
        ),
        default_reasoning_effort: preset.default_reasoning_effort,
        input_modalities: preset.input_modalities,
        supports_personality: preset.supports_personality,
        additional_speed_tiers: preset.additional_speed_tiers,
        service_tiers: preset
            .service_tiers
            .into_iter()
            .map(|service_tier| ModelServiceTier {
                id: service_tier.id,
                name: service_tier.name,
                description: service_tier.description,
            })
            .collect(),
        default_service_tier: preset.default_service_tier,
        is_default: preset.is_default,
    }
}

fn reasoning_efforts_from_preset(
    efforts: Vec<ReasoningEffortPreset>,
) -> Vec<ReasoningEffortOption> {
    efforts
        .into_iter()
        .map(|preset| ReasoningEffortOption {
            reasoning_effort: preset.effort,
            description: preset.description,
        })
        .collect()
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
