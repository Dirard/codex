package codex

import (
	"context"
	"strings"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

type compiledResourceCallsite struct {
	wrapperName string
	convention  string
	callsite    string
	compile     func(context.Context, *Client)
}

var compiledResourceCallsites = map[string]compiledResourceCallsite{
	"account/login/start": {
		wrapperName: "Accounts.StartChatGPTLogin / Accounts.StartDeviceCodeLogin / Accounts.LoginWithAPIKey / LoginHandle",
		convention:  "handle-start",
		callsite:    `login, err := client.Accounts.StartDeviceCodeLogin(ctx); login, err = client.Accounts.StartChatGPTLogin(ctx); err = client.Accounts.LoginWithAPIKey(ctx, codex.APIKey("test-key")); result, err := login.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			login, err := client.Accounts.StartDeviceCodeLogin(ctx)
			_ = err
			login, err = client.Accounts.StartChatGPTLogin(ctx)
			_ = err
			err = client.Accounts.LoginWithAPIKey(ctx, APIKey("test-key"))
			_ = err
			result, err := login.Wait(ctx)
			_, _ = result, err
		},
	},
	"account/login/cancel": {
		wrapperName: "LoginHandle.Cancel",
		convention:  "handle-followup",
		callsite:    `login.Cancel(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var login *LoginHandle
			err := login.Cancel(ctx)
			_ = err
		},
	},
	"account/logout": {
		wrapperName: "Accounts.Logout",
		convention:  "thin",
		callsite:    `client.Accounts.Logout(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			err := client.Accounts.Logout(ctx)
			_ = err
		},
	},
	"account/rateLimits/read": {
		wrapperName: "Accounts.RateLimits",
		convention:  "thin",
		callsite:    `client.Accounts.RateLimits(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			rateLimits, err := client.Accounts.RateLimits(ctx)
			_, _ = rateLimits, err
		},
	},
	"account/usage/read": {
		wrapperName: "Accounts.Usage",
		convention:  "thin",
		callsite:    `client.Accounts.Usage(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			usage, err := client.Accounts.Usage(ctx)
			_, _ = usage, err
		},
	},
	"account/read": {
		wrapperName: "Accounts.Read",
		convention:  "thin",
		callsite:    `client.Accounts.Read(ctx, false)`,
		compile: func(ctx context.Context, client *Client) {
			account, err := client.Accounts.Read(ctx, false)
			_, _ = account, err
		},
	},
	"mcpServer/oauth/login": {
		wrapperName: "MCP.OAuthLogin / MCPOAuthHandle",
		convention:  "handle-start",
		callsite:    `oauth, err := client.MCP.OAuthLogin(ctx, codex.MCPOAuthLoginOptions{Name: "github"}); result, err := oauth.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			oauth, err := client.MCP.OAuthLogin(ctx, MCPOAuthLoginOptions{Name: "github"})
			_ = err
			result, err := oauth.Wait(ctx)
			_, _ = result, err
		},
	},
	"review/start": {
		wrapperName: "Reviews.Start / ReviewHandle",
		convention:  "handle-start",
		callsite:    `review, err := client.Reviews.Start(ctx, codex.ReviewStartOptions{ThreadID: thread.ID()}); result, err := review.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			var thread *Thread
			review, err := client.Reviews.Start(ctx, ReviewStartOptions{ThreadID: thread.ID()})
			_ = err
			result, err := review.Wait(ctx)
			_, _ = result, err
		},
	},
	"thread/start": {
		wrapperName: "Threads.Start",
		convention:  "high-level",
		callsite:    `client.Threads.Start(ctx, codex.ThreadStartOptions{CWD: "/repo", Permissions: "workspace-write"})`,
		compile: func(ctx context.Context, client *Client) {
			thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: "/repo", Permissions: "workspace-write"})
			_, _ = thread, err
		},
	},
	"turn/start": {
		wrapperName: "Thread.Run / Thread.Turn / TurnHandle.Stream",
		convention:  "high-level",
		callsite:    `thread.Run(ctx, codex.Text("inspect this repo"), codex.TurnOptions{Model: "gpt-5.4"}); turn, err := thread.Turn(ctx, codex.Text("continue"), codex.TurnOptions{}); stream, err := turn.Stream(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var thread *Thread
			result, err := thread.Run(ctx, Text("inspect this repo"), TurnOptions{Model: "gpt-5.4"})
			_, _ = result, err
			turn, err := thread.Turn(ctx, Text("continue"), TurnOptions{})
			_ = err
			stream, err := turn.Stream(ctx)
			_, _ = stream, err
		},
	},
	"turn/steer": {
		wrapperName: "TurnHandle.Steer",
		convention:  "handle-followup",
		callsite:    `turn.Steer(ctx, codex.Text("steer toward tests"))`,
		compile: func(ctx context.Context, _ *Client) {
			var turn *TurnHandle
			err := turn.Steer(ctx, Text("steer toward tests"))
			_ = err
		},
	},
	"turn/interrupt": {
		wrapperName: "TurnHandle.Interrupt",
		convention:  "handle-followup",
		callsite:    `turn.Interrupt(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var turn *TurnHandle
			err := turn.Interrupt(ctx)
			_ = err
		},
	},
	"command/exec": {
		wrapperName: "Commands.Exec",
		convention:  "handle-start",
		callsite:    `cmd, err := client.Commands.Exec(ctx, codex.CommandExecOptions{Command: []string{"echo", "ok"}}); stream, err := cmd.Stream(ctx); result, err := cmd.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			cmd, err := client.Commands.Exec(ctx, CommandExecOptions{Command: []string{"echo", "ok"}})
			_ = err
			stream, err := cmd.Stream(ctx)
			_, _ = stream, err
			result, err := cmd.Wait(ctx)
			_, _ = result, err
		},
	},
	"command/exec/write": {
		wrapperName: "CommandHandle.Write / CloseStdin",
		convention:  "handle-followup",
		callsite:    `cmd.Write(ctx, []byte("input")); cmd.CloseStdin(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var cmd *CommandHandle
			err := cmd.Write(ctx, []byte("input"))
			closeErr := cmd.CloseStdin(ctx)
			_, _ = err, closeErr
		},
	},
	"command/exec/terminate": {
		wrapperName: "CommandHandle.Terminate",
		convention:  "handle-followup",
		callsite:    `cmd.Terminate(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var cmd *CommandHandle
			err := cmd.Terminate(ctx)
			_ = err
		},
	},
	"command/exec/resize": {
		wrapperName: "CommandHandle.Resize",
		convention:  "handle-followup",
		callsite:    `cmd.Resize(ctx, codex.TerminalSize{Rows: 24, Cols: 80})`,
		compile: func(ctx context.Context, _ *Client) {
			var cmd *CommandHandle
			err := cmd.Resize(ctx, TerminalSize{Rows: 24, Cols: 80})
			_ = err
		},
	},
	"config/mcpServer/reload": {
		wrapperName: "Config.ReloadMCPServers",
		convention:  "thin",
		callsite:    `client.Config.ReloadMCPServers(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Config.ReloadMCPServers(ctx)
			_, _ = response, err
		},
	},
	"config/read": {
		wrapperName: "Config.Read",
		convention:  "thin",
		callsite:    `client.Config.Read(ctx, protocol.ConfigReadParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Config.Read(ctx, protocol.ConfigReadParams{})
			_, _ = response, err
		},
	},
	"config/value/write": {
		wrapperName: "Config.WriteValue",
		convention:  "thin",
		callsite:    `client.Config.WriteValue(ctx, protocol.ConfigValueWriteParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Config.WriteValue(ctx, protocol.ConfigValueWriteParams{})
			_, _ = response, err
		},
	},
	"config/batchWrite": {
		wrapperName: "Config.BatchWrite",
		convention:  "thin",
		callsite:    `client.Config.BatchWrite(ctx, protocol.ConfigBatchWriteParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Config.BatchWrite(ctx, protocol.ConfigBatchWriteParams{})
			_, _ = response, err
		},
	},
	"configRequirements/read": {
		wrapperName: "Config.ReadRequirements",
		convention:  "thin",
		callsite:    `client.Config.ReadRequirements(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Config.ReadRequirements(ctx)
			_, _ = response, err
		},
	},
	"fs/readFile": {
		wrapperName: "FileSystem.ReadFile",
		convention:  "thin",
		callsite:    `client.FileSystem.ReadFile(ctx, protocol.FsReadFileParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.ReadFile(ctx, protocol.FsReadFileParams{})
			_, _ = response, err
		},
	},
	"fs/writeFile": {
		wrapperName: "FileSystem.WriteFile",
		convention:  "thin",
		callsite:    `client.FileSystem.WriteFile(ctx, protocol.FsWriteFileParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.WriteFile(ctx, protocol.FsWriteFileParams{})
			_, _ = response, err
		},
	},
	"fs/createDirectory": {
		wrapperName: "FileSystem.CreateDirectory",
		convention:  "thin",
		callsite:    `client.FileSystem.CreateDirectory(ctx, protocol.FsCreateDirectoryParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.CreateDirectory(ctx, protocol.FsCreateDirectoryParams{})
			_, _ = response, err
		},
	},
	"fs/getMetadata": {
		wrapperName: "FileSystem.GetMetadata",
		convention:  "thin",
		callsite:    `client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{})
			_, _ = response, err
		},
	},
	"fs/readDirectory": {
		wrapperName: "FileSystem.ReadDirectory",
		convention:  "thin",
		callsite:    `client.FileSystem.ReadDirectory(ctx, protocol.FsReadDirectoryParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.ReadDirectory(ctx, protocol.FsReadDirectoryParams{})
			_, _ = response, err
		},
	},
	"fs/remove": {
		wrapperName: "FileSystem.Remove",
		convention:  "thin",
		callsite:    `client.FileSystem.Remove(ctx, protocol.FsRemoveParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.Remove(ctx, protocol.FsRemoveParams{})
			_, _ = response, err
		},
	},
	"fs/copy": {
		wrapperName: "FileSystem.Copy",
		convention:  "thin",
		callsite:    `client.FileSystem.Copy(ctx, protocol.FsCopyParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.FileSystem.Copy(ctx, protocol.FsCopyParams{})
			_, _ = response, err
		},
	},
	"fs/watch": {
		wrapperName: "FileSystem.Watch",
		convention:  "handle-start",
		callsite:    `watch, start, err := client.FileSystem.Watch(ctx, codex.FileSystemWatchOptions{Path: "/repo"})`,
		compile: func(ctx context.Context, client *Client) {
			watch, start, err := client.FileSystem.Watch(ctx, FileSystemWatchOptions{Path: "/repo"})
			_, _, _ = watch, start, err
		},
	},
	"fs/unwatch": {
		wrapperName: "FileSystemWatchHandle.Close",
		convention:  "handle-followup",
		callsite:    `watch.Close(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var watch *FileSystemWatchHandle
			err := watch.Close(ctx)
			_ = err
		},
	},
	"process/spawn": {
		wrapperName: "Processes.Spawn / process handle",
		convention:  "handle-start",
		callsite:    `proc, start, err := client.Processes.Spawn(ctx, codex.ProcessSpawnOptions{Command: []string{"echo", "ok"}, CWD: "/repo"})`,
		compile: func(ctx context.Context, client *Client) {
			proc, start, err := client.Processes.Spawn(ctx, ProcessSpawnOptions{Command: []string{"echo", "ok"}, CWD: "/repo"})
			_, _, _ = proc, start, err
		},
	},
	"process/writeStdin": {
		wrapperName: "ProcessHandle.WriteStdin / CloseStdin",
		convention:  "handle-followup",
		callsite:    `proc.WriteStdin(ctx, []byte("input")); proc.CloseStdin(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var proc *ProcessHandle
			err := proc.WriteStdin(ctx, []byte("input"))
			closeErr := proc.CloseStdin(ctx)
			_, _ = err, closeErr
		},
	},
	"process/kill": {
		wrapperName: "ProcessHandle.Kill",
		convention:  "handle-followup",
		callsite:    `proc.Kill(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var proc *ProcessHandle
			err := proc.Kill(ctx)
			_ = err
		},
	},
	"process/resizePty": {
		wrapperName: "ProcessHandle.ResizePTY",
		convention:  "handle-followup",
		callsite:    `proc.ResizePTY(ctx, codex.TerminalSize{Rows: 24, Cols: 80})`,
		compile: func(ctx context.Context, _ *Client) {
			var proc *ProcessHandle
			err := proc.ResizePTY(ctx, TerminalSize{Rows: 24, Cols: 80})
			_ = err
		},
	},
	"hooks/list": {
		wrapperName: "Hooks.List",
		convention:  "thin",
		callsite:    `client.Hooks.List(ctx, protocol.HooksListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			hooks, err := client.Hooks.List(ctx, protocol.HooksListParams{})
			_, _ = hooks, err
		},
	},
	"skills/list": {
		wrapperName: "Skills.List",
		convention:  "thin",
		callsite:    `client.Skills.List(ctx, protocol.SkillsListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			skills, err := client.Skills.List(ctx, protocol.SkillsListParams{})
			_, _ = skills, err
		},
	},
	"skills/extraRoots/set": {
		wrapperName: "Skills.SetExtraRoots",
		convention:  "thin",
		callsite:    `client.Skills.SetExtraRoots(ctx, protocol.SkillsExtraRootsSetParams{})`,
		compile: func(ctx context.Context, client *Client) {
			extraRoots, err := client.Skills.SetExtraRoots(ctx, protocol.SkillsExtraRootsSetParams{})
			_, _ = extraRoots, err
		},
	},
	"skills/config/write": {
		wrapperName: "Skills.WriteConfig",
		convention:  "thin",
		callsite:    `client.Skills.WriteConfig(ctx, protocol.SkillsConfigWriteParams{})`,
		compile: func(ctx context.Context, client *Client) {
			config, err := client.Skills.WriteConfig(ctx, protocol.SkillsConfigWriteParams{})
			_, _ = config, err
		},
	},
	"thread/resume": {
		wrapperName: "Threads.Resume",
		convention:  "high-level",
		callsite:    `client.Threads.Resume(ctx, codex.ThreadResumeOptions{ThreadID: "thread-id"})`,
		compile: func(ctx context.Context, client *Client) {
			thread, err := client.Threads.Resume(ctx, ThreadResumeOptions{ThreadID: "thread-id"})
			_, _ = thread, err
		},
	},
	"thread/fork": {
		wrapperName: "Threads.Fork",
		convention:  "high-level",
		callsite:    `client.Threads.Fork(ctx, codex.ThreadForkOptions{ThreadID: "thread-id"})`,
		compile: func(ctx context.Context, client *Client) {
			thread, err := client.Threads.Fork(ctx, ThreadForkOptions{ThreadID: "thread-id"})
			_, _ = thread, err
		},
	},
	"thread/archive": {
		wrapperName: "Threads.Archive",
		convention:  "thin",
		callsite:    `client.Threads.Archive(ctx, protocol.ThreadArchiveParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.Archive(ctx, protocol.ThreadArchiveParams{})
			_, _ = response, err
		},
	},
	"thread/delete": {
		wrapperName: "Threads.Delete",
		convention:  "thin",
		callsite:    `client.Threads.Delete(ctx, protocol.ThreadDeleteParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.Delete(ctx, protocol.ThreadDeleteParams{})
			_, _ = response, err
		},
	},
	"thread/unsubscribe": {
		wrapperName: "Thread.Unsubscribe",
		convention:  "handle-followup",
		callsite:    `thread.Unsubscribe(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var thread *Thread
			err := thread.Unsubscribe(ctx)
			_ = err
		},
	},
	"thread/name/set": {
		wrapperName: "Threads.SetName",
		convention:  "thin",
		callsite:    `client.Threads.SetName(ctx, protocol.ThreadSetNameParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.SetName(ctx, protocol.ThreadSetNameParams{})
			_, _ = response, err
		},
	},
	"thread/goal/set": {
		wrapperName: "Threads.SetGoal",
		convention:  "thin",
		callsite:    `client.Threads.SetGoal(ctx, protocol.ThreadGoalSetParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.SetGoal(ctx, protocol.ThreadGoalSetParams{})
			_, _ = response, err
		},
	},
	"thread/goal/get": {
		wrapperName: "Threads.GetGoal",
		convention:  "thin",
		callsite:    `client.Threads.GetGoal(ctx, protocol.ThreadGoalGetParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.GetGoal(ctx, protocol.ThreadGoalGetParams{})
			_, _ = response, err
		},
	},
	"thread/goal/clear": {
		wrapperName: "Threads.ClearGoal",
		convention:  "thin",
		callsite:    `client.Threads.ClearGoal(ctx, protocol.ThreadGoalClearParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ClearGoal(ctx, protocol.ThreadGoalClearParams{})
			_, _ = response, err
		},
	},
	"thread/metadata/update": {
		wrapperName: "Threads.UpdateMetadata",
		convention:  "thin",
		callsite:    `client.Threads.UpdateMetadata(ctx, protocol.ThreadMetadataUpdateParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.UpdateMetadata(ctx, protocol.ThreadMetadataUpdateParams{})
			_, _ = response, err
		},
	},
	"thread/unarchive": {
		wrapperName: "Threads.Unarchive",
		convention:  "thin",
		callsite:    `client.Threads.Unarchive(ctx, protocol.ThreadUnarchiveParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.Unarchive(ctx, protocol.ThreadUnarchiveParams{})
			_, _ = response, err
		},
	},
	"thread/compact/start": {
		wrapperName: "Threads.StartCompaction",
		convention:  "thin",
		callsite:    `client.Threads.StartCompaction(ctx, protocol.ThreadCompactStartParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.StartCompaction(ctx, protocol.ThreadCompactStartParams{})
			_, _ = response, err
		},
	},
	"thread/shellCommand": {
		wrapperName: "Threads.ShellCommand",
		convention:  "thin",
		callsite:    `client.Threads.ShellCommand(ctx, protocol.ThreadShellCommandParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ShellCommand(ctx, protocol.ThreadShellCommandParams{})
			_, _ = response, err
		},
	},
	"thread/approveGuardianDeniedAction": {
		wrapperName: "Threads.ApproveGuardianDeniedAction",
		convention:  "thin",
		callsite:    `client.Threads.ApproveGuardianDeniedAction(ctx, protocol.ThreadApproveGuardianDeniedActionParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ApproveGuardianDeniedAction(ctx, protocol.ThreadApproveGuardianDeniedActionParams{})
			_, _ = response, err
		},
	},
	"thread/rollback": {
		wrapperName: "Threads.Rollback",
		convention:  "thin",
		callsite:    `client.Threads.Rollback(ctx, protocol.ThreadRollbackParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.Rollback(ctx, protocol.ThreadRollbackParams{})
			_, _ = response, err
		},
	},
	"thread/list": {
		wrapperName: "Threads.List",
		convention:  "thin",
		callsite:    `client.Threads.List(ctx, protocol.ThreadListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.List(ctx, protocol.ThreadListParams{})
			_, _ = response, err
		},
	},
	"thread/loaded/list": {
		wrapperName: "Threads.ListLoaded",
		convention:  "thin",
		callsite:    `client.Threads.ListLoaded(ctx, protocol.ThreadLoadedListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ListLoaded(ctx, protocol.ThreadLoadedListParams{})
			_, _ = response, err
		},
	},
	"thread/read": {
		wrapperName: "Threads.Read",
		convention:  "thin",
		callsite:    `client.Threads.Read(ctx, protocol.ThreadReadParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.Read(ctx, protocol.ThreadReadParams{})
			_, _ = response, err
		},
	},
	"thread/inject_items": {
		wrapperName: "Threads.InjectItems",
		convention:  "thin",
		callsite:    `client.Threads.InjectItems(ctx, protocol.ThreadInjectItemsParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.InjectItems(ctx, protocol.ThreadInjectItemsParams{})
			_, _ = response, err
		},
	},
	"thread/realtime/start": {
		wrapperName: "Realtime.Start / realtime handle",
		convention:  "handle-start",
		callsite:    `session, start, err := client.Realtime.Start(ctx, codex.RealtimeStartOptions{ThreadID: thread.ID()})`,
		compile: func(ctx context.Context, client *Client) {
			var thread *Thread
			session, start, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: thread.ID()})
			_, _, _ = session, start, err
		},
	},
	"thread/realtime/appendAudio": {
		wrapperName: "RealtimeSession.AppendAudio",
		convention:  "handle-followup",
		callsite:    `session.AppendAudio(ctx, codex.AudioChunk{Data: audio})`,
		compile: func(ctx context.Context, _ *Client) {
			var session *RealtimeSession
			audio := "base64-audio"
			err := session.AppendAudio(ctx, AudioChunk{Data: audio})
			_ = err
		},
	},
	"thread/realtime/appendText": {
		wrapperName: "RealtimeSession.AppendText",
		convention:  "handle-followup",
		callsite:    `session.AppendText(ctx, "hello")`,
		compile: func(ctx context.Context, _ *Client) {
			var session *RealtimeSession
			err := session.AppendText(ctx, "hello")
			_ = err
		},
	},
	"thread/realtime/appendSpeech": {
		wrapperName: "RealtimeSession.AppendSpeech",
		convention:  "handle-followup",
		callsite:    `session.AppendSpeech(ctx, codex.SpeechInput{Text: "hello"})`,
		compile: func(ctx context.Context, _ *Client) {
			var session *RealtimeSession
			err := session.AppendSpeech(ctx, SpeechInput{Text: "hello"})
			_ = err
		},
	},
	"thread/realtime/stop": {
		wrapperName: "RealtimeSession.Stop",
		convention:  "handle-followup",
		callsite:    `session.Stop(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var session *RealtimeSession
			err := session.Stop(ctx)
			_ = err
		},
	},
	"thread/realtime/listVoices": {
		wrapperName: "Realtime.ListVoices",
		convention:  "thin",
		callsite:    `client.Realtime.ListVoices(ctx, protocol.ThreadRealtimeListVoicesParams{})`,
		compile: func(ctx context.Context, client *Client) {
			voices, err := client.Realtime.ListVoices(ctx, protocol.ThreadRealtimeListVoicesParams{})
			_, _ = voices, err
		},
	},
	"thread/increment_elicitation": {
		wrapperName: "Thread.IncrementElicitation / Threads.IncrementElicitation",
		convention:  "handle-followup",
		callsite:    `thread.IncrementElicitation(ctx); client.Threads.IncrementElicitation(ctx, thread.ID())`,
		compile: func(ctx context.Context, client *Client) {
			var thread *Thread
			response, err := thread.IncrementElicitation(ctx)
			_, _ = response, err
			response, err = client.Threads.IncrementElicitation(ctx, thread.ID())
			_, _ = response, err
		},
	},
	"thread/decrement_elicitation": {
		wrapperName: "Thread.DecrementElicitation / Threads.DecrementElicitation",
		convention:  "handle-followup",
		callsite:    `thread.DecrementElicitation(ctx); client.Threads.DecrementElicitation(ctx, thread.ID())`,
		compile: func(ctx context.Context, client *Client) {
			var thread *Thread
			response, err := thread.DecrementElicitation(ctx)
			_, _ = response, err
			response, err = client.Threads.DecrementElicitation(ctx, thread.ID())
			_, _ = response, err
		},
	},
	"thread/settings/update": {
		wrapperName: "Threads.UpdateSettings",
		convention:  "thin",
		callsite:    `client.Threads.UpdateSettings(ctx, protocol.ThreadSettingsUpdateParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.UpdateSettings(ctx, protocol.ThreadSettingsUpdateParams{})
			_, _ = response, err
		},
	},
	"thread/memoryMode/set": {
		wrapperName: "Threads.SetMemoryMode",
		convention:  "thin",
		callsite:    `client.Threads.SetMemoryMode(ctx, protocol.ThreadMemoryModeSetParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.SetMemoryMode(ctx, protocol.ThreadMemoryModeSetParams{})
			_, _ = response, err
		},
	},
	"thread/backgroundTerminals/clean": {
		wrapperName: "Threads.CleanBackgroundTerminals",
		convention:  "thin",
		callsite:    `client.Threads.CleanBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsCleanParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.CleanBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsCleanParams{})
			_, _ = response, err
		},
	},
	"thread/backgroundTerminals/list": {
		wrapperName: "Threads.ListBackgroundTerminals",
		convention:  "thin",
		callsite:    `client.Threads.ListBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ListBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsListParams{})
			_, _ = response, err
		},
	},
	"thread/backgroundTerminals/terminate": {
		wrapperName: "Threads.TerminateBackgroundTerminal",
		convention:  "thin",
		callsite:    `client.Threads.TerminateBackgroundTerminal(ctx, protocol.ThreadBackgroundTerminalsTerminateParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.TerminateBackgroundTerminal(ctx, protocol.ThreadBackgroundTerminalsTerminateParams{})
			_, _ = response, err
		},
	},
	"thread/search": {
		wrapperName: "Threads.Search",
		convention:  "thin",
		callsite:    `client.Threads.Search(ctx, protocol.ThreadSearchParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.Search(ctx, protocol.ThreadSearchParams{})
			_, _ = response, err
		},
	},
	"thread/turns/list": {
		wrapperName: "Threads.ListTurns",
		convention:  "thin",
		callsite:    `client.Threads.ListTurns(ctx, protocol.ThreadTurnsListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ListTurns(ctx, protocol.ThreadTurnsListParams{})
			_, _ = response, err
		},
	},
	"thread/items/list": {
		wrapperName: "Threads.ListItems",
		convention:  "thin",
		callsite:    `client.Threads.ListItems(ctx, protocol.ThreadItemsListParams{})`,
		compile: func(ctx context.Context, client *Client) {
			response, err := client.Threads.ListItems(ctx, protocol.ThreadItemsListParams{})
			_, _ = response, err
		},
	},
}

func TestResourceCallsites(t *testing.T) {
	for _, row := range generatedResourceCoverage {
		if row.SDKVisibility != "public" || !strings.HasPrefix(row.ImplementationStatus, "implemented-") {
			continue
		}
		callsite, ok := compiledResourceCallsites[row.Method]
		if !ok {
			t.Fatalf("%s is %s but has no compiled resource callsite", row.Method, row.ImplementationStatus)
		}
		if row.WrapperName != callsite.wrapperName {
			t.Fatalf("%s wrapper = %q, want %q", row.Method, row.WrapperName, callsite.wrapperName)
		}
		if row.SignatureConventionID != callsite.convention {
			t.Fatalf("%s signature convention = %q, want %q", row.Method, row.SignatureConventionID, callsite.convention)
		}
		if row.CompileCallsite != callsite.callsite {
			t.Fatalf("%s compile callsite = %q, want %q", row.Method, row.CompileCallsite, callsite.callsite)
		}
		if callsite.compile == nil {
			t.Fatalf("%s has no typed compile function", row.Method)
		}
	}
	for method := range compiledResourceCallsites {
		if !hasImplementedResourceCoverage(method) {
			t.Fatalf("%s has a compiled callsite but is not implemented public resource coverage", method)
		}
	}

	if false {
		var client *Client
		var ctx context.Context
		for _, callsite := range compiledResourceCallsites {
			callsite.compile(ctx, client)
		}
	}
}

func hasImplementedResourceCoverage(method string) bool {
	for _, row := range generatedResourceCoverage {
		if row.Method == method &&
			row.SDKVisibility == "public" &&
			strings.HasPrefix(row.ImplementationStatus, "implemented-") {
			return true
		}
	}
	return false
}
