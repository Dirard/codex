# Сравнение fork Codex с основной версией

Дата обновления: 2026-06-19.

## База сравнения

- Рабочий каталог: `D:\ai-apps\codex`.
- Rust workspace: `D:\ai-apps\codex\codex-rs`.
- Текущая ветка: `codex/my-staged-changes`.
- Текущий `HEAD`: `ebd605c4d0c79dd75b674ac819104cd968e2ba47`.
- `origin/main`: `765309d5a611ea02be842ead0ab1a2828196fae9`.
- Merge-base с `origin/main`: `765309d5a611ea02be842ead0ab1a2828196fae9`.
- `HEAD` также соответствует `origin/codex/my-staged-changes` на момент обновления отчета.

Основная команда сравнения committed-части ветки:

```powershell
git diff origin/main...HEAD
```

Важная оговорка: при обновлении этого файла рабочее дерево не было чистым, потому что поверх `HEAD` уже внесены review-fix правки. Поэтому разделы про `origin/main...HEAD` описывают committed-состояние ветки, а отдельный раздел ниже перечисляет незакоммиченные исправления, которые тоже должны попасть в итоговый diff перед merge.

## Краткий итог `origin/main...HEAD`

По сравнению с `origin/main` текущий `HEAD` добавляет 9 коммитов:

- 173 файла изменено.
- 9047 строк добавлено.
- 1118 строк удалено.
- 8 новых файлов создано.
- Удаленных файлов нет.

Основной смысл fork:

- Rust workspace обновлен до версии `0.137.0`.
- Добавлен Chat Completions-compatible транспорт для провайдеров с `wire_api = "chat"`.
- Расширена модель провайдеров и выбора моделей: provider id передается через app-server API, thread/session metadata и TUI.
- App-server v2 `model/list` возвращает `modelProvider`, поддерживает модели из конфигурации и cursor pagination.
- App-server account/config/thread flows обновлены под provider-aware model selection.
- TUI model picker умеет показывать и выбирать модели custom providers вместе с provider id.
- Добавлен конфигурируемый output truncation для shell, MCP и function output.
- Для Chat wire providers отключены hosted image generation и web search capabilities, namespace tools остаются доступными.
- `default_mode_request_user_input` переведен в stable/default.
- Tokio worker stack увеличен с 16 MiB до 48 MiB.
- `.serena` добавлена в `.gitignore`.

## Коммиты поверх `origin/main`

| Коммит | Смысл |
| --- | --- |
| `8433454244` | Release/version commit с changelog и обновлением workspace до `0.137.0`. |
| `bf16e0e0b9` | Dependency/version update и добавление `codex-model-provider-info`. |
| `5ed2eed381` | `.serena` добавлена в `.gitignore`. |
| `15401c6a7a` | Tokio worker stack увеличен с 16 MiB до 48 MiB. |
| `95b193778f` | Refactor request user input handling для Default mode. |
| `1d2f97ac0f` | New model selection features и config handling. |
| `bb98b263aa` | Refactor model selection logic и расширение config handling. |
| `db7add979a` | Fix rebase fallout после обновления от main. |
| `ebd605c4d0` | Refactor model selection и validation logic. |

## Изменения по смысловым блокам

### 1. Версия workspace и зависимости

- `codex-rs/Cargo.toml`: workspace package version обновлен до `0.137.0`.
- `codex-rs/Cargo.lock`: обновлен под новую версию и зависимости.
- `codex-rs/app-server/Cargo.toml`: `codex-model-provider-info` используется в runtime-коде app-server.
- `codex-rs/codex-api/Cargo.toml`: добавлена зависимость для output truncation в Chat Completions адаптере.

### 2. Provider-aware model selection

- `codex-rs/protocol/src/openai_models.rs`: `ModelPreset` получает `model_provider`.
- `codex-rs/app-server-protocol/src/protocol/v2/model.rs`: v2 `Model` содержит `modelProvider`.
- `codex-rs/app-server-protocol/src/protocol/v2/account.rs`: `GetAccountParams` принимает `modelProvider`.
- `codex-rs/app-server/src/models.rs`: model catalog объединяет built-in и configured provider models.
- `codex-rs/app-server/src/request_processors/model_selection.rs`: вынесена логика выбора и валидации пары provider/model.
- `codex-rs/app-server/src/request_processors/thread_processor.rs`: thread start/resume/fork paths учитывают provider-aware selection.
- `codex-rs/tui/src/chatwidget/model_popups.rs`: model picker показывает provider-specific entries и сохраняет provider id.
- `codex-rs/tui/src/config_update.rs`: persisted model selection обновлен под provider-aware config.

### 3. Chat Completions-compatible API path

- `codex-rs/codex-api/src/endpoint/chat_completions.rs`: новый адаптер Responses-like requests в Chat Completions requests.
- `codex-rs/model-provider/src/provider.rs`: `wire_api = "chat"` влияет на capabilities и endpoint path.
- `codex-rs/core/tests/suite/client.rs` и `client_websockets.rs`: покрывают Chat/Responses behavior.
- Для Chat wire providers hosted `web_search` и `image_generation` не объявляются как доступные.

### 4. App-server v2 protocol/schema

- Обновлены JSON/TypeScript schema fixtures для `GetAccountParams`, `Model`, `Thread` и thread response/notification payloads.
- `codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs`: `Thread` содержит `model`.
- `codex-rs/app-server-protocol/src/protocol/common.rs`: experimental gating обновлен под новые API fields/methods.

### 5. Output truncation

- `codex-rs/utils/output-truncation/src/lib.rs`: добавлен общий механизм truncation.
- Core tool output paths используют configurable caps для shell/MCP/function output.
- Добавлены и обновлены тесты в `codex-rs/core/tests/suite/truncation.rs`, `core/src/tools/*_tests.rs` и unified exec tests.

### 6. Thread/session metadata

- `codex-rs/thread-store/src/types.rs` и local helpers обновлены под model metadata.
- `codex-rs/core/src/session/turn_context.rs` и session state paths учитывают effective provider/model fields.
- `codex-rs/rollout/src/*`: rollout list/recorder/tests обновлены под thread metadata changes.

### 7. TUI

- `codex-rs/tui/src/app_event.rs`: `PersistModelSelection` несет provider.
- `codex-rs/tui/src/app/event_dispatch.rs`: persistence path обновлен.
- `codex-rs/tui/src/chatwidget/model_popups.rs`: provider-aware picker.
- `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__custom_provider_model_picker.snap`: snapshot покрытия custom provider picker.
- TUI tests обновлены под provider-aware selection и thread `model` metadata.

## Новые файлы в `origin/main...HEAD`

- `FORK_VS_MAIN.md`.
- `codex-rs/app-server/src/models_tests.rs`.
- `codex-rs/app-server/src/request_processors/model_selection.rs`.
- `codex-rs/app-server/src/request_processors/turn_processor_tests.rs`.
- `codex-rs/app-server/tests/suite/v2/custom_chat_provider.rs`.
- `codex-rs/codex-api/src/endpoint/chat_completions.rs`.
- `codex-rs/codex-api/src/endpoint/chat_completions_tests.rs`.
- `codex-rs/tui/src/chatwidget/snapshots/codex_tui__chatwidget__tests__custom_provider_model_picker.snap`.

## Наиболее крупные области diff

- `codex-rs/codex-api/src/endpoint/`: Chat Completions adapter and tests.
- `codex-rs/app-server/src/request_processors/`: provider-aware model selection and thread request handling.
- `codex-rs/app-server/tests/suite/v2/`: v2 integration coverage for account/config/model/thread flows.
- `codex-rs/core/src/tools/` and `codex-rs/core/tests/suite/`: output truncation behavior.
- `codex-rs/app-server-protocol/schema/`: regenerated API schemas.
- `codex-rs/tui/src/chatwidget/`: provider-aware picker and snapshot coverage.

## Незакоммиченные review-fix правки поверх `HEAD`

На момент обновления отчета в рабочем дереве есть незакоммиченные исправления замечаний ревью. Они не входят в `origin/main...HEAD`, пока не будут закоммичены, но должны попасть в итоговую ветку:

- `FORK_VS_MAIN.md`: отчет актуализирован под `origin/main=765309d5...` и `HEAD=ebd605c4...`.
- `codex-rs/app-server/src/request_processors/account_processor.rs`: `getAccount(modelProvider=...)` теперь ищет built-in providers, а не только `config.model_providers`.
- `codex-rs/app-server/src/models.rs`: provider-less presets больше не принудительно маркируются как `openai`; `None` сохраняет смысл default/current provider.
- `codex-rs/tui/src/chatwidget/model_popups.rs`: TUI picker применяет тот же provider-less contract и не переписывает selection на `openai`.
- `codex-rs/app-server/src/config_manager_service.rs`: `env_key` и `env_key_instructions` больше не редактируются как секреты.
- `codex-rs/app-server/README.md`: documented conservative redaction для `http_headers`, `env_http_headers`, `auth`, `aws`.
- `codex-rs/codex-api/src/endpoint/chat_completions.rs`: orphan tool output без предшествующего tool call пропускается с warning.
- `codex-rs/tui/src/app/loaded_threads.rs`, `app/tests.rs`, `app/thread_session_state.rs`, `app_server_session.rs`, `resume_picker.rs`: test fixtures дополнены `Thread.model`.

## Проверки после review-fix правок

Уже запускались после внесения исправлений:

- `just fmt`
- `just test -p codex-app-server`: 873 passed, 11 leaky, 6 skipped.
- `just test -p codex-api`: 140 passed.
- `just test -p codex-tui`: 2900 passed, 36 leaky, 9 skipped.
- `just fix -p codex-app-server`
- `just fix -p codex-api`
- `just fix -p codex-tui`
- `git diff --check`: ошибок нет; Git показал только Windows LF/CRLF warnings.

## Потенциально важные behavioral differences

- Модель теперь не всегда однозначно задается одной строкой `model`: для custom providers используется пара `modelProvider + model`.
- Provider-less preset означает current/default provider, а не обязательно `openai`.
- Chat-compatible providers используют `chat/completions`, а не Responses API.
- Для Chat-compatible providers hosted web/image tools считаются недоступными.
- Tool output может обрезаться строже при configured `output_truncation`.
- `request_user_input` в Default mode становится stable/default capability.
