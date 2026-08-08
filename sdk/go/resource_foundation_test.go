package codex

import (
	"context"
	"testing"
)

func TestClientResourceFields(t *testing.T) {
	client, err := NewClient(context.Background(), ClientConfig{Transport: newScriptedInitializedTransport(t, nil)})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	resources := map[string]any{
		"Accounts":             client.Accounts,
		"Threads":              client.Threads,
		"Turns":                client.Turns,
		"Realtime":             client.Realtime,
		"Reviews":              client.Reviews,
		"Models":               client.Models,
		"Config":               client.Config,
		"FileSystem":           client.FileSystem,
		"Commands":             client.Commands,
		"Processes":            client.Processes,
		"Environments":         client.Environments,
		"Skills":               client.Skills,
		"Hooks":                client.Hooks,
		"Plugins":              client.Plugins,
		"Marketplace":          client.Marketplace,
		"Apps":                 client.Apps,
		"MCP":                  client.MCP,
		"RemoteControl":        client.RemoteControl,
		"CollaborationModes":   client.CollaborationModes,
		"ExternalAgents":       client.ExternalAgents,
		"FuzzyFileSearch":      client.FuzzyFileSearch,
		"Memory":               client.Memory,
		"Feedback":             client.Feedback,
		"WindowsSandbox":       client.WindowsSandbox,
		"ExperimentalFeatures": client.ExperimentalFeatures,
		"PermissionProfiles":   client.PermissionProfiles,
	}
	for name, resource := range resources {
		if resource == nil {
			t.Fatalf("%s resource client is nil", name)
		}
	}
}
