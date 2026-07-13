package codex

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"reflect"
	"runtime"
	"sort"
	"strings"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

type runtimeSource int

const (
	runtimeSourceInjected runtimeSource = iota
	runtimeSourceExplicitPath
	runtimeSourcePathLookup
)

var reservedRuntimeEnv = map[string]struct{}{
	"CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS":            {},
	"CODEX_APP_SERVER_DEV_OPEN_APP_URL":                   {},
	"CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG":             {},
	"CODEX_APP_SERVER_LOGIN_CLIENT_ID":                    {},
	"CODEX_APP_SERVER_LOGIN_ISSUER":                       {},
	"CODEX_APP_SERVER_MANAGED_CONFIG_PATH":                {},
	"CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE":          {},
	"CODEX_APP_SERVER_TEST_USER_CONFIG_FILE":              {},
	"CODEX_AUTHAPI_BASE_URL":                              {},
	"CODEX_CODE_MODE_HOST_PATH":                           {},
	"CODEX_EXEC_SERVER_NOISE_AUTH_TOKEN":                  {},
	"CODEX_EXEC_SERVER_NOISE_CHATGPT_ACCOUNT_ID":          {},
	"CODEX_EXEC_SERVER_NOISE_ENVIRONMENT_ID":              {},
	"CODEX_EXEC_SERVER_NOISE_REGISTRY_URL":                {},
	"CODEX_EXEC_SERVER_URL":                               {},
	"CODEX_INTERNAL_ORIGINATOR_OVERRIDE":                   {},
	"CODEX_REFRESH_TOKEN_URL_OVERRIDE":                    {},
	"CODEX_REVOKE_TOKEN_URL_OVERRIDE":                     {},
	"CODEX_TEST_ALLOW_HTTP_REMOTE_PLUGIN_BUNDLE_DOWNLOADS": {},
	"CODEX_TEST_RATE_LIMIT_RESET_REQUEST_TIMEOUT_MS":       {},
}

type rawOnlyHighLevelWorkflow struct {
	name        string
	startMethod string
}

var rawOnlyHighLevelWorkflows = []rawOnlyHighLevelWorkflow{
	{name: "account/browser-login", startMethod: "account/login/start"},
	{name: "account/device-code-login", startMethod: "account/login/start"},
	{name: "command/exec", startMethod: "command/exec"},
	{name: "fs/watch", startMethod: "fs/watch"},
	{name: "fuzzyFileSearch/sessionStart", startMethod: "fuzzyFileSearch/sessionStart"},
	{name: "mcpServer/oauth/login", startMethod: "mcpServer/oauth/login"},
	{name: "process/spawn", startMethod: "process/spawn"},
	{name: "realtime/start", startMethod: "thread/realtime/start"},
	{name: "remoteControl/pairing/start", startMethod: "remoteControl/pairing/start"},
	{name: "review/start", startMethod: "review/start"},
	{name: "thread/fork", startMethod: "thread/fork"},
	{name: "thread/resume", startMethod: "thread/resume"},
	{name: "thread/start", startMethod: "thread/start"},
	{name: "turn/start", startMethod: "turn/start"},
}

func validateClientConfig(cfg ClientConfig) (ClientConfig, error) {
	if cfg.ProtocolMode != ProtocolModeExperimental && cfg.ProtocolMode != ProtocolModeStable {
		return cfg, &ConfigError{Reason: "unknown protocol mode"}
	}
	if cfg.Compatibility != CompatibilityStrict &&
		cfg.Compatibility != CompatibilityAllowDevBuild &&
		cfg.Compatibility != CompatibilityAllowProtocolDigestUnavailable {
		return cfg, &ConfigError{Reason: "unknown compatibility policy"}
	}
	if cfg.Mode != ClientModeHighLevel && cfg.Mode != ClientModeRawOnly {
		return cfg, &ConfigError{Reason: "unknown client mode"}
	}
	limits, err := normalizeLimits(cfg.Limits)
	if err != nil {
		return cfg, err
	}
	cfg.Limits = limits
	if err := validateNotificationOptOuts(cfg.NotificationOptOuts); err != nil {
		return cfg, err
	}
	if cfg.Mode == ClientModeHighLevel {
		disabled := disabledImplementedHighLevelStartMethods(cfg.NotificationOptOuts)
		if len(disabled) > 0 {
			workflows := disabledHighLevelWorkflowMetadata(disabled)
			return cfg, &ConfigError{Reason: "notification opt-outs disable high-level workflows: " + strings.Join(workflows, "; ")}
		}
	}
	if cfg.ClientName == "" {
		cfg.ClientName = "codex_go_sdk"
	}
	if cfg.ClientVersion == "" {
		cfg.ClientVersion = "0.0.0-dev"
	}
	if cfg.Transport != nil {
		if cfg.CodexPath != "" || !reflect.ValueOf(cfg.Launch).IsZero() || cfg.CWD != "" ||
			len(cfg.Env) > 0 || len(cfg.ConfigOverrides) > 0 {
			return cfg, &ConfigError{Reason: "Transport cannot be combined with process launch fields"}
		}
	}
	if err := validateConfigOverrides(cfg.ConfigOverrides); err != nil {
		return cfg, err
	}
	for name := range cfg.Env {
		if isReservedEnv(name) {
			return cfg, &ConfigError{Reason: "reserved SDK runtime environment variable is not allowed: [redacted]"}
		}
	}
	return cfg, nil
}

func validateNotificationOptOuts(optOuts NotificationOptOuts) error {
	for _, method := range optOuts.Methods {
		if _, ok := protocol.ServerNotificationRoutingByMethod[method]; !ok {
			return &ConfigError{Reason: "unknown notification opt-out method: " + method}
		}
	}
	return nil
}

func disabledHighLevelStartMethods(optOuts NotificationOptOuts) map[string]string {
	optedOut := map[string]struct{}{}
	for _, method := range optOuts.Methods {
		optedOut[method] = struct{}{}
	}
	if len(optedOut) == 0 {
		return nil
	}
	disabled := map[string]string{}
	for startMethod, lifecycle := range protocol.RoutingLifecycleByStartMethod {
		var dependencies []string
		for _, method := range lifecycle.NotificationOptOutDependencies {
			if _, ok := optedOut[method]; ok {
				dependencies = append(dependencies, method)
			}
		}
		if len(dependencies) > 0 {
			sort.Strings(dependencies)
			disabled[startMethod] = strings.Join(dependencies, ", ")
		}
	}
	return disabled
}

func disabledImplementedHighLevelStartMethods(optOuts NotificationOptOuts) map[string]string {
	disabled := disabledHighLevelStartMethods(optOuts)
	if len(disabled) == 0 {
		return nil
	}
	implemented := map[string]string{}
	for _, workflow := range rawOnlyHighLevelWorkflows {
		if dependency, ok := disabled[workflow.startMethod]; ok {
			implemented[workflow.startMethod] = dependency
		}
	}
	return implemented
}

func disabledHighLevelWorkflowMetadata(disabled map[string]string) []string {
	if len(disabled) == 0 {
		return nil
	}
	startMethods := make([]string, 0, len(disabled))
	for startMethod := range disabled {
		startMethods = append(startMethods, startMethod)
	}
	sort.Strings(startMethods)
	workflows := make([]string, 0, len(startMethods))
	for _, startMethod := range startMethods {
		workflows = append(workflows, startMethod+" requires "+disabled[startMethod])
	}
	return workflows
}

func rawOnlyDisabledHighLevelWorkflowMetadata(disabled map[string]string) []string {
	workflows := make([]string, 0, len(rawOnlyHighLevelWorkflows))
	for _, workflow := range rawOnlyHighLevelWorkflows {
		if dependency, ok := disabled[workflow.startMethod]; ok {
			workflows = append(workflows, workflow.name+" requires "+dependency)
		} else {
			workflows = append(workflows, workflow.name+" disabled in raw-only mode")
		}
	}
	return workflows
}

func validateConfigOverrides(overrides map[string]string) error {
	for key, value := range overrides {
		if isSecretLike(key) || isSecretLike(value) || !isAllowedConfigOverride(key, value) {
			return &ConfigError{Reason: "unsupported or secret-like ConfigOverrides key/value rejected: [redacted]"}
		}
	}
	return nil
}

func isAllowedConfigOverride(key string, value string) bool {
	switch key {
	case "model":
		return isQuotedConfigIdentifier(value)
	case "sandbox_mode":
		switch value {
		case `"read-only"`, `"workspace-write"`, `"danger-full-access"`:
			return true
		}
	}
	return false
}

func isQuotedConfigIdentifier(value string) bool {
	if len(value) < 3 || value[0] != '"' || value[len(value)-1] != '"' {
		return false
	}
	for _, ch := range value[1 : len(value)-1] {
		if (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9') {
			continue
		}
		switch ch {
		case '-', '.', '/', ':', '_':
			continue
		default:
			return false
		}
	}
	return true
}

func isSecretLike(value string) bool {
	lower := strings.ToLower(value)
	for _, needle := range []string{"api_key", "apikey", "token", "secret", "password", "credential", "auth", "cookie"} {
		if strings.Contains(lower, needle) {
			return true
		}
	}
	return false
}

func resolveTransport(ctx context.Context, cfg ClientConfig) (jsonrpc.Transport, string, runtimeSource, error) {
	if cfg.Transport != nil {
		return publicTransportAdapter{transport: cfg.Transport, maxFrameBytes: cfg.Limits.MaxFrameBytes}, "", runtimeSourceInjected, nil
	}
	if cfg.CodexPath != "" {
		if _, err := os.Stat(cfg.CodexPath); err != nil {
			return nil, "", runtimeSourceExplicitPath, &RuntimeNotFoundError{
				Searched: []string{"ClientConfig.CodexPath"},
				Hint:     "set ClientConfig.CodexPath to a compatible Codex runtime",
			}
		}
		transport, err := startRuntime(ctx, cfg.CodexPath, cfg)
		return transport, cfg.CodexPath, runtimeSourceExplicitPath, err
	}
	path, err := exec.LookPath("codex")
	if err != nil {
		return nil, "", runtimeSourcePathLookup, &RuntimeNotFoundError{
			Searched: []string{"PATH lookup for codex"},
			Hint:     "install or update Codex, set ClientConfig.CodexPath, or use an injected transport",
		}
	}
	transport, err := startRuntime(ctx, path, cfg)
	return transport, path, runtimeSourcePathLookup, err
}

func startRuntime(ctx context.Context, path string, cfg ClientConfig) (jsonrpc.Transport, error) {
	args := buildRuntimeArgs(cfg)
	env := buildRuntimeEnv(os.Environ(), cfg.Env)
	return jsonrpc.StartStdio(ctx, jsonrpc.StdioOptions{
		Path:          path,
		Args:          args,
		Dir:           cfg.CWD,
		Env:           env,
		MaxFrameBytes: cfg.Limits.MaxFrameBytes,
		StderrBytes:   cfg.Limits.StderrRingBytes,
	})
}

func buildRuntimeArgs(cfg ClientConfig) []string {
	var args []string
	keys := make([]string, 0, len(cfg.ConfigOverrides))
	for key := range cfg.ConfigOverrides {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	for _, key := range keys {
		args = append(args, "--config", key+"="+cfg.ConfigOverrides[key])
	}
	args = append(args, "app-server", "--listen", "stdio://")
	return args
}

func buildRuntimeEnv(parent []string, overrides map[string]string) []string {
	return buildRuntimeEnvForOS(parent, overrides, runtime.GOOS)
}

type runtimeEnvEntry struct {
	key   string
	value string
}

func buildRuntimeEnvForOS(parent []string, overrides map[string]string, goos string) []string {
	env := make(map[string]runtimeEnvEntry)
	for _, item := range parent {
		key, value, ok := strings.Cut(item, "=")
		if !ok || isReservedEnv(key) {
			continue
		}
		env[runtimeEnvKeyID(key, goos)] = runtimeEnvEntry{key: key, value: value}
	}
	for key, value := range overrides {
		env[runtimeEnvKeyID(key, goos)] = runtimeEnvEntry{key: key, value: value}
	}
	entries := make([]runtimeEnvEntry, 0, len(env))
	for _, entry := range env {
		entries = append(entries, entry)
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].key < entries[j].key })
	out := make([]string, 0, len(entries))
	for _, entry := range entries {
		out = append(out, entry.key+"="+entry.value)
	}
	return out
}

func runtimeEnvKeyID(key string, goos string) string {
	if goos == "windows" {
		return strings.ToUpper(key)
	}
	return key
}

func isReservedEnv(name string) bool {
	_, ok := reservedRuntimeEnv[strings.ToUpper(name)]
	return ok
}

type publicTransportAdapter struct {
	transport     Transport
	maxFrameBytes int64
}

func (a publicTransportAdapter) Receive(ctx context.Context) (json.RawMessage, error) {
	frame, err := a.transport.Receive(ctx)
	if err != nil {
		return nil, err
	}
	if a.maxFrameBytes > 0 && int64(len(frame)) > a.maxFrameBytes {
		return nil, &jsonrpc.FrameSizeError{Limit: a.maxFrameBytes, Size: int64(len(frame))}
	}
	return frame, nil
}

func (a publicTransportAdapter) Send(ctx context.Context, frame json.RawMessage) error {
	if a.maxFrameBytes > 0 && int64(len(frame)) > a.maxFrameBytes {
		return &jsonrpc.FrameSizeError{Limit: a.maxFrameBytes, Size: int64(len(frame))}
	}
	return a.transport.Send(ctx, frame)
}

func (a publicTransportAdapter) Close() error {
	return a.transport.Close()
}

func (c *Client) initialize(ctx context.Context, cfg ClientConfig, source runtimeSource) error {
	capabilities := protocol.InitializeCapabilities{
		ExperimentalAPI:    protocol.SomeNonNull(cfg.ProtocolMode == ProtocolModeExperimental),
		RequestAttestation: protocol.SomeNonNull(cfg.Handlers.Attestation != nil),
	}
	if len(cfg.NotificationOptOuts.Methods) > 0 {
		capabilities.OptOutNotificationMethods = protocol.Some(cfg.NotificationOptOuts.Methods)
	}
	if cfg.Handlers.MCPElicitation != nil {
		capabilities.McpServerOpenaiFormElicitation = protocol.SomeNonNull(true)
	}
	params := protocol.InitializeParams{
		ClientInfo: protocol.ClientInfo{
			Name:    cfg.ClientName,
			Title:   protocol.Some("Codex Go SDK"),
			Version: cfg.ClientVersion,
		},
		Capabilities: protocol.Some(capabilities),
	}
	var raw json.RawMessage
	if err := c.rpc.Call(ctx, "initialize", params, &raw, nil); err != nil {
		return err
	}
	envelope, err := decodeInitializeCompatibility(raw)
	if err != nil {
		var compatibilityErr *CompatibilityError
		if errors.As(err, &compatibilityErr) {
			compatibilityErr.ExpectedDigest, _ = expectedProtocol(cfg.ProtocolMode)
			compatibilityErr.ExpectedMode = cfg.ProtocolMode
			compatibilityErr.RuntimePath = c.metadata.RuntimePath
		}
		return err
	}
	current, note, override, err := validateInitializeCompatibility(envelope, cfg, source, c.metadata.RuntimePath)
	if err != nil {
		return err
	}
	c.metadata.UserAgent = envelope.UserAgent
	c.metadata.PlatformFamily = envelope.PlatformFamily
	c.metadata.PlatformOS = envelope.PlatformOS
	c.metadata.RuntimeVersion = runtimeVersionFromUserAgent(envelope.UserAgent)
	c.metadata.CompatibilityOverrideActive = override
	c.metadata.CompatibilityNote = note
	if cfg.Mode == ClientModeRawOnly {
		c.metadata.DisabledHighLevelWorkflows = rawOnlyDisabledHighLevelWorkflowMetadata(c.disabledStart)
	} else if disabled := disabledHighLevelWorkflowMetadata(c.disabledStart); len(disabled) > 0 {
		c.metadata.DisabledHighLevelWorkflows = disabled
	}
	if current != nil {
		c.metadata.UserAgent = current.UserAgent
	}
	initialized := protocol.NewInitializedNotification()
	data, err := json.Marshal(initialized)
	if err != nil {
		return err
	}
	var notification struct {
		Method protocol.ClientNotificationMethod `json:"method"`
		Params json.RawMessage                   `json:"params,omitempty"`
	}
	if err := json.Unmarshal(data, &notification); err != nil {
		return err
	}
	if err := c.rpc.Notify(ctx, string(notification.Method), notification.Params, nil); err != nil {
		return err
	}
	return nil
}

type initializeCompatibilityEnvelope struct {
	Raw                        json.RawMessage
	UserAgent                  string
	CodexHome                  json.RawMessage
	PlatformFamily             string
	PlatformOS                 string
	StableProtocolDigest       *string
	ExperimentalProtocolDigest *string
	ActiveProtocolMode         *protocol.ActiveProtocolMode
}

func decodeInitializeCompatibility(raw json.RawMessage) (initializeCompatibilityEnvelope, error) {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return initializeCompatibilityEnvelope{}, err
	}
	required := []string{"userAgent", "codexHome", "platformFamily", "platformOs"}
	for _, field := range required {
		if len(fields[field]) == 0 {
			return initializeCompatibilityEnvelope{}, &CompatibilityError{Reason: "initialize response missing legacy core field " + field}
		}
	}
	var env initializeCompatibilityEnvelope
	env.Raw = append(json.RawMessage(nil), raw...)
	if err := json.Unmarshal(fields["userAgent"], &env.UserAgent); err != nil {
		return env, err
	}
	env.CodexHome = fields["codexHome"]
	if err := json.Unmarshal(fields["platformFamily"], &env.PlatformFamily); err != nil {
		return env, err
	}
	if err := json.Unmarshal(fields["platformOs"], &env.PlatformOS); err != nil {
		return env, err
	}
	if value, ok, err := optionalStringField(fields, "stableProtocolDigest"); err != nil {
		return env, err
	} else if ok {
		env.StableProtocolDigest = &value
	}
	if value, ok, err := optionalStringField(fields, "experimentalProtocolDigest"); err != nil {
		return env, err
	} else if ok {
		env.ExperimentalProtocolDigest = &value
	}
	if rawMode, ok := fields["activeProtocolMode"]; ok {
		var mode protocol.ActiveProtocolMode
		if err := json.Unmarshal(rawMode, &mode); err != nil {
			return env, err
		}
		env.ActiveProtocolMode = &mode
	}
	return env, nil
}

func optionalStringField(fields map[string]json.RawMessage, name string) (string, bool, error) {
	raw, ok := fields[name]
	if !ok || string(raw) == "null" {
		return "", false, nil
	}
	var value string
	if err := json.Unmarshal(raw, &value); err != nil {
		return "", false, err
	}
	return value, true, nil
}

func validateInitializeCompatibility(env initializeCompatibilityEnvelope, cfg ClientConfig, source runtimeSource, runtimePath string) (*protocol.InitializeResponse, string, bool, error) {
	expectedDigest, expectedMode := expectedProtocol(cfg.ProtocolMode)
	observedDigest := observedProtocolDigest(env, cfg.ProtocolMode)
	modeMatches := env.ActiveProtocolMode != nil && *env.ActiveProtocolMode == expectedMode
	if cfg.Compatibility == CompatibilityStrict {
		if observedDigest == "" {
			requiredOverride := CompatibilityAllowProtocolDigestUnavailable
			return nil, "", false, newCompatibilityError(
				"initialize response missing selected protocol digest",
				env,
				cfg.ProtocolMode,
				runtimePath,
				&requiredOverride,
			)
		}
		if observedDigest != expectedDigest {
			requiredOverride := CompatibilityAllowDevBuild
			return nil, "", false, newCompatibilityError(
				"initialize response protocol digest mismatch",
				env,
				cfg.ProtocolMode,
				runtimePath,
				&requiredOverride,
			)
		}
		if !modeMatches {
			return nil, "", false, newCompatibilityError(
				"initialize response activeProtocolMode mismatch",
				env,
				cfg.ProtocolMode,
				runtimePath,
				nil,
			)
		}
		current, err := decodeCurrentInitializeResponse(env)
		if err != nil {
			return nil, "", false, err
		}
		return current, "", false, nil
	}
	if observedDigest == expectedDigest && modeMatches {
		current, err := decodeCurrentInitializeResponse(env)
		if err != nil {
			return nil, "", false, err
		}
		return current, "", false, nil
	}

	devSource := source == runtimeSourceInjected || source == runtimeSourceExplicitPath
	devIdentity := looksLikeDevBuild(env.UserAgent)
	if !devSource || !devIdentity {
		return nil, "", false, newCompatibilityError(
			"compatibility override requires injected or explicit dev runtime",
			env,
			cfg.ProtocolMode,
			runtimePath,
			nil,
		)
	}
	if env.ActiveProtocolMode != nil && *env.ActiveProtocolMode != expectedMode {
		return nil, "", false, newCompatibilityError(
			"initialize response activeProtocolMode mismatch",
			env,
			cfg.ProtocolMode,
			runtimePath,
			nil,
		)
	}
	if cfg.Compatibility == CompatibilityAllowProtocolDigestUnavailable {
		if hasNonEmptyProtocolDigest(env) {
			requiredOverride := CompatibilityAllowDevBuild
			return nil, "", false, newCompatibilityError(
				"non-empty protocol digest requires current initialize validation",
				env,
				cfg.ProtocolMode,
				runtimePath,
				&requiredOverride,
			)
		}
		return nil, "CompatibilityAllowProtocolDigestUnavailable accepted legacy dev/test initialize response with missing digest or mode", true, nil
	}
	return nil, "CompatibilityAllowDevBuild accepted explicit dev runtime digest mismatch or missing digest", true, nil
}

func newCompatibilityError(
	reason string,
	env initializeCompatibilityEnvelope,
	expectedMode ProtocolMode,
	runtimePath string,
	requiredOverride *CompatibilityPolicy,
) *CompatibilityError {
	expectedDigest, _ := expectedProtocol(expectedMode)
	return &CompatibilityError{
		Reason:           reason,
		ExpectedDigest:   expectedDigest,
		FoundDigest:      observedProtocolDigest(env, expectedMode),
		ExpectedMode:     expectedMode,
		FoundMode:        publicProtocolMode(env.ActiveProtocolMode),
		RuntimePath:      runtimePath,
		RuntimeVersion:   runtimeVersionFromUserAgent(env.UserAgent),
		UserAgent:        env.UserAgent,
		RequiredOverride: requiredOverride,
	}
}

func publicProtocolMode(mode *protocol.ActiveProtocolMode) *ProtocolMode {
	if mode == nil {
		return nil
	}
	publicMode := ProtocolModeExperimental
	if *mode == protocol.ActiveProtocolModeStable {
		publicMode = ProtocolModeStable
	}
	return &publicMode
}

func hasNonEmptyProtocolDigest(env initializeCompatibilityEnvelope) bool {
	return env.StableProtocolDigest != nil && *env.StableProtocolDigest != "" ||
		env.ExperimentalProtocolDigest != nil && *env.ExperimentalProtocolDigest != ""
}

func decodeCurrentInitializeResponse(env initializeCompatibilityEnvelope) (*protocol.InitializeResponse, error) {
	var current protocol.InitializeResponse
	if err := json.Unmarshal(env.Raw, &current); err != nil {
		return nil, err
	}
	return &current, nil
}

func expectedProtocol(mode ProtocolMode) (string, protocol.ActiveProtocolMode) {
	if mode == ProtocolModeStable {
		return protocol.StableProtocolDigest, protocol.ActiveProtocolModeStable
	}
	return protocol.ExperimentalProtocolDigest, protocol.ActiveProtocolModeExperimental
}

func observedProtocolDigest(env initializeCompatibilityEnvelope, mode ProtocolMode) string {
	if mode == ProtocolModeStable {
		if env.StableProtocolDigest == nil {
			return ""
		}
		return *env.StableProtocolDigest
	}
	if env.ExperimentalProtocolDigest == nil {
		return ""
	}
	return *env.ExperimentalProtocolDigest
}

func looksLikeDevBuild(userAgent string) bool {
	lower := strings.ToLower(userAgent)
	return strings.Contains(lower, "dev") || strings.Contains(lower, "0.0.0")
}

func runtimeVersionFromUserAgent(userAgent string) string {
	parts := strings.Fields(userAgent)
	if len(parts) == 0 {
		return ""
	}
	return parts[len(parts)-1]
}

func (c *Client) validateMethodCall(metadata protocol.MethodMetadata, params any) error {
	stableMode := c.metadata.ProtocolMode == ProtocolModeStable
	if stableMode && metadata.Experimental {
		return &ConfigError{Reason: "experimental method is disabled in stable protocol mode: " + metadata.Method}
	}
	if params == nil {
		return nil
	}
	needsExperimentalInspection := stableMode && len(metadata.ExperimentalFields) > 0
	needsBoundedInspection := len(metadata.BoundedModelContextFields) > 0
	if !needsExperimentalInspection && !needsBoundedInspection {
		return nil
	}
	data, err := json.Marshal(params)
	if err != nil {
		return err
	}
	var rawObject map[string]json.RawMessage
	if err := json.Unmarshal(data, &rawObject); err != nil {
		return err
	}
	if needsExperimentalInspection {
		var object map[string]any
		if err := json.Unmarshal(data, &object); err != nil {
			return err
		}
		for _, field := range metadata.ExperimentalFields {
			if !field.InspectParams {
				continue
			}
			present, err := experimentalFieldPresent(object, field)
			if err != nil {
				return err
			}
			if present {
				return &ConfigError{Reason: "experimental field is disabled in stable protocol mode: " + field.FieldPath}
			}
		}
	}
	if err := c.validateBoundedModelContextFields(metadata, rawObject); err != nil {
		return err
	}
	return nil
}

func (c *Client) validateBoundedModelContextFields(metadata protocol.MethodMetadata, object map[string]json.RawMessage) error {
	for _, field := range metadata.BoundedModelContextFields {
		if field.FieldPath != "additional_context.*.value" {
			return &ConfigError{Reason: "unsupported bounded model context field: " + field.FieldPath}
		}
		if field.LimitProfile != "additionalContextValueBytes" {
			return &ConfigError{Reason: "unsupported bounded model context limit profile: " + field.LimitProfile}
		}
		raw, ok := rawJSONField(object, "additional_context")
		if !ok {
			continue
		}
		if err := c.validateAdditionalContext(raw); err != nil {
			return err
		}
	}
	return nil
}

func rawJSONField(object map[string]json.RawMessage, fieldPath string) (json.RawMessage, bool) {
	if value, ok := object[fieldPath]; ok {
		return value, true
	}
	value, ok := object[snakeToCamel(fieldPath)]
	return value, ok
}

func (c *Client) validateAdditionalContext(raw json.RawMessage) error {
	if len(bytes.TrimSpace(raw)) == 0 || bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
		return nil
	}
	var entries map[string]json.RawMessage
	if err := json.Unmarshal(raw, &entries); err != nil {
		return &ConfigError{Reason: "additionalContext must be an object"}
	}
	if len(entries) > c.limits.MaxAdditionalContextEntries {
		return &ConfigError{Reason: "additionalContext entry count exceeds configured limit"}
	}
	var totalBytes int64
	for key, rawEntry := range entries {
		keyBytes := int64(len([]byte(key)))
		if keyBytes > c.limits.MaxAdditionalContextKeyBytes {
			return &ConfigError{Reason: "additionalContext key exceeds configured byte limit"}
		}
		var rawEntryObject map[string]json.RawMessage
		if err := json.Unmarshal(rawEntry, &rawEntryObject); err != nil {
			return &ConfigError{Reason: "additionalContext entry must be an object with a string value"}
		}
		rawValue, ok := rawEntryObject["value"]
		if !ok || bytes.Equal(bytes.TrimSpace(rawValue), []byte("null")) {
			return &ConfigError{Reason: "additionalContext entry must include a string value"}
		}
		var entry struct {
			Value string `json:"value"`
		}
		if err := json.Unmarshal(rawEntry, &entry); err != nil {
			return &ConfigError{Reason: "additionalContext entry must be an object with a string value"}
		}
		valueBytes := int64(len([]byte(entry.Value)))
		if valueBytes > c.limits.MaxAdditionalContextValueBytes {
			return &ConfigError{Reason: "additionalContext value exceeds configured byte limit"}
		}
		totalBytes += keyBytes + valueBytes
		if totalBytes > c.limits.MaxAdditionalContextTotalBytes {
			return &ConfigError{Reason: "additionalContext total size exceeds configured byte limit"}
		}
	}
	return nil
}

func experimentalFieldPresent(object map[string]any, field protocol.ExperimentalFieldMetadata) (bool, error) {
	if field.DiscriminatorJSON == "" {
		return jsonPathPresent(object, field.FieldPath), nil
	}
	var discriminator struct {
		FieldPath string `json:"fieldPath"`
		WireValue string `json:"wireValue"`
	}
	if err := json.Unmarshal([]byte(field.DiscriminatorJSON), &discriminator); err != nil {
		return false, err
	}
	fieldPath := discriminator.FieldPath
	if fieldPath == "" {
		fieldPath = field.FieldPath
	}
	value, ok := jsonPathValue(object, fieldPath)
	if !ok {
		return false, nil
	}
	valueString, ok := value.(string)
	return ok && valueString == discriminator.WireValue, nil
}

func jsonPathPresent(object map[string]any, fieldPath string) bool {
	_, ok := jsonPathValue(object, fieldPath)
	return ok
}

func jsonPathValue(object map[string]any, fieldPath string) (any, bool) {
	segments := strings.Split(fieldPath, ".")
	var current any = object
	for _, segment := range segments {
		if segment == "*" {
			continue
		}
		m, ok := current.(map[string]any)
		if !ok {
			return nil, false
		}
		if value, ok := m[segment]; ok {
			current = value
			continue
		}
		camel := snakeToCamel(segment)
		value, ok := m[camel]
		if !ok {
			return nil, false
		}
		current = value
	}
	return current, true
}

func snakeToCamel(value string) string {
	parts := strings.Split(value, "_")
	for i := 1; i < len(parts); i++ {
		if parts[i] == "" {
			continue
		}
		parts[i] = strings.ToUpper(parts[i][:1]) + parts[i][1:]
	}
	return strings.Join(parts, "")
}
