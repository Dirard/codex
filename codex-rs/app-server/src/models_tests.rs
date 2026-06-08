use super::*;
use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::WireApi;
use codex_protocol::openai_models::InputModality;
use pretty_assertions::assert_eq;

#[test]
fn configured_chat_provider_model_preset_is_text_only() {
    let provider = ModelProviderInfo {
        name: "Z.AI".to_string(),
        wire_api: WireApi::Chat,
        ..Default::default()
    };

    let preset = configured_provider_model_preset("zai", &provider, "glm-5.1");

    assert_eq!(preset.model_provider, Some("zai".to_string()));
    assert_eq!(preset.input_modalities, vec![InputModality::Text]);
}

#[test]
fn builtin_model_presets_report_openai_provider_id() {
    let provider = ModelProviderInfo {
        name: "OpenAI".to_string(),
        wire_api: WireApi::Responses,
        ..Default::default()
    };
    let mut preset = configured_provider_model_preset(OPENAI_PROVIDER_ID, &provider, "gpt-5");
    preset.model_provider = None;

    let model = model_from_preset(preset);

    assert_eq!(model.model_provider, Some(OPENAI_PROVIDER_ID.to_string()));
}
