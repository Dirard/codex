package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestFileSystemThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "read-file",
			method: "fs/readFile",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.ReadFile(ctx, protocol.FsReadFileParams{Path: "/repo/file.go"})
				return err
			},
		},
		{
			name:   "write-file",
			method: "fs/writeFile",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.WriteFile(ctx, protocol.FsWriteFileParams{Path: "/repo/file.go"})
				return err
			},
		},
		{
			name:   "create-directory",
			method: "fs/createDirectory",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.CreateDirectory(ctx, protocol.FsCreateDirectoryParams{Path: "/repo"})
				return err
			},
		},
		{
			name:   "metadata",
			method: "fs/getMetadata",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{Path: "/repo/file.go"})
				return err
			},
		},
		{
			name:   "read-directory",
			method: "fs/readDirectory",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.ReadDirectory(ctx, protocol.FsReadDirectoryParams{Path: "/repo"})
				return err
			},
		},
		{
			name:   "remove",
			method: "fs/remove",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.Remove(ctx, protocol.FsRemoveParams{Path: "/repo/file.go"})
				return err
			},
		},
		{
			name:   "copy",
			method: "fs/copy",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.FileSystem.Copy(ctx, protocol.FsCopyParams{SourcePath: "/repo/a", DestinationPath: "/repo/b"})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })
			failMethod(transport, tt.method)

			err = tt.call(context.Background(), client)
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T(%v), want *RPCError", err, err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
		})
	}
}

func TestFileSystemWatchRoutesAndClosesByOwnedID(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["fs/watch"] = mustJSON(t, protocol.FsWatchResponse{Path: "/repo"})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	watch, response, err := client.FileSystem.Watch(context.Background(), FileSystemWatchOptions{Path: "/repo"})
	if err != nil {
		t.Fatal(err)
	}
	if response.Path != "/repo" {
		t.Fatalf("watch response path = %q, want /repo", response.Path)
	}
	if watch.ID() == "" {
		t.Fatal("watch ID is empty")
	}
	params := requestParamsForMethod(t, transport, "fs/watch")
	assertRequestStringParam(t, params, "watchId", watch.ID())
	assertRequestStringParam(t, params, "path", "/repo")

	stream, err := watch.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("fs/changed", mustJSON(t, protocol.FsChangedNotification{
		WatchID:      watch.ID(),
		ChangedPaths: []protocol.AbsolutePathBuf{"/repo/file.go"},
	}), nil)
	notification := nextTestNotification(t, stream)
	if notification.Method != "fs/changed" {
		t.Fatalf("method = %s, want fs/changed", notification.Method)
	}
	payload, ok := notification.Payload.(protocol.FsChangedNotification)
	if !ok {
		t.Fatalf("payload = %T, want protocol.FsChangedNotification", notification.Payload)
	}
	if payload.WatchID != watch.ID() || len(payload.ChangedPaths) != 1 || payload.ChangedPaths[0] != "/repo/file.go" {
		t.Fatalf("payload = %#v", payload)
	}

	if err := watch.Close(context.Background()); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "fs/unwatch")
	assertRequestStringParam(t, params, "watchId", watch.ID())
	expectClosedStream(t, stream)

	before := methodCount(t, transport, "fs/unwatch")
	assertConflictError(t, watch.Close(context.Background()))
	if _, err := watch.Stream(context.Background()); err == nil {
		t.Fatal("stale watch stream succeeded")
	} else {
		assertConflictError(t, err)
	}
	if got := methodCount(t, transport, "fs/unwatch"); got != before {
		t.Fatalf("fs/unwatch sent %d times after stale close, want %d", got, before)
	}
}

func TestFileSystemWatchRejectsRelativePathBeforeSend(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	_, _, err = client.FileSystem.Watch(context.Background(), FileSystemWatchOptions{Path: "."})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T(%v), want *ConfigError", err, err)
	}
	if methodWasSent(t, transport, "fs/watch") {
		t.Fatal("fs/watch was sent for relative path")
	}
}
