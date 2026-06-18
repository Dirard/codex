# Сравнение fork Codex с основной версией

Дата составления: 2026-06-16.

## База сравнения

- Рабочий каталог: `D:\ai-apps\codex`.
- Кодовый workspace Serena: `D:\ai-apps\codex\codex-rs`.
- Текущая ветка: `codex/my-staged-changes`.
- Текущий `HEAD`: `bf862ab21fbb0eb39c0dbdb74cc249667091972b`.
- База для отчета: `origin/main` на `0afe559318cddc3c47e508944b7bf2ee5791f43d`.
- Merge-base с `origin/main`: `0afe559318cddc3c47e508944b7bf2ee5791f43d`.
- Рабочее дерево до создания этого отчета: чистое, незакоммиченных изменений не было.

Важная оговорка: локальная ветка `main` указывает на `16d02ec77c6337ccea02a8c909e05bf3d905f887` и отстает от текущей ветки на 356 коммитов. Поэтому для описания fork относительно основной версии использован `origin/main`, иначе отчет смешал бы fork-изменения с обычным догоном устаревшего локального `main`.

Команда сравнения:

```powershell
git diff origin/main...HEAD
```

## Краткий итог

По сравнению с `origin/main` текущая ветка добавляет 5 коммитов:

- 95 файлов изменено.
- 4698 строк добавлено.
- 367 строк удалено.
- 4 новых файла создано.

Основной смысл fork:

- Rust workspace переведен на версию `0.137.0`.
- Добавлен Chat Completions-compatible транспорт для провайдеров с `wire_api = "chat"`.
- Расширена модель провайдеров: custom providers могут объявлять список моделей для picker/API.
- App-server v2 `model/list` теперь возвращает `modelProvider` и умеет показывать модели из конфигурации.
- TUI model picker умеет показывать и выбирать модели custom providers вместе с provider id.
- Добавлен конфигурируемый output truncation для shell/MCP/function output.
- Для Chat wire providers отключены hosted image generation и web search capabilities, но namespace tools остаются доступными.
- `default_mode_request_user_input` переведен в stable и включен по умолчанию.
- Tokio worker stack увеличен с 16 MiB до 48 MiB.
- `.serena` добавлена в `.gitignore`.

## Коммиты поверх `origin/main`

| Коммит | Смысл |
| --- | --- |
| `352eae181b` | Release/version commit: `codex-rs` workspace package version изменен с `0.0.0` на `0.137.0`; сообщение коммита содержит changelog `rust-v0.136.0...rust-v0.137.0`. |
| `26f6d36efa` | Основной функциональный блок: model provider info, Chat Completions endpoint, app-server model catalog, TUI picker, output truncation, protocol/schema/test updates. |
| `8613b04f8d` | `.serena` добавлена в `.gitignore`. |
| `d3c42417dd` | Tokio worker stack увеличен с 16 MiB до 48 MiB. |
| `bf862ab21f` | `default_mode_request_user_input` включен как stable feature; тесты и описание tool availability обновлены. |

## Изменения по смысловым блокам

### 1. Версия workspace и зависимости

- `codex-rs/Cargo.toml`: `workspace.package.version` изменен с `0.0.0` на `0.137.0`.
- `codex-rs/Cargo.lock`: обновлены lock entries под новую версию и зависимости.
- `codex-rs/app-server/Cargo.toml`: `codex-model-provider-info` перенесен из dev-dependencies в обычные dependencies, потому что app-server теперь использует provider info в runtime-коде.
- `codex-rs/codex-api/Cargo.toml`: добавлена зависимость `codex-utils-output-truncation` для Chat Completions адаптера.

### 2. Chat Completions-compatible API path

Добавлен новый endpoint-модуль `codex-rs/codex-api/src/endpoint/chat_completions.rs`.

Что делает новый слой:

- Отправляет запросы в `chat/completions`.
- Принимает внутренний `ResponsesApiRequest` и преобразует его в Chat Completions request.
- Переносит `instructions` в system message.
- Преобразует Responses input items в chat messages.
- Преобразует Responses tools в Chat Completions function tools.
- Поддерживает обычные function tools, custom tools, namespace tools и `tool_search`.
- Для `tool_search` ограничивает вывод 8 KiB.
- Пробрасывает `stream_options.include_usage = true`.
- Пробрасывает `response_format` из text controls.
- Пробрасывает `service_tier`.
- Читает SSE chunks Chat Completions и мапит их обратно в `ResponseEvent`.
- Собирает streamed text deltas, legacy `function_call`, indexed `tool_calls`, usage и server model.
- Завершает поток как обычный Responses-compatible stream для остального core-кода.

Core-клиент теперь выбирает транспорт по `wire_api`:

- `WireApi::Responses`: прежний путь через Responses API, включая WebSocket при доступности.
- `WireApi::Chat`: новый путь через Chat Completions HTTP streaming.

### 3. Model providers и capabilities

Изменения в provider model:

- `ModelProviderInfo` получил поле `models: Vec<String>`.
- Это поле описывает model slugs, которые нужно показывать в model picker для данного provider.
- Built-in OpenAI, Bedrock и OSS providers получают пустой список по умолчанию.
- Configured provider с `wire_api = Chat` получает capabilities:
  - `namespace_tools = true`.
  - `image_generation = false`.
  - `web_search = false`.
- Для Responses providers capabilities остаются прежними.

Это меняет fork в сторону более явной поддержки OpenAI-compatible custom providers, особенно тех, которые говорят через Chat Completions, а не через Responses API.

### 4. App-server v2 model catalog

Изменения в app-server:

- `supported_models` теперь добавляет модели из `config.model_providers[*].models`.
- Для custom provider model preset используется id формата `{provider_id}/{model}`.
- В preset сохраняется `model_provider`.
- Описание модели получает вид `Custom provider: <provider label>`.
- Reasoning для custom provider models по умолчанию отключен: `ReasoningEffort::None`.
- Input modalities берутся из provider capabilities.
- В `Model` app-server protocol добавлено поле `modelProvider`.
- Generated JSON schema и TypeScript schema обновлены.

Практический эффект: клиенты app-server могут отличать одинаковые model slug у разных providers и выбирать модель вместе с provider id.

### 5. TUI model picker и сохранение выбора модели

Изменения в TUI:

- Model picker добавляет модели из `config.model_providers[*].models`.
- Picker дедуплицирует пары `(provider_id, model)`.
- Если текущий `config.model` задан для non-OpenAI provider, он тоже добавляется в список.
- При выборе модели TUI может сохранить не только `model`, но и `model_provider`.
- В `AppEvent::PersistModelSelection` добавлен provider-aware сценарий.
- Добавлен snapshot для custom-provider model picker.
- Обновлены тесты model popup, plan mode, status/layout и config update.

Практический эффект: пользователь может выбрать custom-provider модель из UI, а не только прописывать ее вручную в конфиге.

### 6. Конфиг и schema

Добавлены настройки output truncation:

- `output_truncation.max_bytes`.
- `output_truncation.max_lines`.
- `output_truncation.mcp_max_lines`.

Настройки доступны:

- В root `ConfigToml`.
- В profile `ProfileToml`.
- В core `Config` как `OutputTruncationConfig`.
- В `core/config.schema.json`.

Также thread config proto получает поддержку `models` у provider config, чтобы remote/session config мог переносить список моделей provider.

### 7. Output truncation для shell, MCP и function output

`codex-rs/utils/output-truncation/src/lib.rs` расширен новым типом `OutputTruncation`.

Новая логика:

- Truncation policy теперь может учитывать и byte/token budget, и line budget.
- Для MCP output можно отдельно применять `mcp_max_lines`.
- Text output режется по середине строк при превышении `max_lines`.
- Function output content items режутся с сохранением image/encrypted items.
- Shell/exec output форматируется через `formatted_truncate_text_with_config`.
- TurnContext вычисляет эффективный truncation как более строгий из model policy и config `max_bytes`.

Практический эффект: fork дает администратору или профилю более точный контроль над тем, сколько tool output попадает в контекст модели и UI.

### 8. Context, history и tool output handling

Изменения в core вокруг истории и tool outputs:

- `ContextManager::record_items` принимает `OutputTruncation`, а не только `TruncationPolicy`.
- Запись conversation items, inter-agent communication, compact flow и shell/MCP handlers используют `turn_context.output_truncation()`.
- MCP `CallToolResult` conversion лучше сохраняет image content: если ответ содержит image и неизвестные items, unknown content превращается в text item вместо полной потери structured items.
- Добавлены и обновлены тесты history/context/tool output truncation.

### 9. Request user input в Default mode

Feature `default_mode_request_user_input` изменен:

- Был `Stage::UnderDevelopment`.
- Стал `Stage::Stable`.
- Был `default_enabled: false`.
- Стал `default_enabled: true`.

Тесты и спецификация tool handling обновлены так, чтобы отражать новое поведение: `request_user_input` доступен в Default или Plan mode.

### 10. Runtime stack и локальные файлы Serena

- `codex-rs/arg0/src/lib.rs`: `TOKIO_WORKER_STACK_SIZE_BYTES` увеличен с `16 * 1024 * 1024` до `48 * 1024 * 1024`.
- `.gitignore`: добавлена строка `.serena`, чтобы локальные файлы Serena не попадали в git.

## Полный список измененных файлов

| Статус | Файл | Кратко |
| --- | --- | --- |
| M | `.gitignore` | Игнорирование `.serena`. |
| M | `codex-rs/Cargo.lock` | Lockfile после version/dependency updates. |
| M | `codex-rs/Cargo.toml` | Workspace version `0.137.0`. |
| M | `codex-rs/app-server-protocol/schema/json/codex_app_server_protocol.schemas.json` | Generated schema для `modelProvider`. |
| M | `codex-rs/app-server-protocol/schema/json/codex_app_server_protocol.v2.schemas.json` | Generated v2 schema для `modelProvider`. |
| M | `codex-rs/app-server-protocol/schema/json/v2/ModelListResponse.json` | Generated response schema обновлена. |
| M | `codex-rs/app-server-protocol/schema/typescript/v2/Model.ts` | Generated TS type обновлен. |
| M | `codex-rs/app-server-protocol/src/protocol/v2/model.rs` | `Model` получил `model_provider`. |
| M | `codex-rs/app-server/Cargo.toml` | Runtime dependency на `codex-model-provider-info`. |
| M | `codex-rs/app-server/README.md` | App-server docs/example output обновлены под `modelProvider`. |
| M | `codex-rs/app-server/src/models.rs` | Добавление configured provider models в catalog. |
| A | `codex-rs/app-server/src/models_tests.rs` | Unit tests для configured provider models. |
| M | `codex-rs/app-server/src/request_processors/catalog_processor.rs` | Catalog/model list учитывает provider-aware model data. |
| M | `codex-rs/app-server/src/request_processors/thread_processor_tests.rs` | Тесты обновлены под provider model behavior. |
| M | `codex-rs/app-server/src/request_processors/turn_processor.rs` | Turn processing учитывает provider selection/model metadata. |
| M | `codex-rs/app-server/tests/suite/v2/model_list.rs` | Integration coverage для v2 `model/list`. |
| M | `codex-rs/arg0/src/lib.rs` | Tokio worker stack 16 MiB -> 48 MiB. |
| M | `codex-rs/codex-api/Cargo.toml` | Dependency на output truncation utilities. |
| A | `codex-rs/codex-api/src/endpoint/chat_completions.rs` | Новый Chat Completions streaming adapter. |
| A | `codex-rs/codex-api/src/endpoint/chat_completions_tests.rs` | Тесты Chat Completions adapter. |
| M | `codex-rs/codex-api/src/endpoint/mod.rs` | Экспорт нового endpoint. |
| M | `codex-rs/codex-api/src/lib.rs` | Экспорт Chat Completions client/options. |
| M | `codex-rs/config/src/config_toml.rs` | `OutputTruncationToml`, provider `models`. |
| M | `codex-rs/config/src/profile_toml.rs` | Profile-level `output_truncation`. |
| M | `codex-rs/config/src/thread_config.rs` | Thread config переносит provider models. |
| M | `codex-rs/config/src/thread_config/proto/codex.thread_config.v1.proto` | Proto field для provider models. |
| M | `codex-rs/config/src/thread_config/proto/codex.thread_config.v1.rs` | Generated proto Rust обновлен. |
| M | `codex-rs/config/src/thread_config/remote.rs` | Remote config maps provider models. |
| M | `codex-rs/core/config.schema.json` | Schema для `output_truncation` и provider `models`. |
| M | `codex-rs/core/src/agent/role.rs` | Role handling адаптирован к новым message/tool paths. |
| M | `codex-rs/core/src/agent/role_tests.rs` | Тесты role behavior обновлены. |
| M | `codex-rs/core/src/client.rs` | Выбор `WireApi::Chat`, Chat Completions stream path, truncation tests. |
| M | `codex-rs/core/src/compact.rs` | Compact flow использует `output_truncation`. |
| M | `codex-rs/core/src/compact_tests.rs` | Тесты compact обновлены. |
| M | `codex-rs/core/src/config/config_tests.rs` | Тесты загрузки `output_truncation`. |
| M | `codex-rs/core/src/config/mod.rs` | Core `OutputTruncationConfig`, config merge. |
| M | `codex-rs/core/src/context_manager/history.rs` | History truncation через `OutputTruncation`. |
| M | `codex-rs/core/src/context_manager/history_tests.rs` | Тесты history truncation/output. |
| M | `codex-rs/core/src/session/mod.rs` | Recording history uses `turn_context.output_truncation()`. |
| M | `codex-rs/core/src/session/rollout_reconstruction.rs` | Rollout reconstruction адаптирован к provider/truncation changes. |
| M | `codex-rs/core/src/session/tests.rs` | Session tests обновлены. |
| M | `codex-rs/core/src/session/turn_context.rs` | Effective output truncation на уровне turn. |
| M | `codex-rs/core/src/state/session.rs` | Session state учитывает новые model/provider fields. |
| M | `codex-rs/core/src/tasks/user_shell.rs` | Shell task output formatting использует config truncation. |
| M | `codex-rs/core/src/tools/context.rs` | Tool output structs используют `OutputTruncation`. |
| M | `codex-rs/core/src/tools/context_tests.rs` | Tests for tool output truncation/context. |
| M | `codex-rs/core/src/tools/events.rs` | Tool event output formatting обновлен. |
| M | `codex-rs/core/src/tools/handlers/mcp.rs` | MCP output truncation использует `mcp_max_lines`. |
| M | `codex-rs/core/src/tools/handlers/multi_agents/spawn.rs` | Multi-agent tool output behavior обновлен. |
| M | `codex-rs/core/src/tools/handlers/multi_agents_spec_tests.rs` | Spec tests adjusted. |
| M | `codex-rs/core/src/tools/handlers/multi_agents_tests.rs` | Multi-agent tests adjusted. |
| M | `codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs` | Multi-agent v2 spawn output/metadata adjusted. |
| M | `codex-rs/core/src/tools/handlers/request_user_input_spec_tests.rs` | Tests for stable Default mode request user input. |
| M | `codex-rs/core/src/tools/handlers/shell.rs` | Shell post-tool output uses `output_truncation`. |
| M | `codex-rs/core/src/tools/handlers/unified_exec/exec_command.rs` | Unified exec output truncation integration. |
| M | `codex-rs/core/src/tools/handlers/unified_exec/write_stdin.rs` | Background stdin/output truncation integration. |
| M | `codex-rs/core/src/tools/handlers/unified_exec_tests.rs` | Unified exec tests updated. |
| M | `codex-rs/core/src/tools/mod.rs` | Tool formatting helpers accept `OutputTruncation`. |
| M | `codex-rs/core/src/unified_exec/mod.rs` | Unified exec output path updated. |
| M | `codex-rs/core/src/unified_exec/mod_tests.rs` | Unified exec tests updated. |
| M | `codex-rs/core/src/unified_exec/process_manager.rs` | Process output formatting updated. |
| M | `codex-rs/core/src/user_shell_command.rs` | User shell command output formatting updated. |
| M | `codex-rs/core/tests/responses_headers.rs` | Headers integration tests adjusted. |
| M | `codex-rs/core/tests/suite/cli_stream.rs` | Client stream suite adjusted. |
| M | `codex-rs/core/tests/suite/client.rs` | Core client suite covers Chat/Responses behavior. |
| M | `codex-rs/core/tests/suite/client_websockets.rs` | WebSocket suite adjusted for wire API behavior. |
| M | `codex-rs/core/tests/suite/code_mode.rs` | Code mode suite adjusted. |
| M | `codex-rs/core/tests/suite/request_user_input.rs` | Request user input suite updated for stable default. |
| M | `codex-rs/core/tests/suite/stream_error_allows_next_turn.rs` | Stream error suite adjusted. |
| M | `codex-rs/core/tests/suite/stream_no_completed.rs` | Stream completion suite adjusted. |
| M | `codex-rs/core/tests/suite/truncation.rs` | New/expanded truncation integration coverage. |
| M | `codex-rs/features/src/lib.rs` | `default_mode_request_user_input` stable/default enabled. |
| M | `codex-rs/login/src/auth_env_telemetry.rs` | Auth telemetry adjusted for provider/API changes. |
| M | `codex-rs/model-provider-info/src/lib.rs` | `ModelProviderInfo.models`, tests and provider defaults. |
| M | `codex-rs/model-provider-info/src/model_provider_info_tests.rs` | Tests for provider model info. |
| M | `codex-rs/model-provider/src/provider.rs` | Chat wire provider capabilities. |
| M | `codex-rs/protocol/src/models.rs` | MCP content conversion preserves mixed image/unknown items. |
| M | `codex-rs/protocol/src/openai_models.rs` | `ModelPreset.model_provider`. |
| M | `codex-rs/tui/src/app/event_dispatch.rs` | Persist model selection with provider. |
| M | `codex-rs/tui/src/app/startup_prompts.rs` | Startup prompt flow adjusted. |
| M | `codex-rs/tui/src/app/tests/model_catalog.rs` | Model catalog tests adjusted. |
| M | `codex-rs/tui/src/app_event.rs` | `PersistModelSelection` carries provider. |
| M | `codex-rs/tui/src/app_server_session.rs` | App-server model/session integration adjusted. |
| M | `codex-rs/tui/src/chatwidget.rs` | Chat widget wiring for provider-aware model selection. |
| M | `codex-rs/tui/src/chatwidget/model_popups.rs` | Custom provider models in picker. |
| A | `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__custom_provider_model_picker.snap` | Snapshot for custom provider picker. |
| M | `codex-rs/tui/src/chatwidget/tests/plan_mode.rs` | Plan mode tests adjusted. |
| M | `codex-rs/tui/src/chatwidget/tests/popups_and_settings.rs` | Popup/settings tests cover provider model picker. |
| M | `codex-rs/tui/src/chatwidget/tests/status_and_layout.rs` | Status/layout tests adjusted. |
| M | `codex-rs/tui/src/config_update.rs` | Config update persists provider-aware selection. |
| M | `codex-rs/tui/src/config_update_tests.rs` | Tests for provider-aware config update. |
| M | `codex-rs/tui/src/history_cell/tests.rs` | History cell tests adjusted for output rendering. |
| M | `codex-rs/tui/src/status/tests.rs` | Status tests adjusted. |
| M | `codex-rs/utils/output-truncation/src/lib.rs` | `OutputTruncation`, line caps, MCP caps. |
| M | `codex-rs/utils/output-truncation/src/truncate_tests.rs` | Tests for truncation behavior. |

## Новые файлы

- `codex-rs/app-server/src/models_tests.rs`.
- `codex-rs/codex-api/src/endpoint/chat_completions.rs`.
- `codex-rs/codex-api/src/endpoint/chat_completions_tests.rs`.
- `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__custom_provider_model_picker.snap`.

## Потенциально важные behavioral differences

- Providers с `wire_api = "chat"` теперь реально используются через Chat Completions, а не пытаются идти по Responses API.
- Для Chat wire providers hosted `web_search` и `image_generation` считаются недоступными, даже если остальные provider defaults раньше могли показывать их как доступные.
- Модель теперь не всегда однозначно задается только строкой `model`: для custom providers появляется пара `modelProvider + model`.
- Output tool data может обрезаться строже, если настроены `output_truncation.max_bytes`, `max_lines` или `mcp_max_lines`.
- `request_user_input` в Default mode становится обычной стабильной возможностью, а не выключенной under-development feature.

## Что не проверялось в этом отчете

Тесты и форматирование не запускались, потому что задача была аналитической: сравнить ветку и записать документ. Документ основан на `git diff`, `git log`, `git status`, `git show` и выборочном чтении измененных модулей.
