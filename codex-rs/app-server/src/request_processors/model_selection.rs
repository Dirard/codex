use super::*;
use codex_core::build_models_manager;
use codex_model_provider_info::OPENAI_PROVIDER_ID;
use codex_models_manager::manager::RefreshStrategy;

const RAW_MODEL_KEY: &str = "model";
const RAW_MODEL_PROVIDER_KEY: &str = "model_provider";

#[derive(Debug, Default)]
pub(crate) struct ModelSelectionRequest {
    explicit_model: Option<String>,
    explicit_model_provider: Option<String>,
}

impl ModelSelectionRequest {
    pub(crate) fn from_overrides(
        request_overrides: Option<&HashMap<String, serde_json::Value>>,
        typesafe_overrides: &ConfigOverrides,
        method: &str,
    ) -> Result<Self, JSONRPCErrorError> {
        let raw_model = raw_string_override(request_overrides, RAW_MODEL_KEY, method)?;
        let raw_model_provider =
            raw_string_override(request_overrides, RAW_MODEL_PROVIDER_KEY, method)?;

        Ok(Self {
            explicit_model: combined_override(
                method,
                "model",
                RAW_MODEL_KEY,
                typesafe_overrides.model.as_deref(),
                raw_model,
            )?,
            explicit_model_provider: combined_override(
                method,
                "modelProvider",
                RAW_MODEL_PROVIDER_KEY,
                typesafe_overrides.model_provider.as_deref(),
                raw_model_provider,
            )?,
        })
    }

    pub(crate) fn reject_provider_without_model(
        &self,
        method: &str,
    ) -> Result<(), JSONRPCErrorError> {
        if self.explicit_model_provider.is_some() && self.explicit_model.is_none() {
            return Err(invalid_request(format!(
                "invalid {method} model selection: modelProvider requires model"
            )));
        }
        Ok(())
    }

    pub(crate) fn with_model_for_provider_only_selection(mut self, model: Option<&str>) -> Self {
        if self.explicit_model_provider.is_some() && self.explicit_model.is_none() {
            self.explicit_model = model.map(str::to_string);
        }
        self
    }

    fn is_explicit(&self) -> bool {
        self.explicit_model.is_some() || self.explicit_model_provider.is_some()
    }

    fn requires_custom_allowlist(&self) -> bool {
        self.explicit_model_provider.is_some()
    }
}

pub(crate) async fn apply_loaded_config_model_selection(
    thread_manager: &ThreadManager,
    config: &mut Config,
    selection: &ModelSelectionRequest,
    method: &str,
) -> Result<(), JSONRPCErrorError> {
    if !selection.is_explicit() {
        return Ok(());
    }

    let Some(model) = config.model.clone() else {
        return Err(invalid_request(format!(
            "invalid {method} model selection: model is required"
        )));
    };

    if selection.explicit_model_provider.is_none()
        && let Some(provider_id) = inferred_provider_for_model_only_selection(
            thread_manager,
            config,
            model.as_str(),
            method,
        )
        .await?
    {
        apply_model_provider(config, provider_id.as_str(), method)?;
    }

    validate_model_for_provider(
        thread_manager,
        config,
        config.model_provider_id.as_str(),
        model.as_str(),
        method,
        selection.requires_custom_allowlist(),
    )
    .await
}

async fn inferred_provider_for_model_only_selection(
    thread_manager: &ThreadManager,
    config: &Config,
    model: &str,
    method: &str,
) -> Result<Option<String>, JSONRPCErrorError> {
    let provider_ids = configured_provider_ids_for_model(config, model);
    if provider_ids
        .iter()
        .any(|provider_id| provider_id == &config.model_provider_id)
    {
        return Ok(None);
    }

    if provider_ids.is_empty() {
        if config.model_provider_id != OPENAI_PROVIDER_ID
            && !provider_has_model_allowlist(&config.model_provider)
        {
            return Ok(None);
        }

        if config.model_provider_id != OPENAI_PROVIDER_ID
            && validate_openai_model(thread_manager, config, model, method)
                .await
                .is_ok()
        {
            return Ok(Some(OPENAI_PROVIDER_ID.to_string()));
        }
        return Ok(None);
    }

    if config.model_provider_id == OPENAI_PROVIDER_ID
        && validate_openai_model(thread_manager, config, model, method)
            .await
            .is_ok()
    {
        return Ok(None);
    }

    match provider_ids.as_slice() {
        [provider_id] => Ok(Some(provider_id.clone())),
        provider_ids => {
            let provider_list = provider_ids
                .iter()
                .map(|provider_id| format!("'{provider_id}'"))
                .collect::<Vec<_>>()
                .join(", ");
            Err(invalid_request(format!(
                "invalid {method} model selection: model '{model}' is configured for multiple modelProviders ({provider_list}); pass modelProvider explicitly"
            )))
        }
    }
}

fn configured_provider_ids_for_model(config: &Config, model: &str) -> Vec<String> {
    let mut provider_ids = config
        .model_providers
        .iter()
        .filter(|(provider_id, provider)| {
            provider_id.as_str() != OPENAI_PROVIDER_ID && provider_lists_model(provider, model)
        })
        .map(|(provider_id, _)| provider_id.clone())
        .collect::<Vec<_>>();

    if config.model_provider_id != OPENAI_PROVIDER_ID
        && !config
            .model_providers
            .contains_key(&config.model_provider_id)
        && provider_lists_model(&config.model_provider, model)
    {
        provider_ids.push(config.model_provider_id.clone());
    }

    provider_ids.sort();
    provider_ids.dedup();
    provider_ids
}

fn apply_model_provider(
    config: &mut Config,
    provider_id: &str,
    method: &str,
) -> Result<(), JSONRPCErrorError> {
    if provider_id == config.model_provider_id {
        return Ok(());
    }

    let Some(provider) = config.model_providers.get(provider_id).cloned() else {
        return Err(invalid_request(format!(
            "invalid {method} model selection: unknown modelProvider '{provider_id}'"
        )));
    };

    config.model_provider_id = provider_id.to_string();
    config.model_provider = provider;
    Ok(())
}

pub(crate) async fn validate_model_for_provider(
    thread_manager: &ThreadManager,
    config: &Config,
    provider_id: &str,
    model: &str,
    method: &str,
    require_custom_allowlist: bool,
) -> Result<(), JSONRPCErrorError> {
    if provider_id == OPENAI_PROVIDER_ID {
        validate_openai_model(thread_manager, config, model, method).await
    } else {
        let Some(provider) = config.model_providers.get(provider_id).or_else(|| {
            (provider_id == config.model_provider_id).then_some(&config.model_provider)
        }) else {
            return Err(invalid_request(format!(
                "invalid {method} model selection: unknown modelProvider '{provider_id}'"
            )));
        };
        validate_configured_provider_model(
            provider_id,
            provider,
            model,
            method,
            require_custom_allowlist,
        )
    }
}

async fn validate_openai_model(
    thread_manager: &ThreadManager,
    config: &Config,
    model: &str,
    method: &str,
) -> Result<(), JSONRPCErrorError> {
    let Some(openai_provider) = config.model_providers.get(OPENAI_PROVIDER_ID).or_else(|| {
        (config.model_provider_id == OPENAI_PROVIDER_ID).then_some(&config.model_provider)
    }) else {
        return Err(invalid_request(format!(
            "invalid {method} model selection: unknown modelProvider '{OPENAI_PROVIDER_ID}'"
        )));
    };
    let mut openai_config = config.clone();
    openai_config.model_provider_id = OPENAI_PROVIDER_ID.to_string();
    openai_config.model_provider = openai_provider.clone();

    let model_exists = build_models_manager(&openai_config, thread_manager.auth_manager())
        .list_models(RefreshStrategy::OnlineIfUncached)
        .await
        .into_iter()
        .any(|preset| {
            preset.model == model
                && preset
                    .model_provider
                    .as_deref()
                    .unwrap_or(OPENAI_PROVIDER_ID)
                    == OPENAI_PROVIDER_ID
        });

    if model_exists {
        Ok(())
    } else {
        Err(invalid_request(format!(
            "invalid {method} model selection: model '{model}' is not available for modelProvider '{OPENAI_PROVIDER_ID}'"
        )))
    }
}

fn validate_configured_provider_model(
    provider_id: &str,
    provider: &ModelProviderInfo,
    model: &str,
    method: &str,
    require_custom_allowlist: bool,
) -> Result<(), JSONRPCErrorError> {
    if !provider_has_model_allowlist(provider) {
        if require_custom_allowlist {
            return Err(invalid_request(format!(
                "invalid {method} model selection: modelProvider '{provider_id}' does not define any models"
            )));
        }
        return Ok(());
    }

    if provider_lists_model(provider, model) {
        Ok(())
    } else {
        Err(invalid_request(format!(
            "invalid {method} model selection: model '{model}' is not configured for modelProvider '{provider_id}'"
        )))
    }
}

fn provider_has_model_allowlist(provider: &ModelProviderInfo) -> bool {
    provider
        .models
        .iter()
        .any(|configured| !configured.trim().is_empty())
}

fn provider_lists_model(provider: &ModelProviderInfo, model: &str) -> bool {
    provider
        .models
        .iter()
        .map(|configured| configured.trim())
        .any(|configured| !configured.is_empty() && configured == model)
}

fn raw_string_override(
    request_overrides: Option<&HashMap<String, serde_json::Value>>,
    key: &str,
    method: &str,
) -> Result<Option<String>, JSONRPCErrorError> {
    let Some(value) = request_overrides.and_then(|overrides| overrides.get(key)) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(invalid_request(format!(
            "invalid {method} model selection: config.{key} must be a string"
        )));
    };
    Ok(Some(value.to_string()))
}

fn combined_override(
    method: &str,
    top_level_name: &str,
    raw_key: &str,
    top_level_value: Option<&str>,
    raw_value: Option<String>,
) -> Result<Option<String>, JSONRPCErrorError> {
    if let (Some(top_level_value), Some(raw_value)) = (top_level_value, raw_value.as_deref())
        && top_level_value != raw_value
    {
        return Err(invalid_request(format!(
            "invalid {method} model selection: conflicting {top_level_name} and config.{raw_key} values"
        )));
    }

    Ok(top_level_value.map(str::to_string).or(raw_value))
}
