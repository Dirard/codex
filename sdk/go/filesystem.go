package codex

import (
	"context"
	"fmt"

	"github.com/openai/codex/sdk/go/protocol"
)

type FileSystemWatchOptions struct {
	Path string
}

type FileSystemWatchHandle struct {
	client  *Client
	watchID string
}

func (c *FileSystemClient) ReadFile(ctx context.Context, params protocol.FsReadFileParams) (protocol.FsReadFileResponse, error) {
	if c == nil || c.client == nil {
		return protocol.FsReadFileResponse{}, &ClosedError{}
	}
	return c.client.Raw().FsReadFile(ctx, params)
}

func (c *FileSystemClient) WriteFile(ctx context.Context, params protocol.FsWriteFileParams) (protocol.FsWriteFileResponse, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	return c.client.Raw().FsWriteFile(ctx, params)
}

func (c *FileSystemClient) CreateDirectory(ctx context.Context, params protocol.FsCreateDirectoryParams) (protocol.FsCreateDirectoryResponse, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	return c.client.Raw().FsCreateDirectory(ctx, params)
}

func (c *FileSystemClient) GetMetadata(ctx context.Context, params protocol.FsGetMetadataParams) (protocol.FsGetMetadataResponse, error) {
	if c == nil || c.client == nil {
		return protocol.FsGetMetadataResponse{}, &ClosedError{}
	}
	return c.client.Raw().FsGetMetadata(ctx, params)
}

func (c *FileSystemClient) ReadDirectory(ctx context.Context, params protocol.FsReadDirectoryParams) (protocol.FsReadDirectoryResponse, error) {
	if c == nil || c.client == nil {
		return protocol.FsReadDirectoryResponse{}, &ClosedError{}
	}
	return c.client.Raw().FsReadDirectory(ctx, params)
}

func (c *FileSystemClient) Remove(ctx context.Context, params protocol.FsRemoveParams) (protocol.FsRemoveResponse, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	return c.client.Raw().FsRemove(ctx, params)
}

func (c *FileSystemClient) Copy(ctx context.Context, params protocol.FsCopyParams) (protocol.FsCopyResponse, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	return c.client.Raw().FsCopy(ctx, params)
}

func (c *FileSystemClient) Watch(ctx context.Context, opts FileSystemWatchOptions) (*FileSystemWatchHandle, protocol.FsWatchResponse, error) {
	if c == nil || c.client == nil {
		return nil, protocol.FsWatchResponse{}, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("filesystem watch", "fs/watch"); err != nil {
		return nil, protocol.FsWatchResponse{}, err
	}
	if err := validateFileSystemWatchOptions(opts); err != nil {
		return nil, protocol.FsWatchResponse{}, err
	}
	watch := c.reserveWatch()
	params := protocol.FsWatchParams{
		Path:    protocol.AbsolutePathBuf(opts.Path),
		WatchID: watch.watchID,
	}
	response, err := c.client.Raw().FsWatch(ctx, params)
	if err != nil {
		c.releaseWatch(watch.watchID)
		return nil, response, err
	}
	return watch, response, nil
}

func (h *FileSystemWatchHandle) ID() string {
	if h == nil {
		return ""
	}
	return h.watchID
}

func (h *FileSystemWatchHandle) Stream(ctx context.Context) (*NotificationStream, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return nil, err
	}
	stream := h.client.router.subscribe("fs", h.watchID)
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (h *FileSystemWatchHandle) Close(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().FsUnwatch(ctx, protocol.FsUnwatchParams{WatchID: h.watchID})
	if err != nil {
		return err
	}
	if h.client.FileSystem != nil {
		h.client.FileSystem.releaseWatch(h.watchID)
	}
	if h.client.router != nil {
		h.client.router.closeKeys([]routerKey{{domain: "fs", identity: h.watchID}}, nil)
	}
	return nil
}

func (h *FileSystemWatchHandle) ensureActive() error {
	if h == nil || h.client == nil || h.client.FileSystem == nil {
		return &ClosedError{}
	}
	if !h.client.FileSystem.isWatchActive(h.watchID) {
		return &ConflictError{Reason: fmt.Sprintf("filesystem watch %s is no longer active", h.watchID)}
	}
	return nil
}

func (c *FileSystemClient) reserveWatch() *FileSystemWatchHandle {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.activeWatches == nil {
		c.activeWatches = map[string]*FileSystemWatchHandle{}
	}
	c.nextWatchID++
	watch := &FileSystemWatchHandle{
		client:  c.client,
		watchID: fmt.Sprintf("go-fs-watch-%d", c.nextWatchID),
	}
	c.activeWatches[watch.watchID] = watch
	return watch
}

func (c *FileSystemClient) releaseWatch(watchID string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	delete(c.activeWatches, watchID)
}

func (c *FileSystemClient) isWatchActive(watchID string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.activeWatches[watchID] != nil
}

func validateFileSystemWatchOptions(opts FileSystemWatchOptions) error {
	if !isLikelyAbsolutePath(opts.Path) {
		return &ConfigError{Reason: "filesystem watch requires absolute path"}
	}
	return nil
}
