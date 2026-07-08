package protocodex

import (
	"bytes"
	"encoding/json"
	"fmt"
	"go/format"
	"os"
	"path/filepath"
	"strings"
)

type GenerateOptions struct {
	Mode                   string
	StableSchemaRoot       string
	ExperimentalSchemaRoot string
	ManifestPath           string
	OutDir                 string
	RootOutDir             string
	Check                  bool
}

func Generate(opts GenerateOptions) error {
	if opts.Mode != "both" && opts.Mode != "stable" && opts.Mode != "experimental" {
		return fmt.Errorf("mode must be stable, experimental, or both")
	}
	if opts.Mode != "both" && !opts.Check {
		return fmt.Errorf("mode %s is validation-only and requires --check", opts.Mode)
	}
	manifest, err := LoadManifest(opts.ManifestPath)
	if err != nil {
		return err
	}
	stable, err := LoadSchemaBundle(opts.StableSchemaRoot)
	if err != nil {
		return err
	}
	experimental, err := LoadSchemaBundle(opts.ExperimentalSchemaRoot)
	if err != nil {
		return err
	}
	if err := validateSchemaIntegerFormats("stable", stable); err != nil {
		return err
	}
	if err := validateSchemaIntegerFormats("experimental", experimental); err != nil {
		return err
	}
	if err := validateManifestSchemaRefs("stable", manifest.Stable, stable); err != nil {
		return err
	}
	if err := validateManifestSchemaRefs("experimental", manifest.Experimental, experimental); err != nil {
		return err
	}
	if err := validateReachableSerdeShapeCoverage("stable", manifest.Stable, stable); err != nil {
		return err
	}
	if err := validateReachableSerdeShapeCoverage("experimental", manifest.Experimental, experimental); err != nil {
		return err
	}
	if err := ValidateResourceMappings(manifest, experimental, resourceAPIMappings, serverHandlerMappings); err != nil {
		return err
	}
	if err := validateModeSubset(opts.Mode, manifest, stable, experimental); err != nil {
		return err
	}
	if opts.Check && opts.Mode != "both" {
		return checkModeCanonicalMetadata(opts, manifest, stable, experimental)
	}
	if opts.OutDir == "" || opts.RootOutDir == "" {
		return fmt.Errorf("--out and --root-out are required for mode both")
	}
	files, err := renderFiles(manifest, stable, experimental)
	if err != nil {
		return err
	}
	rootFiles := renderRootFiles(manifest)
	if opts.Check {
		return checkGeneratedFiles(opts.OutDir, opts.RootOutDir, files, rootFiles)
	}
	if err := writeGeneratedFiles(opts.OutDir, files); err != nil {
		return err
	}
	return writeGeneratedFiles(opts.RootOutDir, rootFiles)
}

func validateModeSubset(mode string, manifest *Manifest, stable, experimental *SchemaBundle) error {
	if len(stable.Definitions) == 0 || len(experimental.Definitions) == 0 {
		return fmt.Errorf("schema bundles must not be empty")
	}
	switch mode {
	case "stable":
		if len(stable.Definitions) >= len(experimental.Definitions) {
			return fmt.Errorf("stable schema definitions must be a strict subset of experimental definitions")
		}
		if len(manifest.Stable.ClientRequests) == 0 {
			return fmt.Errorf("stable manifest has no client requests")
		}
	case "experimental":
		if len(manifest.Experimental.ClientRequests) <= len(manifest.Stable.ClientRequests) {
			return fmt.Errorf("experimental manifest must be a superset of stable")
		}
	}
	stableInventory, err := extractProtocolSchemaInventory(stable)
	if err != nil {
		return fmt.Errorf("stable schema inventory: %w", err)
	}
	experimentalInventory, err := extractProtocolSchemaInventory(experimental)
	if err != nil {
		return fmt.Errorf("experimental schema inventory: %w", err)
	}
	if err := validateProtocolSchemaManifestMode("experimental", manifest.Experimental, experimental); err != nil {
		return err
	}
	if err := validateStableProtocolSchemaManifestCoverage("stable", stableInventory, manifest.Stable); err != nil {
		return err
	}
	if err := validateProtocolManifestSchemaSubset("stable", manifest.Stable, stableInventory); err != nil {
		return err
	}
	switch mode {
	case "stable", "both":
		if err := validateProtocolSchemaInventorySubset("stable", stableInventory, experimentalInventory); err != nil {
			return err
		}
	}
	return nil
}

func validateSchemaIntegerFormats(modeName string, bundle *SchemaBundle) error {
	for name, schema := range bundle.Definitions {
		if err := validateSchemaIntegerFormat(modeName+" definition "+name, schema); err != nil {
			return err
		}
	}
	return nil
}

func validateSchemaIntegerFormat(path string, schema Schema) error {
	if (schema.Type == "integer" || hasType(schema.Types, "integer")) && !knownIntegerFormat(schema.Format) {
		return fmt.Errorf("%s has unsupported integer format %q", path, schema.Format)
	}
	for name, property := range schema.Properties {
		if err := validateSchemaIntegerFormat(path+"."+name, property); err != nil {
			return err
		}
	}
	if schema.Items != nil {
		if err := validateSchemaIntegerFormat(path+"[]", *schema.Items); err != nil {
			return err
		}
	}
	if schema.AdditionalProperties != nil {
		if err := validateSchemaIntegerFormat(path+"{}", *schema.AdditionalProperties); err != nil {
			return err
		}
	}
	for index, branch := range schema.AllOf {
		if err := validateSchemaIntegerFormat(fmt.Sprintf("%s.allOf[%d]", path, index), branch); err != nil {
			return err
		}
	}
	for index, branch := range schema.AnyOf {
		if err := validateSchemaIntegerFormat(fmt.Sprintf("%s.anyOf[%d]", path, index), branch); err != nil {
			return err
		}
	}
	for index, branch := range schema.OneOf {
		if err := validateSchemaIntegerFormat(fmt.Sprintf("%s.oneOf[%d]", path, index), branch); err != nil {
			return err
		}
	}
	return nil
}

func knownIntegerFormat(format string) bool {
	switch format {
	case "", "int32", "int64", "uint", "uint16", "uint32", "uint64":
		return true
	default:
		return false
	}
}

func validateManifestSchemaRefs(modeName string, mode ManifestMode, bundle *SchemaBundle) error {
	names := definitionNameMap(sortedDefinitionKeys(bundle))
	for _, entry := range mode.ClientRequests {
		if err := validateRequestManifestSchemaRefs(modeName, "client request", entry.Method, entry.ParamsType, entry.ParamsSchemaRef, entry.ResponseType, entry.ResponseSchemaRef, entry.SDKVisibility, entry.SchemaExcludedReason, entry.ManualPayloadConversion, bundle, names); err != nil {
			return err
		}
	}
	for _, entry := range mode.ServerRequests {
		if err := validateRequestManifestSchemaRefs(modeName, "server request", entry.Method, entry.PayloadType, entry.ParamsSchemaRef, entry.ResponseType, entry.ResponseSchemaRef, entry.SDKVisibility, entry.SchemaExcludedReason, entry.ManualPayloadConversion, bundle, names); err != nil {
			return err
		}
	}
	for _, entry := range mode.ServerNotifications {
		if err := validateNotificationManifestSchemaRef(modeName, "server notification", entry.Method, entry.PayloadType, entry.PayloadSchemaRef, entry.SDKVisibility, entry.SchemaExcludedReason, entry.ManualPayloadConversion, bundle, names); err != nil {
			return err
		}
	}
	for _, entry := range mode.ClientNotifications {
		if err := validateNotificationManifestSchemaRef(modeName, "client notification", entry.Method, entry.PayloadType, entry.PayloadSchemaRef, entry.SDKVisibility, entry.SchemaExcludedReason, entry.ManualPayloadConversion, bundle, names); err != nil {
			return err
		}
	}
	return nil
}

func validateRequestManifestSchemaRefs(modeName, label, method, paramsType, paramsSchemaRef, responseType, responseSchemaRef, visibility, schemaExcludedReason string, manualPayloadConversion *string, bundle *SchemaBundle, names map[string]string) error {
	if err := validateManifestSchemaRef(modeName, label, method, "paramsSchemaRef", paramsType, paramsSchemaRef, visibility, schemaExcludedReason, bundle, names); err != nil {
		return err
	}
	if err := validateManifestSchemaRef(modeName, label, method, "responseSchemaRef", responseType, responseSchemaRef, visibility, schemaExcludedReason, bundle, names); err != nil {
		return err
	}
	return validateManualPayloadConversion(modeName, label, method, manualPayloadConversion)
}

func validateNotificationManifestSchemaRef(modeName, label, method, payloadType, payloadSchemaRef, visibility, schemaExcludedReason string, manualPayloadConversion *string, bundle *SchemaBundle, names map[string]string) error {
	if err := validateManifestSchemaRef(modeName, label, method, "payloadSchemaRef", payloadType, payloadSchemaRef, visibility, schemaExcludedReason, bundle, names); err != nil {
		return err
	}
	return validateManualPayloadConversion(modeName, label, method, manualPayloadConversion)
}

func validateManifestSchemaRef(modeName, label, method, fieldName, typeName, ref, visibility, schemaExcludedReason string, bundle *SchemaBundle, names map[string]string) error {
	if typeName == "" || typeName == "Option<()>" {
		if ref == "" {
			return nil
		}
		if _, ok := schemaDefinitionKeyForRef(ref, bundle, names); !ok {
			return fmt.Errorf("%s %s method %q %s %q is missing from schema bundle", modeName, label, method, fieldName, ref)
		}
		return nil
	}
	if ref == "" {
		if schemaExcludedReasonAllowsMissingSchemaRef(visibility, schemaExcludedReason) {
			return nil
		}
		return fmt.Errorf("%s %s method %q is missing %s for %s", modeName, label, method, fieldName, typeName)
	}
	key, ok := schemaDefinitionKeyForRef(ref, bundle, names)
	if !ok {
		return fmt.Errorf("%s %s method %q %s %q is missing from schema bundle", modeName, label, method, fieldName, ref)
	}
	refType := typeNameForDefinition(key)
	manifestType := typeNameForDefinition(typeName)
	if !payloadTypeNamesEquivalent(refType, manifestType) {
		return fmt.Errorf("%s %s method %q %s %q resolves to %s, want %s", modeName, label, method, fieldName, ref, refType, manifestType)
	}
	return nil
}

func schemaExcludedReasonAllowsMissingSchemaRef(visibility, reason string) bool {
	if reason == "" {
		return false
	}
	return visibility != "public" && visibility != "generatedOnly" && visibility != "handshakeOnly"
}

func validateManualPayloadConversion(modeName, label, method string, marker *string) error {
	if marker == nil {
		return nil
	}
	if strings.TrimSpace(*marker) == "" {
		return fmt.Errorf("%s %s method %q manualPayloadConversion must not be empty", modeName, label, method)
	}
	switch *marker {
	case "manual response payload conversion":
		return nil
	default:
		return fmt.Errorf("%s %s method %q manualPayloadConversion %q requires explicit Go generator support", modeName, label, method, *marker)
	}
}

func checkModeCanonicalMetadata(opts GenerateOptions, manifest *Manifest, stable, experimental *SchemaBundle) error {
	outDir := opts.OutDir
	if outDir == "" {
		outDir = "protocol"
	}
	files, err := renderFiles(manifest, stable, experimental)
	if err != nil {
		return err
	}
	for _, name := range []string{"metadata.go", "server_request_metadata.go", "client_notifications.go", "server_notification_metadata.go"} {
		if err := compareGeneratedFile(filepath.Join(outDir, name), files[name]); err != nil {
			return fmt.Errorf("mode %s metadata validation failed: %w", opts.Mode, err)
		}
	}
	return validateModeGating(opts.Mode, manifest)
}

func validateModeGating(mode string, manifest *Manifest) error {
	stable := mapClientMethods(manifest.Stable.ClientRequests)
	experimental := mapClientMethods(manifest.Experimental.ClientRequests)
	switch mode {
	case "stable":
		for method, entry := range stable {
			if !rawJSONEmptyOrNull(entry.Experimental) {
				return fmt.Errorf("stable manifest includes method-level experimental-only method %q", method)
			}
			experimentalEntry, ok := experimental[method]
			if !ok {
				return fmt.Errorf("stable method %q is missing from experimental manifest", method)
			}
			if len(entry.ExperimentalFields) > 0 && len(experimentalEntry.ExperimentalFields) == 0 {
				return fmt.Errorf("stable method %q declares experimental fields missing from experimental metadata", method)
			}
		}
		if err := validateStableServerRequests(manifest); err != nil {
			return err
		}
		if err := validateStableNotifications("server notification", manifest.Stable.ServerNotifications, manifest.Experimental.ServerNotifications); err != nil {
			return err
		}
		if err := validateStableNotifications("client notification", manifest.Stable.ClientNotifications, manifest.Experimental.ClientNotifications); err != nil {
			return err
		}
	case "experimental":
		for method := range experimental {
			if _, ok := stable[method]; ok {
				continue
			}
			if method == "initialize" {
				return fmt.Errorf("initialize must not be experimental-only")
			}
		}
	}
	return nil
}

func validateStableServerRequests(manifest *Manifest) error {
	experimental := mapServerRequests(manifest.Experimental.ServerRequests)
	for _, entry := range manifest.Stable.ServerRequests {
		if !rawJSONEmptyOrNull(entry.Experimental) {
			return fmt.Errorf("stable server request includes method-level experimental-only method %q", entry.Method)
		}
		if _, ok := experimental[entry.Method]; !ok {
			return fmt.Errorf("stable server request %q is missing from experimental manifest", entry.Method)
		}
	}
	return nil
}

func validateStableNotifications(label string, stableEntries, experimentalEntries []NotificationEntry) error {
	experimental := mapNotifications(experimentalEntries)
	for _, entry := range stableEntries {
		if !rawJSONEmptyOrNull(entry.Experimental) {
			return fmt.Errorf("stable %s includes method-level experimental-only method %q", label, entry.Method)
		}
		if _, ok := experimental[entry.Method]; !ok {
			return fmt.Errorf("stable %s %q is missing from experimental manifest", label, entry.Method)
		}
	}
	return nil
}

func rawJSONEmptyOrNull(raw json.RawMessage) bool {
	if len(raw) == 0 {
		return true
	}
	return bytes.Equal(bytes.TrimSpace(raw), []byte("null"))
}

func renderFiles(manifest *Manifest, stable, schema *SchemaBundle) (map[string]string, error) {
	types, err := renderTypes(schema, manifest)
	if err != nil {
		return nil, err
	}
	stableInventory, err := extractProtocolSchemaInventory(stable)
	if err != nil {
		return nil, fmt.Errorf("stable schema inventory: %w", err)
	}
	clientNotifications, err := renderClientNotifications(manifest, schema)
	if err != nil {
		return nil, err
	}
	return map[string]string{
		"types_generated.go":              types,
		"raw_client.go":                   renderRawClient(manifest),
		"metadata.go":                     renderMetadata(manifest),
		"server_request_metadata.go":      renderServerRequestMetadata(manifest),
		"client_notifications.go":         clientNotifications,
		"server_notification_metadata.go": renderServerNotificationMetadata(manifest, stableInventory.ServerNotifications),
	}, nil
}

func renderRootFiles(manifest *Manifest) map[string]string {
	return map[string]string{
		"handlers_generated.go":          renderHandlersGenerated(manifest),
		"resource_coverage_generated.go": renderResourceCoverageGenerated(manifest),
		filepath.Join("internal", "protocodex", "current_protocol_inventory.generated.md"): renderInventory(manifest),
	}
}

func writeGeneratedFiles(root string, files map[string]string) error {
	for name, content := range files {
		path := filepath.Join(root, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return err
		}
		if strings.HasSuffix(name, ".go") {
			formatted, err := format.Source([]byte(content))
			if err != nil {
				return fmt.Errorf("format %s: %w\n%s", name, err, content)
			}
			content = string(formatted)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			return err
		}
	}
	return nil
}

func checkGeneratedFiles(outDir, rootOutDir string, files, rootFiles map[string]string) error {
	for name, content := range files {
		if err := compareGeneratedFile(filepath.Join(outDir, name), content); err != nil {
			return err
		}
	}
	for name, content := range rootFiles {
		if err := compareGeneratedFile(filepath.Join(rootOutDir, name), content); err != nil {
			return err
		}
	}
	return nil
}

func compareGeneratedFile(path, want string) error {
	if strings.HasSuffix(path, ".go") {
		formatted, err := format.Source([]byte(want))
		if err != nil {
			return err
		}
		want = string(formatted)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if !bytes.Equal(got, []byte(want)) {
		return fmt.Errorf("%s is out of date", path)
	}
	return nil
}
