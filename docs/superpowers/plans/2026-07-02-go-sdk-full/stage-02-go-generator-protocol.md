# Stage 2: Go Generator And Protocol Package

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Generate Go protocol types, nullable wrappers, request IDs, raw typed RPC methods, method metadata, and digest constants from Rust-owned manifest/schema inputs plus the reviewed Go-owned resource API mapping.

**Architecture:** `sdk/go/internal/cmd/protocodex` consumes checked-in app-server schema bundles, the Rust manifest, and a reviewed Go-owned resource API mapping, then writes deterministic generated files under `sdk/go/protocol` plus root-package generated integration files for handlers and resource coverage. The generated `protocol` package does not import root `codex`.

**Tech Stack:** Go generator, `encoding/json`, JSON Schema fixtures, golden tests, generated Go code.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:212-381`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:641-668`
- `codex-rs/app-server-protocol/schema/json/codex_app_server_protocol.schemas.json`
- `codex-rs/app-server-protocol/schema/json/codex_app_server_protocol.v2.schemas.json`
- Stage 1 checked-in generated inputs:
  - stable schema root `codex-rs/app-server-protocol/schema/{json,typescript}`
  - `sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json`
  - `sdk/go/internal/protocodex/schema-experimental/json`
  - `sdk/go/internal/protocodex/schema-experimental/typescript`.

## Files

- Create/modify: `sdk/go/internal/cmd/protocodex/main.go`
- Create: `sdk/go/internal/protocodex/schema.go`
- Create: `sdk/go/internal/protocodex/manifest.go`
- Create: `sdk/go/internal/protocodex/names.go`
- Create: `sdk/go/internal/protocodex/types.go`
- Create: `sdk/go/internal/protocodex/resources.go`
- Create: `sdk/go/internal/protocodex/resource_mapping.go`
- Create: `sdk/go/internal/protocodex/render.go`
- Create: `sdk/go/internal/protocodex/generator_test.go`
- Create: `sdk/go/internal/protocodex/testdata/*`
- Read checked-in input: `sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json`
- Read stable schema input tree: `codex-rs/app-server-protocol/schema/{json,typescript}`
- Include stable schema fixture changes in Stage 2 scope when they are not already landed: if `codex-rs/app-server-protocol/schema/{json,typescript}` is dirty or updated as the source input for Go generation, Stage 2 must commit those fixture changes with `sdk/go` and validate them with the Rust app-server-protocol checks below. Do not claim Stage 2 from `sdk/go` output generated against an uncommitted schema tree.
- Include Rust app-server-protocol source/config/test prerequisites in Stage 2 scope when they are not already landed and they define the manifest/schema basis consumed by Go generation: `codex-rs/app-server-protocol/{Cargo.toml,BUILD.bazel,src,tests}`. If this includes Rust dependency changes, Stage 2 must also include `codex-rs/Cargo.lock`, run `just bazel-lock-update` from the repo root, and include `MODULE.bazel.lock` if that command updates it. Stage 2 must either start from a clean/landed Rust protocol source-input commit or include those Rust protocol/exporter changes with the schema fixtures, lockfiles, and `sdk/go`; do not validate generated Go against uncommitted Rust source changes outside the commit scope.
- Include app-server digest handshake prerequisites in Stage 2 scope when they are not already landed and they produce or document the initialize digest fields consumed by the generated Go metadata: `codex-rs/app-server/README.md`, `codex-rs/app-server/src/request_processors/initialize_processor.rs`, `codex-rs/app-server/tests/common/test_app_server.rs`, and `codex-rs/app-server/tests/suite/v2/initialize.rs`. Stage 2 must either start from a clean/landed app-server prerequisite commit or include and verify these app-server files with the protocol/schema/Go generated output; do not claim digest compatibility while the runtime/docs/tests that surface those digests remain outside the commit/check boundary.
- Read checked-in input tree: `sdk/go/internal/protocodex/schema-experimental/{json,typescript}`
- Generate: `sdk/go/protocol/*.go`
- Create: `sdk/go/protocol/optional.go`
- Create: `sdk/go/protocol/request_id.go`
- Generate: `sdk/go/protocol/client_notifications.go`
- Create: `sdk/go/protocol/raw_client.go`
- Create: `sdk/go/protocol/metadata.go`
- Generate: `sdk/go/protocol/server_request_metadata.go`
- Generate: `sdk/go/handlers_generated.go`
- Generate: `sdk/go/resource_coverage_generated.go`
- Generate checked inventory: `sdk/go/internal/protocodex/current_protocol_inventory.generated.md`
- Create: `sdk/go/protocol/protocol_test.go`

## Tasks

### Task 2.0: Verify Checked-In Generator Inputs

- [ ] Confirm Stage 1 produced:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
test -f sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
test -d codex-rs/app-server-protocol/schema/json
test -d codex-rs/app-server-protocol/schema/typescript
test -d sdk/go/internal/protocodex/schema-experimental/json
test -d sdk/go/internal/protocodex/schema-experimental/typescript
```

- [ ] Run the Rust check-mode commands from Stage 1 before implementing Go parsing:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --check --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
```

Expected: both commands pass without modifying the worktree.

- [ ] If `codex-rs/app-server-protocol/schema/{json,typescript}` has fixture diffs after these checks or before Go generation, treat those diffs as Stage 2 source-input changes rather than local setup noise:
  - either land them before Stage 2, or include them in the Stage 2 commit with `sdk/go`;
  - run the Rust schema/protocol validation from `codex-rs`;
  - do not accept Go generator `--check` output as sufficient when it was produced from dirty stable schema fixtures that will not be committed.
- [ ] If `codex-rs/app-server-protocol/{Cargo.toml,BUILD.bazel,src,tests}` is dirty and those source/config/test changes define the manifest/schema fixture basis consumed by Stage 2, treat them as source-input changes rather than local setup noise:
  - either land them before Stage 2, or include them in the Stage 2 commit with `sdk/go` and generated schema fixtures;
  - run the Rust schema/protocol validation from `codex-rs`;
  - do not commit Stage 2 from generated Go output that only reproduces with uncommitted Rust protocol/exporter sources.
- [ ] If `codex-rs/app-server-protocol/Cargo.toml` changes Rust dependencies, run `just bazel-lock-update` from the repository root and include `codex-rs/Cargo.lock` plus `MODULE.bazel.lock` if it changes. A verified no-op `MODULE.bazel.lock` result is acceptable, but it must be recorded in Stage 2 verification evidence.

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
just bazel-lock-update
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just test -p codex-app-server-protocol
```

### Task 2.1: Define Go-Owned Resource API Mapping Input

- [ ] Create `sdk/go/internal/protocodex/resource_mapping.go` as a handwritten, reviewed generator input. Rust manifest/schema remains the wire/protocol source of truth; this file owns Go SDK public API decisions that cannot be derived from Rust:

```go
package protocodex

type ResourceAPIMapping struct {
	Method                 string
	ResourceOwner          string
	WrapperName            string
	WrapperFile            string
	PublicSignature        string
	SignatureConventionID  string
	CompileCallsite        string
	UnitTestOwner          string
	SafeIntegrationOwner   string
	SafeIntegrationReason  string
	DocsExampleOwner       string
	ServerHandlerLinks     []string
	GeneratedOnlyException string
	ReviewNote            string
}

type ServerHandlerMapping struct {
	Method                 string
	HandlerOwner           string
	Visibility             string
	Capability             string
	UnitTestOwner          string
	DocsExampleOwner       string
	GeneratedOnlyException string
	ReviewNote             string
}

var resourceAPIMappings []ResourceAPIMapping
var serverHandlerMappings []ServerHandlerMapping
```

- [ ] Populate `resourceAPIMappings` by copying the reviewed rows from `appendix-current-protocol-inventory.md#reviewed-go-resource-api-mapping-seed`. Do not invent owners, wrapper names, signature conventions, tests, docs owners, or integration decisions in Stage 2. Any current method missing from the reviewed appendix blocks Stage 2 until the appendix is updated and fresh-reviewed.
- [ ] Populate `serverHandlerMappings` by copying the reviewed rows from `appendix-current-protocol-inventory.md#reviewed-server-handler-mapping-seed`. Do not infer handler owner, public/compatibility visibility, docs owners, or capability behavior from method prefixes.
- [ ] Each SDK-public first-class wrapper row must include `ResourceOwner`, `WrapperName`, `WrapperFile`, either exact `PublicSignature` or `SignatureConventionID`, `CompileCallsite`, `UnitTestOwner`, `DocsExampleOwner`, and either `SafeIntegrationOwner` or `SafeIntegrationReason`.
- [ ] Each generated-only raw-protocol row must include generated raw method name, `GeneratedOnlyException`, `ReviewNote`, `UnitTestOwner`, and either `SafeIntegrationOwner` or `SafeIntegrationReason`; it must not require `WrapperName`, `WrapperFile`, `PublicSignature`, or public wrapper docs.
- [ ] For any row with `SignatureConventionID == "high-level"`, `CompileCallsite` must use root SDK types such as `codex.ThreadStartOptions`, `codex.ThreadResumeOptions`, `codex.ThreadForkOptions`, `codex.Text(...)`, and `codex.TurnOptions`; it must fail validation if the primary public callsite uses `protocol.*Params` unless a reviewed exception is present. Generated `protocol.*Params` remain reachable through `Raw()` and thin resource rows only.
- [ ] For any row with `SignatureConventionID == "handle-start"`, the generator must consult the Rust manifest lifecycle/identity metadata. If the start request has client-supplied connection-scoped identity such as `processId`, `watchId`, `processHandle`, `sessionId`, `loginId`, or a realtime session identity, the public `CompileCallsite` and `PublicSignature` must use root SDK option types and the SDK must generate/inject that identity. Validation must fail if such a row exposes `protocol.*Params` as the primary public callsite. Generated raw protocol methods remain available through `Raw()`.
- [ ] Each generated-only row is SDK-public through generated `Raw()` protocol access only. It must include `GeneratedOnlyException` and `ReviewNote`, must generate a public `Raw()` method, must not silently inherit a resource owner from the wire method prefix, and must not require first-class resource docs/examples unless the reviewed row names them.
- [ ] Each compatibility-only, handshake-only, internal-test-only, or excluded row must include an exception and `ReviewNote`; it must not generate a public `Raw()` method, ergonomic wrapper, or public docs/examples.
- [ ] Each server-handler row must include `HandlerOwner`, `Visibility`, `Capability`, `UnitTestOwner`, `DocsExampleOwner`, and either public handler behavior or a non-public compatibility/internal exception with `ReviewNote`.
- [ ] Add mapping validation tests that read the selected stable schema bundle, selected experimental schema bundle, and Rust manifest, then fail on:
  - duplicate `Method`
  - mapping row for a method absent from schema/manifest
  - SDK-public method with no mapping row
  - SDK-public first-class wrapper method with no wrapper/signature/callsite/test/docs owner
  - SDK-public generated-only method that incorrectly requires a wrapper/signature/callsite or lacks raw method/test/safe-integration metadata
  - high-level row whose compile callsite exposes `protocol.*Params` instead of root SDK option/input/result types
  - handle-start row whose lifecycle metadata says identity is client-supplied but whose public callsite or signature exposes `protocol.*Params` instead of a root SDK option type
  - generated-only method with no public generated `Raw()` method or no exception/review note explaining why it has no ergonomic wrapper
  - compatibility-only, handshake-only, internal-test-only, or excluded method that generates a public `Raw()` method, ergonomic wrapper, or public docs/examples
  - server request with no `serverHandlerMappings` row
  - server request with no handler owner, visibility, capability, test owner, or docs owner
  - resource owner not present in the approved resource owner set
  - `CompileCallsite` that does not name the declared wrapper.
- [ ] Stage 2 generation must consume schema + Rust manifest + `resourceAPIMappings` + `serverHandlerMappings`. Do not derive public Go resource names, wrapper signatures, server-handler owners, docs owners, or test owners from Rust method prefixes.

### Task 2.2: Add Protocol Helper Types

- [ ] Create `sdk/go/protocol/optional.go` with nullable/tri-state helpers:

```go
package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
)

type Optional[T any] struct {
	value T
	set   bool
	null  bool
}

func Some[T any](value T) Optional[T] {
	return Optional[T]{value: value, set: true}
}

func Null[T any]() Optional[T] {
	return Optional[T]{set: true, null: true}
}

func (o Optional[T]) IsSet() bool { return o.set }
func (o Optional[T]) IsNull() bool { return o.set && o.null }
func (o Optional[T]) Value() (T, bool) {
	return o.value, o.set && !o.null
}

func (o Optional[T]) MarshalJSON() ([]byte, error) {
	if !o.set {
		return nil, fmt.Errorf("cannot marshal unset Optional directly; generated structs must omit unset fields")
	}
	if o.null {
		return []byte("null"), nil
	}
	return json.Marshal(o.value)
}

func (o *Optional[T]) UnmarshalJSON(data []byte) error {
	o.set = true
	if bytes.Equal(data, []byte("null")) {
		o.null = true
		var zero T
		o.value = zero
		return nil
	}
	o.null = false
	return json.Unmarshal(data, &o.value)
}

type OptionalNonNull[T any] struct {
	value T
	set   bool
}

func SomeNonNull[T any](value T) OptionalNonNull[T] {
	return OptionalNonNull[T]{value: value, set: true}
}

func (o OptionalNonNull[T]) IsSet() bool { return o.set }
func (o OptionalNonNull[T]) Value() (T, bool) { return o.value, o.set }

func (o OptionalNonNull[T]) MarshalJSON() ([]byte, error) {
	if !o.set {
		return nil, fmt.Errorf("cannot marshal unset OptionalNonNull directly; generated structs must omit unset fields")
	}
	return json.Marshal(o.value)
}

func (o *OptionalNonNull[T]) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return fmt.Errorf("optional non-null field cannot be null")
	}
	o.set = true
	return json.Unmarshal(data, &o.value)
}
```

- [ ] Generated structs must use custom `MarshalJSON` where needed so optional non-null and optional nullable fields are omitted when unset. Do not rely on directly marshaling an unset wrapper value.
- [ ] Add tests proving:
  - optional nullable unset is omitted
  - optional nullable explicit null encodes `null`
  - optional non-null unset is omitted
  - direct `json.Marshal(protocol.SomeNonNull(value))` encodes the value
  - direct `json.Marshal(protocol.OptionalNonNull[T]{})` returns an unset-wrapper error
  - a generated struct with a set optional non-null field encodes the field value
  - optional non-null explicit JSON `null` is rejected
  - default/skip bool rejects explicit `null`.

- [ ] Create `sdk/go/protocol/request_id.go`:

```go
package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

type RequestID struct {
	stringValue string
	intValue    int64
	kind        requestIDKind
}

type requestIDKind uint8

const (
	requestIDUnset requestIDKind = iota
	requestIDString
	requestIDInt
)

func StringRequestID(value string) RequestID {
	return RequestID{stringValue: value, kind: requestIDString}
}

func IntRequestID(value int64) RequestID {
	return RequestID{intValue: value, kind: requestIDInt}
}

func (id RequestID) MarshalJSON() ([]byte, error) {
	switch id.kind {
	case requestIDString:
		return json.Marshal(id.stringValue)
	case requestIDInt:
		return json.Marshal(id.intValue)
	default:
		return nil, fmt.Errorf("unset request id")
	}
}

func (id *RequestID) UnmarshalJSON(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	var value any
	if err := decoder.Decode(&value); err != nil {
		return err
	}
	if err := decoder.Decode(&struct{}{}); err != io.EOF {
		return fmt.Errorf("request id must be a single JSON string or integer")
	}
	switch value := value.(type) {
	case string:
		*id = StringRequestID(value)
		return nil
	case json.Number:
		if strings.ContainsAny(value.String(), ".eE") {
			return fmt.Errorf("request id integer must not be a floating-point number")
		}
		n, err := value.Int64()
		if err != nil {
			return fmt.Errorf("request id integer out of range: %w", err)
		}
		*id = IntRequestID(n)
		return nil
	default:
		return fmt.Errorf("request id must be string or integer")
	}
}
```

- [ ] Add `RequestID` tests proving unmarshalling accepts JSON strings and signed integers, rejects `null`, booleans, floats, exponent notation, objects, arrays, and multiple JSON values, and preserves canonical marshal output for accepted values.

### Task 2.3: Implement Generator Input Parsing

- [ ] Implement schema bundle parsing in `internal/protocodex/schema.go`.
- [ ] Implement manifest parsing in `internal/protocodex/manifest.go`.
- [ ] Manifest parsing must accept all four protocol directions and reject direction/type mismatches: `clientToServer` only for client requests, `serverToClient` only for server requests, `serverNotification` only for server notifications, and `clientNotification` only for client notifications.
- [ ] Manifest parsing must keep `requestSerializationScopes` separate from `serdeShapeRequirement`. For client-to-server methods, parse all current Rust `ClientRequestSerializationScope` kinds (`global`, `globalSharedRead`, `thread`, `threadPath`, `commandExecProcess`, `process`, `fuzzyFileSearchSession`, `fsWatch`, `mcpOauth`) plus conditional discriminator and identity extractor paths; reject unknown scope kinds, duplicate or overlapping conditional branches, missing extractors for identity-bearing scopes, and non-empty request serialization scopes on non-client-to-server entries.
- [ ] Implement resource API and server-handler mapping validation in `internal/protocodex/resources.go` using `resourceAPIMappings` and `serverHandlerMappings` as inputs, not as generated output.
- [ ] Add tests with testdata for:
  - `$ref`
  - single-ref `allOf`
  - nullable `anyOf`
  - `additionalProperties`
  - integer formats and bounds
  - tagged unions
  - field and variant aliases
  - notification direction values `serverNotification` and `clientNotification`
  - missing SDK-public resource mapping row
  - missing server-handler mapping row for experimental `currentTime/read`.

- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./internal/protocodex
```

Expected: parser tests pass before rendering full code.

### Task 2.4: Implement Deterministic Naming

- [ ] Implement `internal/protocodex/names.go`:
  - exported Go type names
  - enum constant names
  - reserved word handling
  - duplicate name suffixing
  - method name mapping from wire methods.
- [ ] Add golden tests for reserved words, duplicate names, enum collisions, and method collisions.

### Task 2.5: Implement Type Rendering

- [ ] Implement `internal/protocodex/types.go` and `render.go` for:
  - required/optional/nullable field shapes
  - fixed-width integer formats
  - maps/open objects
  - tagged and untagged unions with raw fallback
  - unknown enum values
  - field and variant aliases from manifest serde metadata
  - default values and default providers from manifest serde metadata, including default-true booleans and default collections
  - `skip_serializing_if` predicates from manifest serde metadata
  - flattened maps and flattened object fields
  - custom serialize/deserialize hooks from manifest serde metadata.
- [ ] Generate presence-tracking `UnmarshalJSON` for every struct with required fields. Missing required non-null fields, missing required nullable fields, and explicit `null` for required non-null fields must return typed decode errors with the field name; do not allow standard `encoding/json` zero values to stand in for absent protocol data. Generated custom unmarshalling must preserve unknown/raw fallback behavior for unions while still enforcing required fields inside the selected concrete payload.
- [ ] The generated `protocol.InitializeResponse` must follow the same required-field rule. Do not add a generator-level exception that lets current `InitializeResponse` accept the pre-Stage-1 legacy shape without digest/mode fields. Stage 3 may add a Go-internal, hand-owned compatibility envelope for raw initialize response bytes, but that envelope must live outside generated protocol output and must not weaken required-field decoding for any generated current protocol type.
- [ ] The generator must compute the same transitive reachable schema/type set as the Rust manifest tests and fail when any reachable definition lacks either schema-sufficient proof or manifest serde metadata. Do not infer Rust defaults, aliases, flattened fields, or custom serde hooks from JSON Schema alone.
- [ ] For every reachable type with `metadataStatus == SchemaSufficient`, the generator must require `schemaSufficientProof` and fail if the proof does not explicitly rule out aliases, defaults, skip predicates, flatten, custom serde hooks, and schema-lost union/nullability shape.
- [ ] Add generated fixture tests that marshal/unmarshal:
  - `AppScreenshot.fileId` and `file_id`
  - `AppScreenshot.userPrompt` and `user_prompt`
  - `FileSystemSpecialPath.ProjectRoots` with `project_roots` and `current_working_directory`
  - a default-true field omitted on the wire
  - a default collection omitted on the wire
  - a nested alias below a response/request payload root
  - a flattened map/object field
  - a custom deserialize path
  - unsigned integer negative/overflow rejection
  - missing required scalar field rejection
  - missing required object field rejection
  - missing required enum/tagged-union discriminator rejection
  - missing required nullable field rejection
  - explicit `null` rejection for a required non-null field.

### Task 2.6: Generate Raw Client, Handler Metadata, And Resource Matrix

- [ ] Generate typed `ClientNotification` structs/union helpers in `protocol/client_notifications.go`, including `initialized`.
- [ ] Validate that the selected complete schema source contains `ClientNotification` before generation. The generator must fail if it tries to resolve `ClientNotification` only from the flat v2 bundle while that bundle does not contain the union.
- [ ] Add a drift test that fails when any future client-to-server notification lacks generated coverage or an explicit manifest exception.
- [ ] Add generator/protocol tests proving the generated `ClientNotification` union contains `initialized`, serializes to the exact `initialized` wire method, and fails closed if future client notifications lack generated coverage. Do not add runtime/client handshake behavior in Stage 2.
- [ ] Generate `protocol/raw_client.go` with one typed method per SDK-public generated client-to-server method except `initialize`.
- [ ] `protocol/raw_client.go` must use a protocol-owned sender interface so generated protocol code does not import the root `codex` package:

```go
package protocol

import "context"

type Sender interface {
	Call(ctx context.Context, method string, params any, result any, metadata MethodMetadata) error
}
```

- [ ] Generate `protocol/metadata.go` with:
  - stable/experimental protocol digest constants
  - method metadata, including params type, response type, `paramsSchemaRef`, `responseSchemaRef`, `inspectParams`, manual payload conversion marker, schema-exclusion reason, bounded model-context field metadata, visibility, experimental status, retry policy, and experimental field paths
  - experimental field paths
  - retry policy
  - visibility classifications.
- [ ] Generator validation must fail closed when a manifest `paramsSchemaRef`, `responseSchemaRef`, or notification `payloadSchemaRef` is missing without an allowed non-public schema-exclusion reason, resolves outside the selected schema bundle, or resolves to a different payload/response type. Known manual payload conversion markers must be preserved in generated metadata; unknown or empty manual payload conversion markers must fail generation instead of being silently dropped.
- [ ] Ensure `initialize` params/response types exist but no callable `Raw().Initialize` exists.
- [ ] Generate `protocol/server_request_metadata.go` with a complete `ServerRequestMetadata` table from schema + Rust manifest + `serverHandlerMappings`, covering every `ServerRequest`, including SDK-public, deprecated legacy compatibility, internal-test-only, and unsupported-by-default rows. Each row must include handler owner, visibility, capability, generated decode function, unit test owner, `docsExampleOwner` for public rows, and compatibility exception where applicable. Compatibility-only rows must decode/dispatch internally or return the reviewed unsupported behavior, but must not generate public handler fields or public docs/examples.
- [ ] Generate root package `sdk/go/handlers_generated.go` with:
  - one typed optional handler field or interface method per SDK-public server request
  - adapter function types for function-style registration
  - generated decode/dispatch helpers that call typed handlers
  - compatibility hook metadata for non-public server requests.
- [ ] Replace the Stage 0 `ServerHandlers` skeleton with generated-owned definitions. Keep handwritten convenience helpers in `sdk/go/handlers.go` only when they do not duplicate generated type names.
- [ ] Generate `sdk/go/resource_coverage_generated.go` with a method-level matrix from schema + Rust manifest + `resourceAPIMappings`. Each row must include wire method, `sdkVisibility`, resource owner, generated raw method name, root wrapper method name or explicit exception reason, exact `publicSignature` or strict signature-convention id, compile-test callsite, required unit test name, safe integration test name when applicable, and docs/example owner.
- [ ] Generate `sdk/go/internal/protocodex/current_protocol_inventory.generated.md` from schema + Rust manifest + `resourceAPIMappings` + `serverHandlerMappings`. It must enumerate, grouped by resource owner:
  - every SDK-public client-to-server method
  - wrapper method name and owning file
  - generated raw method name
  - unit test owner
  - safe integration test owner or not-applicable reason
  - docs/example owner
  - server notifications consumed by that resource
  - server requests/handler capabilities related to that resource
  - client notifications, including `initialized`.
- [ ] Add an independent inventory extraction check that reads the selected stable schema bundle, selected experimental schema bundle, Rust-owned manifest, `resourceAPIMappings`, and `serverHandlerMappings`. The check must fail if any current method/request/notification appears in schema or manifest but is missing from `current_protocol_inventory.generated.md`, if any SDK-public method lacks a Go-owned resource mapping row, or if any server request lacks a reviewed handler mapping row.
- [ ] Compare the generated inventory against `docs/superpowers/plans/2026-07-02-go-sdk-full/appendix-current-protocol-inventory.md`. The appendix is only a reviewed seed: any current schema method listed in the appendix that is absent from the generated manifest inventory must fail generation unless it has a manifest exception with `sdkVisibility`, owner, and review note, but passing the appendix comparison alone is not sufficient.
- [ ] Add contract tests asserting known currently-required experimental entries are present in generated metadata and marked experimental:
  - `thread/realtime/start`
  - `thread/settings/update`
  - `memory/reset`
  - `collaborationMode/list`
  - `process/spawn`
  - `fuzzyFileSearch/sessionStart`.
- [ ] Add contract tests asserting legacy-but-public `fuzzyFileSearch` and session-based `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop` are owned by the `FuzzyFileSearch` resource rather than silently hidden as v1 compatibility.
- [ ] Add tests proving every `ServerRequest` has handler or compatibility metadata and every SDK-public client method has a resource-coverage matrix row.
- [ ] Add tests proving every `ServerNotification` has generated routing metadata derived from `NotificationRoutingStrategy`. The generator must fail if it would need to infer routing from method-name prefixes or handwritten Go tables.

### Task 2.7: Add Generator Command And Drift Check

- [ ] Implement `internal/cmd/protocodex/main.go` flags:
  - `--stable-schema-root`
  - `--experimental-schema-root`
  - `--manifest`
  - `--out`
  - `--root-out`
  - `--check`
  - `--mode stable|experimental|both`.
- [ ] Define mode semantics exactly:
  - `--mode both` is the only mode that writes or checks the canonical checked-in Go public output under `sdk/go/protocol` and root generated files. It consumes the experimental superset and emits stable/experimental metadata plus stable-mode pre-write gates in one type graph.
  - `--mode stable --check` performs metadata-only validation into a temporary directory: stable schema entries must be present in the canonical output, stable mode must reject experimental methods/fields, and no stable-only public output is written or compared.
  - `--mode experimental --check` performs metadata-only validation into a temporary directory: experimental schema entries and gating metadata must be present in the canonical output, and no experimental-only public output is written or compared.
  - `--mode stable` or `--mode experimental` without `--check` must fail with a usage error so the executor cannot accidentally overwrite the canonical output with a subset graph.
- [ ] Add generator tests that run into a temp directory and compare deterministic output.
- [ ] Add drift tests for stable and experimental generation separately:
  - `--mode stable --check` must validate stable subset/gating metadata against the canonical checked-in output without comparing a separate stable file tree
  - `--mode experimental --check` must validate experimental additions/gating metadata against the canonical checked-in output without comparing a separate experimental file tree
  - `--mode both --check` must compare the canonical checked-in public output and fail if either stable or experimental schema roots are omitted.
- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go run ./internal/cmd/protocodex --mode both --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
go run ./internal/cmd/protocodex --check --mode stable --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
go run ./internal/cmd/protocodex --check --mode experimental --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
go run ./internal/cmd/protocodex --check --mode both --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
go test ./...
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just test -p codex-app-server-protocol
just test -p codex-app-server initialize_response_includes_protocol_digests_without_runtime_schema_tree initialize_uses_client_info_name_as_originator
```

Do not claim broad `just test -p codex-app-server` success from the scoped initialize verification. If a broad local package run is attempted and the only failures are unrelated `suite::v2::command_exec::*` assertions caused by the reproducible Linux sandbox wrapper stderr `Failed to create stream fd: Operation not permitted` (also reproducible with `codex sandbox -c sandbox_mode="read-only"`), record that as a local environment blocker for the broad suite and keep Stage 2 acceptance tied to the changed initialize digest boundary plus the protocol/generator checks above. Any app-server failure touching initialize, schema/manifest digest fields, protocol mode selection, or `TestAppServer::new_with_cwd` remains a Stage 2 blocker.

### Task 2.8: Commit Stage 2

- [ ] Commit:

```bash
git add sdk/go
git status --short --untracked-files=all -- sdk/go codex-rs/app-server-protocol codex-rs/app-server codex-rs/Cargo.lock MODULE.bazel.lock
git ls-files --others --exclude-standard -- sdk/go codex-rs/app-server-protocol codex-rs/app-server
# The untracked list must be empty after staging, or every listed file must be deliberately removed/regenerated before commit. In the current Stage 2 shape, required new files such as protocol helpers, generator tests, schema fixtures, and app-server digest tests must be staged rather than left as local-only inputs.
# If Rust protocol/exporter source/config/test changes or schema fixtures are dirty because Stage 2 generated Go from them, include them too:
git add codex-rs/app-server-protocol/Cargo.toml codex-rs/app-server-protocol/BUILD.bazel codex-rs/app-server-protocol/src codex-rs/app-server-protocol/tests
git add codex-rs/app-server-protocol/schema
# If Rust dependency changes are part of those source inputs, include lockfile updates too:
git add codex-rs/Cargo.lock MODULE.bazel.lock
# If app-server runtime/docs/tests emit or verify the initialize digest contract used by the generated Go metadata, include them in the same boundary:
git add codex-rs/app-server/README.md codex-rs/app-server/src/request_processors/initialize_processor.rs codex-rs/app-server/tests/common/test_app_server.rs codex-rs/app-server/tests/suite/v2/initialize.rs
git status --short --untracked-files=all -- sdk/go codex-rs/app-server-protocol codex-rs/app-server codex-rs/Cargo.lock MODULE.bazel.lock
git commit -m "feat(go-sdk): generate protocol bindings"
```

Do not commit Stage 2 as `sdk/go`-only while `codex-rs/app-server-protocol/{Cargo.toml,BUILD.bazel,src,tests,schema}`, `codex-rs/app-server/{README.md,src/request_processors/initialize_processor.rs,tests/common/test_app_server.rs,tests/suite/v2/initialize.rs}`, or required Rust lockfiles contain unlanded source-input/runtime changes used by the generator or digest handshake. If the Rust protocol/exporter, schema fixture, or app-server runtime/docs/tests update belongs to a separate earlier stage, land or verify that commit first, then regenerate/check `sdk/go` from the clean source-input tree. If `just bazel-lock-update` leaves `MODULE.bazel.lock` unchanged, record that no-op in verification evidence before committing. Required untracked files are a Stage 2 blocker until staged, intentionally deleted, or regenerated away and rechecked.

## Stage Review

Fresh blind engineering and product reviews are mandatory. Product review must inspect generated public names, raw method shape, sample callsites, and the generated resource inventory before later stages build handwritten wrappers on top.
