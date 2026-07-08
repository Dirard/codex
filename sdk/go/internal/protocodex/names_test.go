package protocodex

import "testing"

func TestGoTypeNameHandlesReservedWordsAndNamespaces(t *testing.T) {
	tests := map[string]string{
		"v2/AbsolutePathBuf":  "AbsolutePathBuf",
		"type":                "TypeValue",
		"map":                 "MapValue",
		"thread/start":        "ThreadStart",
		"thread/inject_items": "ThreadInjectItems",
	}

	for input, want := range tests {
		if got := GoTypeName(input); got != want {
			t.Fatalf("GoTypeName(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestTypeNameForDefinitionAppliesInitialisms(t *testing.T) {
	tests := map[string]string{
		"JSONRPCMessage": "JSONRPCMessage",
		"json_rpc_error": "JSONRPCError",
	}
	for input, want := range tests {
		if got := typeNameForDefinition(input); got != want {
			t.Fatalf("typeNameForDefinition(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestUniqueGoNamesSuffixesCollisions(t *testing.T) {
	got := UniqueGoNames([]string{"foo-bar", "foo_bar", "fooBar"})
	want := map[string]string{
		"foo-bar": "FooBar",
		"foo_bar": "FooBar2",
		"fooBar":  "FooBar3",
	}
	for input, wantName := range want {
		if got[input] != wantName {
			t.Fatalf("UniqueGoNames(%q) = %q, want %q", input, got[input], wantName)
		}
	}
}

func TestEnumConstName(t *testing.T) {
	tests := map[string]string{
		"stable":                    "ActiveProtocolModeStable",
		"current_working_directory": "FileSystemSpecialPathCurrentWorkingDirectory",
		"project-roots":             "FileSystemSpecialPathProjectRoots",
	}
	for value, want := range tests {
		var typeName string
		if value == "stable" {
			typeName = "ActiveProtocolMode"
		} else {
			typeName = "FileSystemSpecialPath"
		}
		if got := EnumConstName(typeName, value); got != want {
			t.Fatalf("EnumConstName(%q, %q) = %q, want %q", typeName, value, got, want)
		}
	}
}

func TestRawMethodName(t *testing.T) {
	tests := map[string]string{
		"thread/start":                 "ThreadStart",
		"account/rateLimits/read":      "AccountRateLimitsRead",
		"mcpServer/oauth/login":        "McpServerOauthLogin",
		"fuzzyFileSearch/sessionStart": "FuzzyFileSearchSessionStart",
	}
	for method, want := range tests {
		if got := RawMethodName(method); got != want {
			t.Fatalf("RawMethodName(%q) = %q, want %q", method, got, want)
		}
	}
}
