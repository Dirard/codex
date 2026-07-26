package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *ExternalAgentsClient) DetectConfig(ctx context.Context, params protocol.ExternalAgentConfigDetectParams) (protocol.ExternalAgentConfigDetectResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ExternalAgentConfigDetectResponse{}, &ClosedError{}
	}
	return c.client.Raw().ExternalAgentConfigDetect(ctx, params)
}

func (c *ExternalAgentsClient) ImportConfig(ctx context.Context, params protocol.ExternalAgentConfigImportParams) (protocol.ExternalAgentConfigImportResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ExternalAgentConfigImportResponse{}, &ClosedError{}
	}
	return c.client.Raw().ExternalAgentConfigImport(ctx, params)
}

func (c *ExternalAgentsClient) RecordImportHistory(ctx context.Context, params protocol.ExternalAgentConfigImportHistoryRecordParams) (protocol.ExternalAgentConfigImportHistoryRecordResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ExternalAgentConfigImportHistoryRecordResponse{}, &ClosedError{}
	}
	return c.client.Raw().ExternalAgentConfigImportRecordHistory(ctx, params)
}

func (c *ExternalAgentsClient) ReadImportHistories(ctx context.Context) (protocol.ExternalAgentConfigImportHistoriesReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ExternalAgentConfigImportHistoriesReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().ExternalAgentConfigImportReadHistories(ctx)
}
