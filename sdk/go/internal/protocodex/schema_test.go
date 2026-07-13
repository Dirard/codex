package protocodex

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSchemaBundleParsesCurrentShapes(t *testing.T) {
	bundle, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}

	initializeResponse := mustDefinition(t, bundle, "InitializeResponse")
	codexHome := mustProperty(t, initializeResponse, "codexHome")
	if got, ok := codexHome.SingleRef(); !ok || got != "#/definitions/v2/AbsolutePathBuf" {
		t.Fatalf("InitializeResponse.codexHome SingleRef() = %q, %v", got, ok)
	}

	additionalPermissions := mustDefinition(t, bundle, "AdditionalPermissionProfile")
	fileSystem := mustProperty(t, additionalPermissions, "fileSystem")
	if got, ok := fileSystem.NullableRef(); !ok || got != "#/definitions/v2/AdditionalFileSystemPermissions" {
		t.Fatalf("AdditionalPermissionProfile.fileSystem NullableRef() = %q, %v", got, ok)
	}

	applyPatch := mustDefinition(t, bundle, "ApplyPatchApprovalParams")
	fileChanges := mustProperty(t, applyPatch, "fileChanges")
	if fileChanges.AdditionalProperties == nil || fileChanges.AdditionalProperties.Ref != "#/definitions/FileChange" {
		t.Fatalf("ApplyPatchApprovalParams.fileChanges additionalProperties = %#v", fileChanges.AdditionalProperties)
	}

	searchResult := mustDefinition(t, bundle, "FuzzyFileSearchResult")
	score := mustProperty(t, searchResult, "score")
	if score.Type != "integer" || score.Format != "uint32" || score.Minimum == nil || score.Minimum.String() != "0.0" {
		t.Fatalf("FuzzyFileSearchResult.score = type %q format %q minimum %v", score.Type, score.Format, score.Minimum)
	}

	clientRequest := mustDefinition(t, bundle, "ClientRequest")
	if len(clientRequest.OneOf) < 100 {
		t.Fatalf("ClientRequest oneOf count = %d, want current tagged union variants", len(clientRequest.OneOf))
	}

	requestID := mustDefinition(t, bundle, "RequestId")
	if len(requestID.AnyOf) != 2 {
		t.Fatalf("RequestId anyOf count = %d, want string or int", len(requestID.AnyOf))
	}
}

func TestLoadManifestRejectsDirectionMismatch(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "manifest.json")
	err := os.WriteFile(path, []byte(`{
		"manifestSchemaVersion": 1,
		"stable": {
			"protocolMode": "stable",
			"clientRequests": [{"variant": "Bad", "direction": "serverToClient", "method": "bad"}],
			"serverRequests": [],
			"serverNotifications": [],
			"clientNotifications": []
		},
		"experimental": {
			"protocolMode": "experimental",
			"clientRequests": [],
			"serverRequests": [],
			"serverNotifications": [],
			"clientNotifications": []
		}
	}`), 0o600)
	if err != nil {
		t.Fatal(err)
	}

	_, err = LoadManifest(path)
	if err == nil {
		t.Fatal("expected direction mismatch error")
	}
}

func TestLoadManifestRejectsUnknownRequestSerializationScope(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "manifest.json")
	err := os.WriteFile(path, []byte(`{
		"manifestSchemaVersion": 1,
		"stable": {
			"protocolMode": "stable",
			"clientRequests": [{
				"variant": "Bad",
				"direction": "clientToServer",
				"method": "bad",
				"requestSerializationScopes": [{"kind": "mystery"}]
			}],
			"serverRequests": [],
			"serverNotifications": [],
			"clientNotifications": []
		},
		"experimental": {
			"protocolMode": "experimental",
			"clientRequests": [],
			"serverRequests": [],
			"serverNotifications": [],
			"clientNotifications": []
		}
	}`), 0o600)
	if err != nil {
		t.Fatal(err)
	}

	_, err = LoadManifest(path)
	if err == nil || !strings.Contains(err.Error(), "unknown request serialization scope") {
		t.Fatalf("err = %v, want unknown request serialization scope", err)
	}
}

func TestLoadSchemaBundleRequiresFlatV2Schema(t *testing.T) {
	dir := t.TempDir()
	jsonDir := filepath.Join(dir, "json")
	if err := os.MkdirAll(jsonDir, 0o755); err != nil {
		t.Fatal(err)
	}
	err := os.WriteFile(filepath.Join(jsonDir, "codex_app_server_protocol.schemas.json"), []byte(`{"definitions":{}}`), 0o600)
	if err != nil {
		t.Fatal(err)
	}

	_, err = LoadSchemaBundle(dir)
	if err == nil || !strings.Contains(err.Error(), "codex_app_server_protocol.v2.schemas.json") {
		t.Fatalf("err = %v, want missing flat v2 schema rejection", err)
	}
}

func TestManifestParsesRequestSerializationScopes(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	var resume ClientRequestEntry
	for _, entry := range manifest.Experimental.ClientRequests {
		if entry.Method == "thread/resume" {
			resume = entry
			break
		}
	}
	if len(resume.RequestSerializationScopes) != 3 {
		t.Fatalf("thread/resume scopes = %d, want 3", len(resume.RequestSerializationScopes))
	}
	first := resume.RequestSerializationScopes[0]
	if first.Kind != "thread" || len(first.IdentityExtractors) != 1 || first.IdentityExtractors[0].IdentityName != "threadId" || len(first.Condition) == 0 {
		t.Fatalf("first thread/resume scope = %#v", first)
	}
}

func TestManifestParsesSerdeAliasesAndNotificationDirections(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}

	appScreenshot := mustSerdeShape(t, manifest, "AppScreenshot")
	fileID := mustSerdeField(t, appScreenshot, "fileId")
	if len(fileID.Aliases) != 1 || fileID.Aliases[0] != "file_id" {
		t.Fatalf("AppScreenshot.fileId aliases = %#v", fileID.Aliases)
	}

	specialPath := mustSerdeShape(t, manifest, "FileSystemSpecialPath")
	if len(specialPath.VariantAliases) != 1 || specialPath.VariantAliases[0].CanonicalWireValue != "project_roots" || specialPath.VariantAliases[0].Aliases[0] != "current_working_directory" {
		t.Fatalf("FileSystemSpecialPath variant aliases = %#v", specialPath.VariantAliases)
	}

	if len(manifest.Experimental.ServerNotifications) == 0 || manifest.Experimental.ServerNotifications[0].Direction != "serverNotification" {
		t.Fatalf("server notification direction = %q", manifest.Experimental.ServerNotifications[0].Direction)
	}
	if len(manifest.Experimental.ClientNotifications) != 1 || manifest.Experimental.ClientNotifications[0].Direction != "clientNotification" {
		t.Fatalf("client notification direction = %#v", manifest.Experimental.ClientNotifications)
	}
}

func mustDefinition(t *testing.T, bundle *SchemaBundle, name string) Schema {
	t.Helper()
	schema, ok := bundle.Definition(name)
	if !ok {
		t.Fatalf("definition %q not found", name)
	}
	return schema
}

func mustProperty(t *testing.T, schema Schema, name string) Schema {
	t.Helper()
	property, ok := schema.Properties[name]
	if !ok {
		t.Fatalf("property %q not found", name)
	}
	return property
}

func mustSerdeShape(t *testing.T, manifest *Manifest, rustType string) SerdeShape {
	t.Helper()
	for _, shape := range manifest.Experimental.SerdeShapes {
		if shape.RustType == rustType {
			return shape
		}
	}
	t.Fatalf("serde shape %q not found", rustType)
	return SerdeShape{}
}

func mustSerdeField(t *testing.T, shape SerdeShape, wireName string) SerdeField {
	t.Helper()
	for _, field := range shape.Fields {
		if field.WireName == wireName {
			return field
		}
	}
	t.Fatalf("serde field %q not found in %#v", wireName, shape)
	return SerdeField{}
}
