package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestThreadsResourceWrappersSendMatrixMethods(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	thread := &Thread{client: client, id: "thread-1"}

	calls := []struct {
		name     string
		method   string
		threadID string
		call     func() error
	}{
		{
			name: "resume", method: "thread/resume", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Resume(ctx, ThreadResumeOptions{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "fork", method: "thread/fork", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Fork(ctx, ThreadForkOptions{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "archive", method: "thread/archive", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Archive(ctx, protocol.ThreadArchiveParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "delete", method: "thread/delete", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Delete(ctx, protocol.ThreadDeleteParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "unsubscribe handle", method: "thread/unsubscribe", threadID: "thread-1",
			call: func() error { return thread.Unsubscribe(ctx) },
		},
		{
			name: "set name", method: "thread/name/set", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.SetName(ctx, protocol.ThreadSetNameParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "set goal", method: "thread/goal/set", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.SetGoal(ctx, protocol.ThreadGoalSetParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "get goal", method: "thread/goal/get", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.GetGoal(ctx, protocol.ThreadGoalGetParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "clear goal", method: "thread/goal/clear", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.ClearGoal(ctx, protocol.ThreadGoalClearParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "update metadata", method: "thread/metadata/update", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.UpdateMetadata(ctx, protocol.ThreadMetadataUpdateParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "unarchive", method: "thread/unarchive", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Unarchive(ctx, protocol.ThreadUnarchiveParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "start compaction", method: "thread/compact/start", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.StartCompaction(ctx, protocol.ThreadCompactStartParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "shell command", method: "thread/shellCommand", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.ShellCommand(ctx, protocol.ThreadShellCommandParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "approve guardian denied action", method: "thread/approveGuardianDeniedAction", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.ApproveGuardianDeniedAction(ctx, protocol.ThreadApproveGuardianDeniedActionParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "rollback", method: "thread/rollback", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Rollback(ctx, protocol.ThreadRollbackParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "list", method: "thread/list",
			call: func() error {
				_, err := client.Threads.List(ctx, protocol.ThreadListParams{})
				return err
			},
		},
		{
			name: "list loaded", method: "thread/loaded/list",
			call: func() error {
				_, err := client.Threads.ListLoaded(ctx, protocol.ThreadLoadedListParams{})
				return err
			},
		},
		{
			name: "read", method: "thread/read", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.Read(ctx, protocol.ThreadReadParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "inject items", method: "thread/inject_items", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.InjectItems(ctx, protocol.ThreadInjectItemsParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "increment elicitation handle", method: "thread/increment_elicitation", threadID: "thread-1",
			call: func() error {
				_, err := thread.IncrementElicitation(ctx)
				return err
			},
		},
		{
			name: "increment elicitation root", method: "thread/increment_elicitation", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.IncrementElicitation(ctx, thread.ID())
				return err
			},
		},
		{
			name: "decrement elicitation handle", method: "thread/decrement_elicitation", threadID: "thread-1",
			call: func() error {
				_, err := thread.DecrementElicitation(ctx)
				return err
			},
		},
		{
			name: "decrement elicitation root", method: "thread/decrement_elicitation", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.DecrementElicitation(ctx, thread.ID())
				return err
			},
		},
		{
			name: "update settings", method: "thread/settings/update", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.UpdateSettings(ctx, protocol.ThreadSettingsUpdateParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "set memory mode", method: "thread/memoryMode/set", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.SetMemoryMode(ctx, protocol.ThreadMemoryModeSetParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "clean background terminals", method: "thread/backgroundTerminals/clean", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.CleanBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsCleanParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "list background terminals", method: "thread/backgroundTerminals/list", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.ListBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsListParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "terminate background terminal", method: "thread/backgroundTerminals/terminate", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.TerminateBackgroundTerminal(ctx, protocol.ThreadBackgroundTerminalsTerminateParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "search", method: "thread/search",
			call: func() error {
				_, err := client.Threads.Search(ctx, protocol.ThreadSearchParams{})
				return err
			},
		},
		{
			name: "list turns", method: "thread/turns/list", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.ListTurns(ctx, protocol.ThreadTurnsListParams{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name: "list items", method: "thread/items/list", threadID: "thread-1",
			call: func() error {
				_, err := client.Threads.ListItems(ctx, protocol.ThreadItemsListParams{ThreadID: "thread-1"})
				return err
			},
		},
	}

	for _, tt := range calls {
		t.Run(tt.name, func(t *testing.T) {
			failMethod(transport, tt.method)
			err := tt.call()
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T, want *RPCError", err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
			if tt.threadID != "" {
				assertRequestThreadID(t, requestParamsForMethod(t, transport, tt.method), tt.threadID)
			}
		})
	}
}

func newStage5Client(t *testing.T) (*Client, *scriptedTransport) {
	t.Helper()
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	return client, transport
}

func failMethod(transport *scriptedTransport, method string) {
	transport.errors[method] = &RPCError{Code: -32000, Message: "captured request"}
}

func assertRequestThreadID(t *testing.T, params json.RawMessage, want string) {
	t.Helper()
	var raw struct {
		ThreadID string `json:"threadId"`
	}
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if raw.ThreadID != want {
		t.Fatalf("threadId = %q, want %q; params = %s", raw.ThreadID, want, params)
	}
}
