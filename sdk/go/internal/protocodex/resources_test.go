package protocodex

import (
	"errors"
	"path/filepath"
	"strings"
	"testing"
)

func TestResourceMappingsCoverCurrentManifest(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}

	if err := ValidateResourceMappings(manifest, testExperimentalSchemaBundle(t), resourceAPIMappings, serverHandlerMappings); err != nil {
		t.Fatal(err)
	}
}

func TestResourceMappingsRejectMissingPublicClientMethod(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mappings := dropResourceMapping("thread/start")

	err = ValidateResourceMappings(manifest, testExperimentalSchemaBundle(t), mappings, serverHandlerMappings)
	if !hasValidationError(err, "thread/start") {
		t.Fatalf("error = %v, want missing thread/start mapping", err)
	}
}

func TestResourceMappingsRejectProtocolParamsForHighLevelCallsite(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mappings := replaceResourceMapping("thread/start", func(mapping ResourceAPIMapping) ResourceAPIMapping {
		mapping.CompileCallsite = "client.Threads.Start(ctx, protocol.ThreadStartParams{})"
		return mapping
	})

	err = ValidateResourceMappings(manifest, testExperimentalSchemaBundle(t), mappings, serverHandlerMappings)
	if !hasValidationError(err, "thread/start") || !hasValidationError(err, "protocol.") {
		t.Fatalf("error = %v, want high-level protocol params validation error", err)
	}
}

func TestResourceMappingsRejectIdentityBearingHandleStartWithoutRootOptions(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mappings := replaceResourceMapping("process/spawn", func(mapping ResourceAPIMapping) ResourceAPIMapping {
		mapping.CompileCallsite = "client.Processes.Spawn(ctx, processParams)"
		return mapping
	})

	err = ValidateResourceMappings(manifest, testExperimentalSchemaBundle(t), mappings, serverHandlerMappings)
	if !hasValidationError(err, "process/spawn") || !hasValidationError(err, "root SDK options") {
		t.Fatalf("error = %v, want identity-bearing handle-start options validation error", err)
	}
}

func TestResourceMappingsRejectMissingServerHandler(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mappings := dropServerHandlerMapping("currentTime/read")

	err = ValidateResourceMappings(manifest, testExperimentalSchemaBundle(t), resourceAPIMappings, mappings)
	if !hasValidationError(err, "currentTime/read") {
		t.Fatalf("error = %v, want missing currentTime/read handler mapping", err)
	}
}

func TestResourceMappingsRejectNonPublicServerHandlerWithoutException(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mappings := replaceServerHandlerMapping("applyPatchApproval", func(mapping ServerHandlerMapping) ServerHandlerMapping {
		mapping.GeneratedOnlyException = ""
		return mapping
	})

	err = ValidateResourceMappings(manifest, testExperimentalSchemaBundle(t), resourceAPIMappings, mappings)
	if !hasValidationError(err, "applyPatchApproval") || !hasValidationError(err, "non-public server handler mapping has no exception") {
		t.Fatalf("error = %v, want missing generated-only exception validation error", err)
	}
}

func testManifestPath(t *testing.T) string {
	t.Helper()
	return filepath.Join("manifest", "app_server_protocol_manifest.json")
}

func testExperimentalSchemaBundle(t *testing.T) *SchemaBundle {
	t.Helper()
	schema, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	return schema
}

func dropResourceMapping(method string) []ResourceAPIMapping {
	var mappings []ResourceAPIMapping
	for _, mapping := range resourceAPIMappings {
		if mapping.Method != method {
			mappings = append(mappings, mapping)
		}
	}
	return mappings
}

func replaceResourceMapping(method string, replace func(ResourceAPIMapping) ResourceAPIMapping) []ResourceAPIMapping {
	mappings := append([]ResourceAPIMapping(nil), resourceAPIMappings...)
	for i, mapping := range mappings {
		if mapping.Method == method {
			mappings[i] = replace(mapping)
		}
	}
	return mappings
}

func dropServerHandlerMapping(method string) []ServerHandlerMapping {
	var mappings []ServerHandlerMapping
	for _, mapping := range serverHandlerMappings {
		if mapping.Method != method {
			mappings = append(mappings, mapping)
		}
	}
	return mappings
}

func replaceServerHandlerMapping(method string, replace func(ServerHandlerMapping) ServerHandlerMapping) []ServerHandlerMapping {
	mappings := append([]ServerHandlerMapping(nil), serverHandlerMappings...)
	for i, mapping := range mappings {
		if mapping.Method == method {
			mappings[i] = replace(mapping)
		}
	}
	return mappings
}

func hasValidationError(err error, substring string) bool {
	if err == nil {
		return false
	}
	var validationErr *ValidationError
	if errors.As(err, &validationErr) {
		return strings.Contains(validationErr.Error(), substring)
	}
	return strings.Contains(err.Error(), substring)
}
