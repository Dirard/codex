package protocodex

import (
	"encoding/json"
	"fmt"
	"strings"
)

func renderRawClient(manifest *Manifest) string {
	var b strings.Builder
	b.WriteString("package protocol\n\nimport \"context\"\n\n")
	b.WriteString("type Sender interface {\n\tCall(ctx context.Context, method string, params any, result any, metadata MethodMetadata) error\n}\n\n")
	b.WriteString("type RawClient struct { sender Sender }\n\nfunc NewRawClient(sender Sender) RawClient { return RawClient{sender: sender} }\n\n")
	for _, entry := range manifest.Experimental.ClientRequests {
		if entry.SDKVisibility != "public" || entry.Method == "initialize" {
			continue
		}
		methodName := RawMethodName(entry.Method)
		params := typeNameForDefinition(entry.ParamsType)
		response := typeNameForDefinition(entry.ResponseType)
		if entry.ParamsType == "" || entry.ParamsType == "Option<()>" {
			b.WriteString(fmt.Sprintf("func (c RawClient) %s(ctx context.Context) (%s, error) {\n\tvar result %s\n\terr := c.sender.Call(ctx, %q, nil, &result, MethodMetadataByMethod[%q])\n\treturn result, err\n}\n\n", methodName, response, response, entry.Method, entry.Method))
		} else {
			b.WriteString(fmt.Sprintf("func (c RawClient) %s(ctx context.Context, params %s) (%s, error) {\n\tvar result %s\n\terr := c.sender.Call(ctx, %q, params, &result, MethodMetadataByMethod[%q])\n\treturn result, err\n}\n\n", methodName, params, response, response, entry.Method, entry.Method))
		}
	}
	return b.String()
}

func renderMetadata(manifest *Manifest) string {
	var b strings.Builder
	b.WriteString("package protocol\n\n")
	b.WriteString(fmt.Sprintf("const StableProtocolDigest = %q\n", manifest.Stable.Digests["protocolDigest"]))
	b.WriteString(fmt.Sprintf("const StableSchemaDigest = %q\n", manifest.Stable.Digests["schemaDigest"]))
	b.WriteString(fmt.Sprintf("const StableManifestDigest = %q\n", manifest.Stable.Digests["manifestDigest"]))
	b.WriteString(fmt.Sprintf("const ExperimentalProtocolDigest = %q\n", manifest.Experimental.Digests["protocolDigest"]))
	b.WriteString(fmt.Sprintf("const ExperimentalSchemaDigest = %q\n", manifest.Experimental.Digests["schemaDigest"]))
	b.WriteString(fmt.Sprintf("const ExperimentalManifestDigest = %q\n\n", manifest.Experimental.Digests["manifestDigest"]))
	b.WriteString(fmt.Sprintf("const MaxAdditionalContextEntries = %d\n", manifest.ModelContextLimits.MaxAdditionalContextEntries))
	b.WriteString(fmt.Sprintf("const MaxAdditionalContextKeyBytes = %d\n", manifest.ModelContextLimits.MaxAdditionalContextKeyBytes))
	b.WriteString(fmt.Sprintf("const MaxAdditionalContextValueBytes = %d\n", manifest.ModelContextLimits.MaxAdditionalContextValueBytes))
	b.WriteString(fmt.Sprintf("const MaxAdditionalContextTotalBytes = %d\n\n", manifest.ModelContextLimits.MaxAdditionalContextTotalBytes))
	b.WriteString("type ExperimentalFieldMetadata struct { ContainingType string; FieldPath string; Reason string; InspectParams bool; DiscriminatorJSON string }\n\n")
	b.WriteString("type BoundedModelContextFieldMetadata struct { Method string; FieldPath string; LimitProfile string }\n\n")
	b.WriteString("type MethodMetadata struct { Method string; Visibility string; ParamsType string; ResponseType string; ParamsSchemaRef string; ResponseSchemaRef string; SchemaExcludedReason string; InspectParams bool; ManualPayloadConversion string; Experimental bool; Retry string; BoundedModelContextFields []BoundedModelContextFieldMetadata; ExperimentalFields []ExperimentalFieldMetadata }\n\n")
	b.WriteString("var MethodMetadataByMethod = map[string]MethodMetadata{\n")
	stable := mapClientMethods(manifest.Stable.ClientRequests)
	for _, entry := range manifest.Experimental.ClientRequests {
		if entry.SDKVisibility == "internalTestOnly" || entry.SDKVisibility == "excluded" {
			continue
		}
		_, isStable := stable[entry.Method]
		b.WriteString(fmt.Sprintf("\t%q: {Method: %q, Visibility: %q, ParamsType: %q, ResponseType: %q, ParamsSchemaRef: %q, ResponseSchemaRef: %q, SchemaExcludedReason: %q, InspectParams: %v, ManualPayloadConversion: %q, Experimental: %v, Retry: %q%s%s},\n", entry.Method, entry.Method, entry.SDKVisibility, entry.ParamsType, entry.ResponseType, entry.ParamsSchemaRef, entry.ResponseSchemaRef, entry.SchemaExcludedReason, entry.InspectParams, manualPayloadConversionString(entry.ManualPayloadConversion), !isStable, entry.Retry, renderBoundedModelContextFieldMetadata(entry.BoundedModelContextFields), renderExperimentalFieldMetadata(entry.ExperimentalFields)))
	}
	b.WriteString("}\n")
	return b.String()
}

func renderExperimentalFieldMetadata(fields []ExperimentalField) string {
	if len(fields) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString(", ExperimentalFields: []ExperimentalFieldMetadata{")
	for _, field := range fields {
		discriminator := ""
		if len(field.Discriminator) > 0 && string(field.Discriminator) != "null" {
			discriminator = string(field.Discriminator)
		}
		b.WriteString(fmt.Sprintf("{ContainingType: %q, FieldPath: %q, Reason: %q, InspectParams: %v, DiscriminatorJSON: %q},", field.ContainingType, field.FieldPath, field.Reason, field.InspectParams, discriminator))
	}
	b.WriteString("}")
	return b.String()
}

func renderBoundedModelContextFieldMetadata(fields []BoundedModelContextField) string {
	if len(fields) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString(", BoundedModelContextFields: []BoundedModelContextFieldMetadata{")
	for _, field := range fields {
		b.WriteString(fmt.Sprintf("{Method: %q, FieldPath: %q, LimitProfile: %q},", field.Method, field.FieldPath, field.LimitProfile))
	}
	b.WriteString("}")
	return b.String()
}

func manualPayloadConversionString(marker *string) string {
	if marker == nil {
		return ""
	}
	return *marker
}

func mapClientMethods(entries []ClientRequestEntry) map[string]ClientRequestEntry {
	out := make(map[string]ClientRequestEntry, len(entries))
	for _, entry := range entries {
		out[entry.Method] = entry
	}
	return out
}

func renderServerRequestMetadata(manifest *Manifest) string {
	var b strings.Builder
	b.WriteString("package protocol\n\n")
	b.WriteString("type ServerRequestMetadata struct { Method string; Visibility string; ParamsType string; ResponseType string; ParamsSchemaRef string; ResponseSchemaRef string; SchemaExcludedReason string; ManualPayloadConversion string; Capability string; HandlerOwner string; DecodeFunction string; UnitTestOwner string; DocsExampleOwner string; GeneratedOnlyException string; ReviewNote string; Experimental bool }\n\n")
	b.WriteString("var ServerRequestMetadataByMethod = map[string]ServerRequestMetadata{\n")
	handlers := mapServerHandlerMappings(nil, &[]string{})
	for _, mapping := range serverHandlerMappings {
		handlers[mapping.Method] = mapping
	}
	stable := mapServerRequests(manifest.Stable.ServerRequests)
	for _, entry := range manifest.Experimental.ServerRequests {
		mapping := handlers[entry.Method]
		_, isStable := stable[entry.Method]
		b.WriteString(fmt.Sprintf("\t%q: {Method: %q, Visibility: %q, ParamsType: %q, ResponseType: %q, ParamsSchemaRef: %q, ResponseSchemaRef: %q, SchemaExcludedReason: %q, ManualPayloadConversion: %q, Capability: %q, HandlerOwner: %q, DecodeFunction: %q, UnitTestOwner: %q, DocsExampleOwner: %q, GeneratedOnlyException: %q, ReviewNote: %q, Experimental: %v},\n", entry.Method, entry.Method, mapping.Visibility, entry.PayloadType, entry.ResponseType, entry.ParamsSchemaRef, entry.ResponseSchemaRef, entry.SchemaExcludedReason, manualPayloadConversionString(entry.ManualPayloadConversion), mapping.Capability, mapping.HandlerOwner, "decode"+RawMethodName(entry.Method)+"ServerRequest", mapping.UnitTestOwner, mapping.DocsExampleOwner, mapping.GeneratedOnlyException, mapping.ReviewNote, !isStable))
	}
	b.WriteString("}\n")
	return b.String()
}

func renderServerNotificationMetadata(manifest *Manifest, stableServerNotifications map[string]protocolSchemaVariant) string {
	var b strings.Builder
	b.WriteString("package protocol\n\n")
	b.WriteString("import \"encoding/json\"\n\n")
	b.WriteString("type ServerNotificationIdentityExtractor struct { FieldPath string; IdentityName string; Optional bool; TerminalPredicateJSON string }\n\n")
	b.WriteString("type ServerNotificationRouteMetadata struct { ResourceDomain string; WireIdentitySource string; IdentityExtractors []ServerNotificationIdentityExtractor }\n\n")
	b.WriteString("type ServerNotificationRoutingMetadata struct { Method string; PayloadType string; PayloadSchemaRef string; Visibility string; SchemaExcludedReason string; ManualPayloadConversion string; RoutingKind string; Routes []ServerNotificationRouteMetadata; Experimental bool }\n\n")
	b.WriteString("type LifecycleTriggerMetadata struct { Kind string; Method string; Predicate string }\n\n")
	b.WriteString("type RoutingLifecycleMetadata struct { ResourceDomain string; StartMethod string; WireIdentitySource string; StartCompletion LifecycleTriggerMetadata; CleanupTriggers []LifecycleTriggerMetadata; NotificationOptOutDependencies []string }\n\n")
	b.WriteString("var ServerNotificationRoutingByMethod = map[string]ServerNotificationRoutingMetadata{\n")
	stable := stableServerNotificationMethods(manifest, stableServerNotifications)
	for _, entry := range manifest.Experimental.ServerNotifications {
		_, isStable := stable[entry.Method]
		b.WriteString(fmt.Sprintf("\t%q: {Method: %q, PayloadType: %q, PayloadSchemaRef: %q, Visibility: %q, SchemaExcludedReason: %q, ManualPayloadConversion: %q, RoutingKind: %q, Experimental: %v, Routes: []ServerNotificationRouteMetadata{", entry.Method, entry.Method, entry.PayloadType, entry.PayloadSchemaRef, entry.SDKVisibility, entry.SchemaExcludedReason, manualPayloadConversionString(entry.ManualPayloadConversion), entry.RoutingStrategy.Kind, !isStable))
		for _, route := range entry.RoutingStrategy.Routes {
			b.WriteString(fmt.Sprintf("{ResourceDomain: %q, WireIdentitySource: %q, IdentityExtractors: []ServerNotificationIdentityExtractor{", route.ResourceDomain, route.WireIdentitySource))
			for _, extractor := range route.IdentityExtractors {
				terminalPredicate := ""
				if len(extractor.TerminalPredicate) > 0 && string(extractor.TerminalPredicate) != "null" {
					terminalPredicate = string(extractor.TerminalPredicate)
				}
				b.WriteString(fmt.Sprintf("{FieldPath: %q, IdentityName: %q, Optional: %v, TerminalPredicateJSON: %q},", extractor.FieldPath, extractor.IdentityName, extractor.Optional, terminalPredicate))
			}
			b.WriteString("}},")
		}
		b.WriteString("}},\n")
	}
	b.WriteString("}\n")
	b.WriteString("\nfunc DecodeServerNotificationPayload(method string, params json.RawMessage) (any, error) {\n")
	b.WriteString("\tswitch method {\n")
	for _, entry := range manifest.Experimental.ServerNotifications {
		if entry.PayloadType == "" {
			continue
		}
		b.WriteString(fmt.Sprintf("\tcase %q:\n", entry.Method))
		b.WriteString(fmt.Sprintf("\t\tvar payload %s\n", entry.PayloadType))
		b.WriteString("\t\tif err := json.Unmarshal(params, &payload); err != nil {\n")
		b.WriteString("\t\t\treturn nil, err\n")
		b.WriteString("\t\t}\n")
		b.WriteString("\t\treturn payload, nil\n")
	}
	b.WriteString("\t}\n")
	b.WriteString("\treturn append(json.RawMessage(nil), params...), nil\n")
	b.WriteString("}\n")
	b.WriteString("\nvar RoutingLifecycleByStartMethod = map[string]RoutingLifecycleMetadata{\n")
	for _, entry := range manifest.Experimental.RoutingLifecycle {
		b.WriteString(fmt.Sprintf("\t%q: {ResourceDomain: %q, StartMethod: %q, WireIdentitySource: %q, StartCompletion: %s, CleanupTriggers: []LifecycleTriggerMetadata{", entry.StartMethod, entry.ResourceDomain, entry.StartMethod, entry.WireIdentitySource, renderLifecycleTriggerMetadata(entry.StartCompletion)))
		for _, trigger := range entry.CleanupTriggers {
			b.WriteString(renderLifecycleTriggerMetadata(trigger))
			b.WriteString(",")
		}
		b.WriteString(fmt.Sprintf("}, NotificationOptOutDependencies: %s},\n", renderStringSliceLiteral(entry.NotificationOptOutDependencies)))
	}
	b.WriteString("}\n")
	return b.String()
}

func stableServerNotificationMethods(manifest *Manifest, stableServerNotifications map[string]protocolSchemaVariant) map[string]bool {
	stable := make(map[string]bool, len(stableServerNotifications)+len(manifest.Stable.ServerNotifications))
	for method := range stableServerNotifications {
		stable[method] = true
	}
	for _, entry := range manifest.Stable.ServerNotifications {
		stable[entry.Method] = true
	}
	return stable
}

func renderLifecycleTriggerMetadata(trigger LifecycleTrigger) string {
	return fmt.Sprintf("LifecycleTriggerMetadata{Kind: %q, Method: %q, Predicate: %q}", trigger.Kind, trigger.Method, trigger.Predicate)
}

func renderStringSliceLiteral(values []string) string {
	if len(values) == 0 {
		return "nil"
	}
	var b strings.Builder
	b.WriteString("[]string{")
	for _, value := range values {
		b.WriteString(fmt.Sprintf("%q,", value))
	}
	b.WriteString("}")
	return b.String()
}

func mapNotifications(entries []NotificationEntry) map[string]NotificationEntry {
	out := make(map[string]NotificationEntry, len(entries))
	for _, entry := range entries {
		out[entry.Method] = entry
	}
	return out
}

func renderClientNotifications(manifest *Manifest, schema *SchemaBundle) (string, error) {
	clientNotificationSchema, ok := schema.Definition("ClientNotification")
	if !ok {
		return "", fmt.Errorf("complete schema source is missing ClientNotification")
	}
	schemaMethods := clientNotificationMethods(clientNotificationSchema)
	if len(schemaMethods) == 0 {
		return "", fmt.Errorf("ClientNotification schema has no method variants")
	}
	manifestMethods := make(map[string]bool, len(manifest.Experimental.ClientNotifications))
	for _, entry := range manifest.Experimental.ClientNotifications {
		manifestMethods[entry.Method] = true
		if !schemaMethods[entry.Method] {
			return "", fmt.Errorf("client notification %q is missing from ClientNotification schema", entry.Method)
		}
	}
	for method := range schemaMethods {
		if !manifestMethods[method] {
			return "", fmt.Errorf("ClientNotification schema method %q is missing from manifest", method)
		}
	}
	if err := validateClientNotificationPayloads(manifest.Experimental.ClientNotifications, clientNotificationSchema); err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("package protocol\n\nimport (\n\t\"encoding/json\"\n\t\"fmt\"\n)\n\n")
	b.WriteString("type ClientNotificationMethod string\n\nconst (\n")
	for _, entry := range manifest.Experimental.ClientNotifications {
		b.WriteString(fmt.Sprintf("\tClientNotification%s ClientNotificationMethod = %q\n", RawMethodName(entry.Method), entry.Method))
	}
	b.WriteString(")\n\n")
	b.WriteString("type ClientNotification struct {\n\tMethod ClientNotificationMethod `json:\"method\"`\n\tParams json.RawMessage `json:\"params,omitempty\"`\n}\n\n")
	for _, entry := range manifest.Experimental.ClientNotifications {
		typeName := RawMethodName(entry.Method) + "Notification"
		b.WriteString(fmt.Sprintf("type %s struct{}\n\n", typeName))
		b.WriteString(fmt.Sprintf("func New%s() ClientNotification { return %s{}.ClientNotification() }\n\n", typeName, typeName))
		b.WriteString(fmt.Sprintf("func (%s) ClientNotification() ClientNotification { return ClientNotification{Method: ClientNotification%s} }\n\n", typeName, RawMethodName(entry.Method)))
	}
	b.WriteString("func (n ClientNotification) MarshalJSON() ([]byte, error) {\n")
	b.WriteString("\tswitch n.Method {\n")
	for _, entry := range manifest.Experimental.ClientNotifications {
		b.WriteString(fmt.Sprintf("\tcase ClientNotification%s:\n", RawMethodName(entry.Method)))
	}
	b.WriteString("\tdefault:\n\t\treturn nil, fmt.Errorf(\"unsupported client notification method %q\", n.Method)\n\t}\n")
	b.WriteString("\ttype wire struct { Method ClientNotificationMethod `json:\"method\"`; Params json.RawMessage `json:\"params,omitempty\"` }\n")
	b.WriteString("\treturn json.Marshal(wire{Method: n.Method, Params: n.Params})\n}\n")
	return b.String(), nil
}

func validateClientNotificationPayloads(entries []NotificationEntry, schema Schema) error {
	for _, entry := range entries {
		if entry.PayloadType != "" && entry.PayloadType != "Option<()>" {
			return fmt.Errorf("params-bearing client notification %q is not supported yet", entry.Method)
		}
	}
	for _, variant := range schema.OneOf {
		method := clientNotificationVariantMethod(variant)
		if _, ok := variant.Properties["params"]; ok {
			return fmt.Errorf("params-bearing client notification %q is not supported yet", method)
		}
		if stringSliceContains(variant.Required, "params") {
			return fmt.Errorf("params-bearing client notification %q is not supported yet", method)
		}
	}
	return nil
}

func clientNotificationVariantMethod(schema Schema) string {
	method, ok := schema.Properties["method"]
	if !ok {
		return ""
	}
	for _, raw := range method.Enum {
		var value string
		if err := json.Unmarshal(raw, &value); err == nil {
			return value
		}
	}
	return ""
}

func stringSliceContains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func clientNotificationMethods(schema Schema) map[string]bool {
	methods := map[string]bool{}
	for _, variant := range schema.OneOf {
		method, ok := variant.Properties["method"]
		if !ok {
			continue
		}
		for _, raw := range method.Enum {
			var value string
			if err := json.Unmarshal(raw, &value); err == nil {
				methods[value] = true
			}
		}
	}
	return methods
}
