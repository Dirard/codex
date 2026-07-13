package protocodex

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
)

const (
	dynamicToolSpecsDeserializeHook = "codex_protocol::dynamic_tools::deserialize_dynamic_tool_specs"
	doubleOptionDeserializeHook     = "crate::protocol::serde_helpers::deserialize_double_option"
	doubleOptionSerializeHook       = "crate::protocol::serde_helpers::serialize_double_option"
	supportedManifestSchemaVersion  = 1
	skipSerializingIfNotNot         = "std::ops::Not::not"
	skipSerializingIfOptionNone     = "Option::is_none"
	skipSerializingIfVecEmpty       = "Vec::is_empty"
)

type Manifest struct {
	ManifestSchemaVersion int                `json:"manifestSchemaVersion"`
	Stable                ManifestMode       `json:"stable"`
	Experimental          ManifestMode       `json:"experimental"`
	ModelContextLimits    ModelContextLimits `json:"modelContextLimits"`
}

type ManifestMode struct {
	ProtocolMode        string               `json:"protocolMode"`
	ClientRequests      []ClientRequestEntry `json:"clientRequests"`
	ServerRequests      []ServerRequestEntry `json:"serverRequests"`
	ServerNotifications []NotificationEntry  `json:"serverNotifications"`
	ClientNotifications []NotificationEntry  `json:"clientNotifications"`
	RoutingLifecycle    []RoutingLifecycle   `json:"routingLifecycle"`
	SerdeShapes         []SerdeShape         `json:"serdeShapes"`
	Digests             map[string]string    `json:"digests"`
}

type ClientRequestEntry struct {
	Variant                    string                      `json:"variant"`
	Direction                  string                      `json:"direction"`
	Exception                  json.RawMessage             `json:"exception"`
	Method                     string                      `json:"method"`
	ParamsType                 string                      `json:"paramsType"`
	ParamsSchemaRef            string                      `json:"paramsSchemaRef"`
	ResponseType               string                      `json:"responseType"`
	ResponseSchemaRef          string                      `json:"responseSchemaRef"`
	SDKVisibility              string                      `json:"sdkVisibility"`
	SchemaExcludedReason       string                      `json:"schemaExcludedReason"`
	SerdeShapeRequirement      string                      `json:"serdeShapeRequirement"`
	Retry                      string                      `json:"retry"`
	Experimental               json.RawMessage             `json:"experimental"`
	ExperimentalFields         []ExperimentalField         `json:"experimentalFields"`
	BoundedModelContextFields  []BoundedModelContextField  `json:"boundedModelContextFields"`
	InspectParams              bool                        `json:"inspectParams"`
	ManualPayloadConversion    *string                     `json:"manualPayloadConversion"`
	RequestSerializationScopes []RequestSerializationScope `json:"requestSerializationScopes"`
}

type ServerRequestEntry struct {
	Variant                    string                      `json:"variant"`
	Direction                  string                      `json:"direction"`
	Exception                  json.RawMessage             `json:"exception"`
	Method                     string                      `json:"method"`
	PayloadType                string                      `json:"paramsType"`
	ParamsSchemaRef            string                      `json:"paramsSchemaRef"`
	ResponseType               string                      `json:"responseType"`
	ResponseSchemaRef          string                      `json:"responseSchemaRef"`
	SDKVisibility              string                      `json:"sdkVisibility"`
	SchemaExcludedReason       string                      `json:"schemaExcludedReason"`
	SerdeShapeRequirement      string                      `json:"serdeShapeRequirement"`
	Experimental               json.RawMessage             `json:"experimental"`
	ExperimentalFields         []ExperimentalField         `json:"experimentalFields"`
	BoundedModelContextFields  []BoundedModelContextField  `json:"boundedModelContextFields"`
	InspectParams              bool                        `json:"inspectParams"`
	Retry                      string                      `json:"retry"`
	ManualPayloadConversion    *string                     `json:"manualPayloadConversion"`
	RequestSerializationScopes []RequestSerializationScope `json:"requestSerializationScopes"`
}

type NotificationEntry struct {
	Variant                    string                      `json:"variant"`
	Direction                  string                      `json:"direction"`
	Exception                  json.RawMessage             `json:"exception"`
	Method                     string                      `json:"method"`
	PayloadType                string                      `json:"payloadType"`
	PayloadSchemaRef           string                      `json:"payloadSchemaRef"`
	SDKVisibility              string                      `json:"sdkVisibility"`
	SchemaExcludedReason       string                      `json:"schemaExcludedReason"`
	SerdeShapeRequirement      string                      `json:"serdeShapeRequirement"`
	Experimental               json.RawMessage             `json:"experimental"`
	ExperimentalFields         []ExperimentalField         `json:"experimentalFields"`
	RoutingStrategy            RoutingStrategy             `json:"routingStrategy"`
	ManualPayloadConversion    *string                     `json:"manualPayloadConversion"`
	RequestSerializationScopes []RequestSerializationScope `json:"requestSerializationScopes"`
}

type RequestSerializationScope struct {
	Kind               string              `json:"kind"`
	Condition          json.RawMessage     `json:"condition"`
	IdentityExtractors []IdentityExtractor `json:"identityExtractors"`
	Precedence         int                 `json:"precedence"`
	QueueKey           json.RawMessage     `json:"queueKey"`
}

type ExperimentalField struct {
	ContainingType string          `json:"containingType"`
	Discriminator  json.RawMessage `json:"discriminator"`
	FieldPath      string          `json:"fieldPath"`
	InspectParams  bool            `json:"inspectParams"`
	Reason         string          `json:"reason"`
}

type BoundedModelContextField struct {
	Method       string `json:"method"`
	FieldPath    string `json:"fieldPath"`
	LimitProfile string `json:"limitProfile"`
}

type ModelContextLimits struct {
	MaxAdditionalContextEntries    int `json:"maxAdditionalContextEntries"`
	MaxAdditionalContextKeyBytes   int `json:"maxAdditionalContextKeyBytes"`
	MaxAdditionalContextValueBytes int `json:"maxAdditionalContextValueBytes"`
	MaxAdditionalContextTotalBytes int `json:"maxAdditionalContextTotalBytes"`
}

type RoutingLifecycle struct {
	CleanupTriggers                []LifecycleTrigger `json:"cleanupTriggers"`
	NotificationOptOutDependencies []string           `json:"notificationOptOutDependencies"`
	ResourceDomain                 string             `json:"resourceDomain"`
	StartCompletion                LifecycleTrigger   `json:"startCompletion"`
	StartMethod                    string             `json:"startMethod"`
	WireIdentitySource             string             `json:"wireIdentitySource"`
}

type LifecycleTrigger struct {
	Kind      string `json:"kind"`
	Method    string `json:"method"`
	Predicate string `json:"predicate"`
}

type RoutingStrategy struct {
	Kind                  string         `json:"kind"`
	Routes                []RoutingRoute `json:"routes"`
	MissingIdentityReason string         `json:"missingIdentityReason"`
	Reason                string         `json:"reason"`
}

type RoutingRoute struct {
	ResourceDomain     string              `json:"resourceDomain"`
	WireIdentitySource string              `json:"wireIdentitySource"`
	IdentityExtractors []IdentityExtractor `json:"identityExtractors"`
}

type IdentityExtractor struct {
	FieldPath         string          `json:"fieldPath"`
	IdentityName      string          `json:"identityName"`
	Optional          bool            `json:"optional"`
	TerminalPredicate json.RawMessage `json:"terminalPredicate"`
}

type SerdeShape struct {
	RustType                string                 `json:"rustType"`
	MetadataStatus          string                 `json:"metadataStatus"`
	SchemaRef               string                 `json:"schemaRef"`
	SchemaSufficientProof   *SchemaSufficientProof `json:"schemaSufficientProof"`
	Fields                  []SerdeField           `json:"fields"`
	VariantAliases          []SerdeVariantAlias    `json:"variantAliases"`
	ManualPayloadConversion *string                `json:"manualPayloadConversion"`
	ReviewNote              *string                `json:"reviewNote"`
}

type SchemaSufficientProof struct {
	CheckedAdditionalProperties bool   `json:"checkedAdditionalProperties"`
	CheckedEnumValues           bool   `json:"checkedEnumValues"`
	CheckedNullableFields       bool   `json:"checkedNullableFields"`
	CheckedRequiredFields       bool   `json:"checkedRequiredFields"`
	CheckedUnionTags            bool   `json:"checkedUnionTags"`
	NoCustomSerde               bool   `json:"noCustomSerde"`
	NoFlatten                   bool   `json:"noFlatten"`
	NoSerdeAliases              bool   `json:"noSerdeAliases"`
	NoSerdeDefaults             bool   `json:"noSerdeDefaults"`
	NoSkipSerializingIf         bool   `json:"noSkipSerializingIf"`
	SourceAnchor                string `json:"sourceAnchor"`
}

type SerdeField struct {
	WireName  string          `json:"wireName"`
	RustField string          `json:"rustField"`
	Aliases   []string        `json:"aliases"`
	Shape     SerdeFieldShape `json:"shape"`
}

type SerdeFieldShape struct {
	Presence          string        `json:"presence"`
	Flattened         bool          `json:"flattened"`
	SkipSerializingIf string        `json:"skipSerializingIf"`
	CustomSerialize   string        `json:"customSerialize"`
	CustomDeserialize string        `json:"customDeserialize"`
	Default           *SerdeDefault `json:"default"`
}

type SerdeDefault struct {
	Provider      json.RawMessage `json:"provider"`
	WireValueJSON string          `json:"wireValueJson"`
}

type SerdeVariantAlias struct {
	RustVariant        string   `json:"rustVariant"`
	CanonicalWireValue string   `json:"canonicalWireValue"`
	Aliases            []string `json:"aliases"`
}

func LoadManifest(path string) (*Manifest, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var manifest Manifest
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&manifest); err != nil {
		return nil, err
	}
	if err := ensureJSONEOF(decoder); err != nil {
		return nil, err
	}
	if manifest.ManifestSchemaVersion != supportedManifestSchemaVersion {
		return nil, fmt.Errorf(
			"unsupported manifestSchemaVersion %d: want %d",
			manifest.ManifestSchemaVersion,
			supportedManifestSchemaVersion,
		)
	}
	if err := validateManifestV1Metadata("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateManifestV1Metadata("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateManifestDirections("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateManifestDirections("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateRoutingLifecycle("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateRoutingLifecycle("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateRequestSerializationScopes("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateRequestSerializationScopes("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateSerdeShapeProofs("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateSerdeShapeProofs("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateCustomSerdeHooks("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateCustomSerdeHooks("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateManifestV1Semantics("stable", "stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateManifestV1Semantics("experimental", "experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	if err := validateModelContextLimits(manifest.ModelContextLimits); err != nil {
		return nil, err
	}
	if err := validateManifestDigests("stable", manifest.Stable); err != nil {
		return nil, err
	}
	if err := validateManifestDigests("experimental", manifest.Experimental); err != nil {
		return nil, err
	}
	return &manifest, nil
}

func ensureJSONEOF(decoder *json.Decoder) error {
	var extra any
	err := decoder.Decode(&extra)
	if err == io.EOF {
		return nil
	}
	if err == nil {
		return fmt.Errorf("manifest contains a trailing JSON value")
	}
	return fmt.Errorf("read trailing manifest JSON: %w", err)
}

func validateManifestV1Metadata(modeName string, mode ManifestMode) error {
	for _, entry := range mode.ClientRequests {
		if entry.Variant == "" {
			return fmt.Errorf("%s client request %q has empty variant", modeName, entry.Method)
		}
	}
	for _, entry := range mode.ServerRequests {
		if entry.Variant == "" {
			return fmt.Errorf("%s server request %q has empty variant", modeName, entry.Method)
		}
	}
	for _, entries := range [][]NotificationEntry{mode.ServerNotifications, mode.ClientNotifications} {
		for _, entry := range entries {
			if entry.Variant == "" {
				return fmt.Errorf("%s notification %q has empty variant", modeName, entry.Method)
			}
			switch entry.RoutingStrategy.Kind {
			case "routed":
				if len(entry.RoutingStrategy.Routes) == 0 {
					return fmt.Errorf("%s notification %q has no routes", modeName, entry.Method)
				}
			case "routedWithGlobalFallback":
				if len(entry.RoutingStrategy.Routes) == 0 || entry.RoutingStrategy.MissingIdentityReason == "" {
					return fmt.Errorf("%s notification %q has incomplete routed fallback metadata", modeName, entry.Method)
				}
			case "globalOnly", "internalOnly", "rawOnly":
				if entry.RoutingStrategy.Reason == "" {
					return fmt.Errorf("%s notification %q has empty routing reason", modeName, entry.Method)
				}
			default:
				return fmt.Errorf("%s notification %q has unknown routing kind %q", modeName, entry.Method, entry.RoutingStrategy.Kind)
			}
		}
	}
	return nil
}

func validateManifestDirections(modeName string, mode ManifestMode) error {
	for _, entry := range mode.ClientRequests {
		if entry.Direction != "clientToServer" {
			return fmt.Errorf("%s client request %q has direction %q", modeName, entry.Method, entry.Direction)
		}
	}
	for _, entry := range mode.ServerRequests {
		if entry.Direction != "serverToClient" {
			return fmt.Errorf("%s server request %q has direction %q", modeName, entry.Method, entry.Direction)
		}
	}
	for _, entry := range mode.ServerNotifications {
		if entry.Direction != "serverNotification" {
			return fmt.Errorf("%s server notification %q has direction %q", modeName, entry.Method, entry.Direction)
		}
	}
	for _, entry := range mode.ClientNotifications {
		if entry.Direction != "clientNotification" {
			return fmt.Errorf("%s client notification %q has direction %q", modeName, entry.Method, entry.Direction)
		}
	}
	return nil
}

var requiredDigestKeys = []string{"protocolDigest", "schemaDigest", "manifestDigest"}

func validateManifestDigests(modeName string, mode ManifestMode) error {
	if len(mode.Digests) == 0 {
		return fmt.Errorf("%s manifest digests are missing", modeName)
	}
	for _, key := range requiredDigestKeys {
		value, ok := mode.Digests[key]
		if !ok {
			return fmt.Errorf("%s manifest digest %s is missing", modeName, key)
		}
		if !isSHA256Hex(value) {
			return fmt.Errorf("%s manifest digest %s must be a SHA-256 hex digest", modeName, key)
		}
	}
	if len(mode.Digests) != len(requiredDigestKeys) {
		return fmt.Errorf("%s manifest digests contain unsupported fields", modeName)
	}
	return nil
}

func isSHA256Hex(value string) bool {
	if len(value) != 64 {
		return false
	}
	for _, ch := range value {
		if (ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f') {
			continue
		}
		return false
	}
	return true
}

func validateRoutingLifecycle(modeName string, mode ManifestMode) error {
	seen := map[string]bool{}
	for _, entry := range mode.RoutingLifecycle {
		if entry.ResourceDomain == "" {
			return fmt.Errorf("%s routingLifecycle entry for %q has empty resourceDomain", modeName, entry.StartMethod)
		}
		if entry.StartMethod == "" {
			return fmt.Errorf("%s routingLifecycle entry for %q has empty startMethod", modeName, entry.ResourceDomain)
		}
		if seen[entry.StartMethod] {
			return fmt.Errorf("%s routingLifecycle has duplicate startMethod %q", modeName, entry.StartMethod)
		}
		seen[entry.StartMethod] = true
		if entry.WireIdentitySource == "" {
			return fmt.Errorf("%s routingLifecycle %q has empty wireIdentitySource", modeName, entry.StartMethod)
		}
		if err := validateLifecycleTrigger(modeName, entry.StartMethod, "startCompletion", entry.StartCompletion); err != nil {
			return err
		}
		if len(entry.CleanupTriggers) == 0 {
			return fmt.Errorf("%s routingLifecycle %q has no cleanupTriggers", modeName, entry.StartMethod)
		}
		for index, trigger := range entry.CleanupTriggers {
			if err := validateLifecycleTrigger(modeName, entry.StartMethod, fmt.Sprintf("cleanupTriggers[%d]", index), trigger); err != nil {
				return err
			}
		}
		for _, method := range entry.NotificationOptOutDependencies {
			if method == "" {
				return fmt.Errorf("%s routingLifecycle %q has empty notificationOptOutDependencies entry", modeName, entry.StartMethod)
			}
		}
	}
	return nil
}

func validateLifecycleTrigger(modeName, startMethod, label string, trigger LifecycleTrigger) error {
	if trigger.Kind == "" {
		return fmt.Errorf("%s routingLifecycle %q %s has empty kind", modeName, startMethod, label)
	}
	if !knownLifecycleTriggerKind(trigger.Kind) {
		return fmt.Errorf("%s routingLifecycle %q %s has unknown kind %q", modeName, startMethod, label, trigger.Kind)
	}
	if trigger.Method == "" {
		return fmt.Errorf("%s routingLifecycle %q %s has empty method", modeName, startMethod, label)
	}
	return nil
}

func knownLifecycleTriggerKind(kind string) bool {
	switch kind {
	case "jsonRpcResponse", "terminalNotification", "explicitMethodResponse":
		return true
	default:
		return false
	}
}

func validateRequestSerializationScopes(modeName string, mode ManifestMode) error {
	for _, entry := range mode.ClientRequests {
		if err := validateClientRequestSerializationScopes(modeName, entry); err != nil {
			return err
		}
	}
	for _, entry := range mode.ServerRequests {
		if len(entry.RequestSerializationScopes) > 0 {
			return fmt.Errorf("%s server request %q must not declare request serialization scopes", modeName, entry.Method)
		}
	}
	for _, entry := range mode.ServerNotifications {
		if len(entry.RequestSerializationScopes) > 0 {
			return fmt.Errorf("%s server notification %q must not declare request serialization scopes", modeName, entry.Method)
		}
	}
	for _, entry := range mode.ClientNotifications {
		if len(entry.RequestSerializationScopes) > 0 {
			return fmt.Errorf("%s client notification %q must not declare request serialization scopes", modeName, entry.Method)
		}
	}
	return nil
}

func validateSerdeShapeProofs(modeName string, mode ManifestMode) error {
	for _, shape := range mode.SerdeShapes {
		switch shape.MetadataStatus {
		case "schemaSufficient":
			if shape.SchemaSufficientProof == nil {
				return fmt.Errorf("%s serde shape %s is schemaSufficient without schemaSufficientProof", modeName, shape.RustType)
			}
			if !shape.SchemaSufficientProof.Complete() {
				return fmt.Errorf("%s serde shape %s has incomplete schemaSufficientProof", modeName, shape.RustType)
			}
		case "manifestRequired":
		default:
			return fmt.Errorf("%s serde shape %s has unknown metadataStatus %q", modeName, shape.RustType, shape.MetadataStatus)
		}
	}
	return nil
}

func (p SchemaSufficientProof) Complete() bool {
	return p.CheckedAdditionalProperties &&
		p.CheckedEnumValues &&
		p.CheckedNullableFields &&
		p.CheckedRequiredFields &&
		p.CheckedUnionTags &&
		p.NoCustomSerde &&
		p.NoFlatten &&
		p.NoSerdeAliases &&
		p.NoSerdeDefaults &&
		p.NoSkipSerializingIf &&
		p.SourceAnchor != ""
}

func validateCustomSerdeHooks(modeName string, mode ManifestMode) error {
	for _, shape := range mode.SerdeShapes {
		for _, field := range shape.Fields {
			if hook := field.Shape.CustomDeserialize; hook != "" && !knownCustomDeserializeHook(hook) {
				return fmt.Errorf("%s serde shape %s field %s has unsupported customDeserialize hook %q", modeName, shape.RustType, field.WireName, hook)
			}
			if hook := field.Shape.CustomSerialize; hook != "" && !knownCustomSerializeHook(hook) {
				return fmt.Errorf("%s serde shape %s field %s has unsupported customSerialize hook %q", modeName, shape.RustType, field.WireName, hook)
			}
			if predicate := field.Shape.SkipSerializingIf; predicate != "" && !knownSkipSerializingIfPredicate(predicate) {
				return fmt.Errorf("%s serde shape %s field %s has unsupported skipSerializingIf predicate %q", modeName, shape.RustType, field.WireName, predicate)
			}
		}
	}
	return nil
}

func knownCustomDeserializeHook(hook string) bool {
	switch hook {
	case dynamicToolSpecsDeserializeHook, doubleOptionDeserializeHook:
		return true
	default:
		return false
	}
}

func knownCustomSerializeHook(hook string) bool {
	switch hook {
	case doubleOptionSerializeHook:
		return true
	default:
		return false
	}
}

func knownSkipSerializingIfPredicate(predicate string) bool {
	switch predicate {
	case skipSerializingIfNotNot, skipSerializingIfOptionNone, skipSerializingIfVecEmpty:
		return true
	default:
		return false
	}
}

func validateClientRequestSerializationScopes(modeName string, entry ClientRequestEntry) error {
	var seenConditions []parsedRequestSerializationCondition
	for _, scope := range entry.RequestSerializationScopes {
		if !knownRequestSerializationScope(scope.Kind) {
			return fmt.Errorf("%s client request %q has unknown request serialization scope %q", modeName, entry.Method, scope.Kind)
		}
		condition, err := parseRequestSerializationCondition(scope.Condition)
		if err != nil {
			return fmt.Errorf("%s client request %q scope %q has %w", modeName, entry.Method, scope.Kind, err)
		}
		for _, seen := range seenConditions {
			if requestSerializationConditionsOverlap(seen, condition) {
				return fmt.Errorf("%s client request %q has overlapping request serialization scope condition %s", modeName, entry.Method, condition.Label)
			}
		}
		seenConditions = append(seenConditions, condition)
		if scopeRequiresIdentity(scope.Kind) && len(scope.IdentityExtractors) == 0 {
			return fmt.Errorf("%s client request %q scope %q has no identity extractors", modeName, entry.Method, scope.Kind)
		}
		for _, extractor := range scope.IdentityExtractors {
			if extractor.FieldPath == "" || extractor.IdentityName == "" {
				return fmt.Errorf("%s client request %q scope %q has incomplete identity extractor", modeName, entry.Method, scope.Kind)
			}
		}
	}
	return nil
}

type requestSerializationFieldState string

const (
	requestSerializationFieldPresent   requestSerializationFieldState = "present"
	requestSerializationFieldAbsent    requestSerializationFieldState = "absent"
	requestSerializationStringEmpty    requestSerializationFieldState = "stringEmpty"
	requestSerializationStringNonEmpty requestSerializationFieldState = "stringNonEmpty"
)

type parsedRequestSerializationCondition struct {
	Label  string
	Fields map[string]requestSerializationFieldState
}

func parseRequestSerializationCondition(raw json.RawMessage) (parsedRequestSerializationCondition, error) {
	label := "always"
	if len(raw) > 0 {
		label = string(raw)
	}
	if rawJSONEmptyOrNull(raw) {
		return parsedRequestSerializationCondition{Label: label, Fields: map[string]requestSerializationFieldState{}}, nil
	}
	var keyword string
	if err := json.Unmarshal(raw, &keyword); err == nil {
		if keyword != "always" {
			return parsedRequestSerializationCondition{}, fmt.Errorf("unknown request serialization condition %q", keyword)
		}
		return parsedRequestSerializationCondition{Label: label, Fields: map[string]requestSerializationFieldState{}}, nil
	}
	condition, err := parseRequestSerializationConditionObject(raw)
	if err != nil {
		return parsedRequestSerializationCondition{}, err
	}
	condition.Label = label
	return condition, nil
}

func parseRequestSerializationConditionObject(raw json.RawMessage) (parsedRequestSerializationCondition, error) {
	var object map[string]json.RawMessage
	if err := json.Unmarshal(raw, &object); err != nil {
		return parsedRequestSerializationCondition{}, fmt.Errorf("invalid request serialization condition: %w", err)
	}
	if len(object) != 1 {
		return parsedRequestSerializationCondition{}, fmt.Errorf("request serialization condition must have exactly one discriminator")
	}
	for discriminator, value := range object {
		switch discriminator {
		case "fieldPresent":
			return parseRequestSerializationFieldCondition(value, requestSerializationFieldPresent)
		case "fieldAbsent":
			return parseRequestSerializationFieldCondition(value, requestSerializationFieldAbsent)
		case "stringEmpty":
			return parseRequestSerializationFieldCondition(value, requestSerializationStringEmpty)
		case "stringNonEmpty":
			return parseRequestSerializationFieldCondition(value, requestSerializationStringNonEmpty)
		case "all":
			return parseRequestSerializationAllCondition(value)
		default:
			return parsedRequestSerializationCondition{}, fmt.Errorf("unknown request serialization condition %q", discriminator)
		}
	}
	return parsedRequestSerializationCondition{}, fmt.Errorf("empty request serialization condition")
}

func parseRequestSerializationFieldCondition(raw json.RawMessage, state requestSerializationFieldState) (parsedRequestSerializationCondition, error) {
	var field string
	if err := json.Unmarshal(raw, &field); err != nil {
		return parsedRequestSerializationCondition{}, fmt.Errorf("request serialization condition field must be a string: %w", err)
	}
	if field == "" {
		return parsedRequestSerializationCondition{}, fmt.Errorf("request serialization condition field must not be empty")
	}
	return parsedRequestSerializationCondition{Fields: map[string]requestSerializationFieldState{field: state}}, nil
}

func parseRequestSerializationAllCondition(raw json.RawMessage) (parsedRequestSerializationCondition, error) {
	var branches []json.RawMessage
	if err := json.Unmarshal(raw, &branches); err != nil {
		return parsedRequestSerializationCondition{}, fmt.Errorf("request serialization all condition must be a list: %w", err)
	}
	if len(branches) == 0 {
		return parsedRequestSerializationCondition{}, fmt.Errorf("request serialization all condition must not be empty")
	}
	condition := parsedRequestSerializationCondition{Fields: map[string]requestSerializationFieldState{}}
	for _, branch := range branches {
		parsed, err := parseRequestSerializationCondition(branch)
		if err != nil {
			return parsedRequestSerializationCondition{}, err
		}
		if err := mergeRequestSerializationConditionFields(condition.Fields, parsed.Fields); err != nil {
			return parsedRequestSerializationCondition{}, err
		}
	}
	return condition, nil
}

func mergeRequestSerializationConditionFields(target map[string]requestSerializationFieldState, source map[string]requestSerializationFieldState) error {
	for field, sourceState := range source {
		targetState, ok := target[field]
		if !ok {
			target[field] = sourceState
			continue
		}
		merged, ok := mergeRequestSerializationFieldState(targetState, sourceState)
		if !ok {
			return fmt.Errorf("contradictory request serialization condition for field %q", field)
		}
		target[field] = merged
	}
	return nil
}

func requestSerializationConditionsOverlap(a, b parsedRequestSerializationCondition) bool {
	for field, aState := range a.Fields {
		if bState, ok := b.Fields[field]; ok && requestSerializationFieldStatesConflict(aState, bState) {
			return false
		}
	}
	return true
}

func requestSerializationFieldStatesConflict(a, b requestSerializationFieldState) bool {
	_, ok := mergeRequestSerializationFieldState(a, b)
	return !ok
}

func mergeRequestSerializationFieldState(a, b requestSerializationFieldState) (requestSerializationFieldState, bool) {
	if a == b {
		return a, true
	}
	if a == requestSerializationFieldPresent {
		if b == requestSerializationFieldAbsent {
			return "", false
		}
		return b, true
	}
	if b == requestSerializationFieldPresent {
		if a == requestSerializationFieldAbsent {
			return "", false
		}
		return a, true
	}
	if a == requestSerializationFieldAbsent || b == requestSerializationFieldAbsent {
		return "", false
	}
	return "", false
}

func knownRequestSerializationScope(kind string) bool {
	switch kind {
	case "global",
		"globalSharedRead",
		"thread",
		"threadPath",
		"commandExecProcess",
		"process",
		"fuzzyFileSearchSession",
		"fsWatch",
		"mcpOauth":
		return true
	default:
		return false
	}
}

func scopeRequiresIdentity(kind string) bool {
	return kind != "global" && kind != "globalSharedRead"
}
