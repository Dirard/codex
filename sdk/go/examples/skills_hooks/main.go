package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/protocol"
)

// codex-go-sdk-resource:Skills
// codex-go-sdk-docs:skills/list
// codex-go-sdk-docs:skills/extraRoots/set
// codex-go-sdk-docs:skills/config/write
// codex-go-sdk-resource:Hooks
// codex-go-sdk-docs:hooks/list
// codex-go-sdk-resource:Plugins
// codex-go-sdk-docs:plugin/skill/read
func skillsAndHooks(ctx context.Context, client *codex.Client) error {
	if _, err := client.Skills.List(ctx, protocol.SkillsListParams{}); err != nil {
		return err
	}
	if _, err := client.Hooks.List(ctx, protocol.HooksListParams{}); err != nil {
		return err
	}
	_, _ = client.Skills.SetExtraRoots(ctx, protocol.SkillsExtraRootsSetParams{})
	_, _ = client.Skills.WriteConfig(ctx, protocol.SkillsConfigWriteParams{})
	_, _ = client.Plugins.ReadSkill(ctx, protocol.PluginSkillReadParams{})
	return nil
}

func main() {}
