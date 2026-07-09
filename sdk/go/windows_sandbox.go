package codex

import (
	"context"
	"strings"

	"github.com/openai/codex/sdk/go/protocol"
)

type WindowsSandboxReadinessStatus string

const (
	WindowsSandboxReadinessReady               WindowsSandboxReadinessStatus = "ready"
	WindowsSandboxReadinessNotConfigured       WindowsSandboxReadinessStatus = "notConfigured"
	WindowsSandboxReadinessUpdateRequired      WindowsSandboxReadinessStatus = "updateRequired"
	WindowsSandboxReadinessUnsupportedPlatform WindowsSandboxReadinessStatus = "unsupportedPlatform"
)

type WindowsSandboxReadinessResult struct {
	Status     WindowsSandboxReadinessStatus
	PlatformOS string
	Response   protocol.WindowsSandboxReadinessResponse
}

type UnsupportedPlatformError struct {
	Feature    string
	PlatformOS string
}

func (e *UnsupportedPlatformError) Error() string {
	if e == nil {
		return "codex sdk unsupported platform"
	}
	if e.PlatformOS == "" {
		return "codex sdk unsupported platform: " + e.Feature + " requires windows app-server runtime"
	}
	return "codex sdk unsupported platform: " + e.Feature + " requires windows app-server runtime, got " + e.PlatformOS
}

func (e *UnsupportedPlatformError) SafeJSONRPCMessage() string { return e.Error() }

func (c *WindowsSandboxClient) Readiness(ctx context.Context) (WindowsSandboxReadinessResult, error) {
	if c == nil || c.client == nil {
		return WindowsSandboxReadinessResult{}, &ClosedError{}
	}
	if !c.runtimeIsWindows() {
		return WindowsSandboxReadinessResult{
			Status:     WindowsSandboxReadinessUnsupportedPlatform,
			PlatformOS: c.platformOS(),
		}, nil
	}
	response, err := c.client.Raw().WindowsSandboxReadiness(ctx)
	if err != nil {
		return WindowsSandboxReadinessResult{}, err
	}
	return WindowsSandboxReadinessResult{
		Status:     WindowsSandboxReadinessStatus(response.Status),
		PlatformOS: c.platformOS(),
		Response:   response,
	}, nil
}

func (c *WindowsSandboxClient) SetupStart(ctx context.Context, params protocol.WindowsSandboxSetupStartParams) (protocol.WindowsSandboxSetupStartResponse, error) {
	if c == nil || c.client == nil {
		return protocol.WindowsSandboxSetupStartResponse{}, &ClosedError{}
	}
	if !c.runtimeIsWindows() {
		return protocol.WindowsSandboxSetupStartResponse{}, &UnsupportedPlatformError{
			Feature:    "windows sandbox setup",
			PlatformOS: c.platformOS(),
		}
	}
	return c.client.Raw().WindowsSandboxSetupStart(ctx, params)
}

func (c *WindowsSandboxClient) runtimeIsWindows() bool {
	return strings.EqualFold(c.platformOS(), "windows")
}

func (c *WindowsSandboxClient) platformOS() string {
	if c == nil || c.client == nil {
		return ""
	}
	return c.client.metadata.PlatformOS
}
