package codex

import (
	"context"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

const remoteControlPairingPollInterval = 250 * time.Millisecond

type RemoteControlPairingOptions struct {
	ManualCode bool
}

type RemoteControlPairingHandle struct {
	client            *Client
	pairingCode       string
	manualPairingCode string
	environmentID     string
	expiresAt         int64
}

func (c *RemoteControlClient) Enable(ctx context.Context, params protocol.NullableRemoteControlEnableParams) (protocol.RemoteControlEnableResponse, error) {
	if c == nil || c.client == nil {
		return protocol.RemoteControlEnableResponse{}, &ClosedError{}
	}
	return c.client.Raw().RemoteControlEnable(ctx, params)
}

func (c *RemoteControlClient) Disable(ctx context.Context, params protocol.NullableRemoteControlDisableParams) (protocol.RemoteControlDisableResponse, error) {
	if c == nil || c.client == nil {
		return protocol.RemoteControlDisableResponse{}, &ClosedError{}
	}
	return c.client.Raw().RemoteControlDisable(ctx, params)
}

func (c *RemoteControlClient) ReadStatus(ctx context.Context) (protocol.RemoteControlStatusReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.RemoteControlStatusReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().RemoteControlStatusRead(ctx)
}

func (c *RemoteControlClient) StartPairing(ctx context.Context, opts RemoteControlPairingOptions) (*RemoteControlPairingHandle, protocol.RemoteControlPairingStartResponse, error) {
	if c == nil || c.client == nil {
		return nil, protocol.RemoteControlPairingStartResponse{}, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("remote control pairing", "remoteControl/pairing/start"); err != nil {
		return nil, protocol.RemoteControlPairingStartResponse{}, err
	}
	params := protocol.RemoteControlPairingStartParams{}
	if opts.ManualCode {
		params.ManualCode = protocol.SomeNonNull(true)
	}
	response, err := c.client.Raw().RemoteControlPairingStart(ctx, params)
	if err != nil {
		return nil, protocol.RemoteControlPairingStartResponse{}, err
	}
	if response.PairingCode == "" {
		return nil, protocol.RemoteControlPairingStartResponse{}, &UnsupportedError{Reason: "remote control pairing response did not include pairingCode"}
	}
	manualPairingCode, _ := response.ManualPairingCode.Value()
	return &RemoteControlPairingHandle{
		client:            c.client,
		pairingCode:       response.PairingCode,
		manualPairingCode: manualPairingCode,
		environmentID:     response.EnvironmentID,
		expiresAt:         response.ExpiresAt,
	}, response, nil
}

func (c *RemoteControlClient) PairingStatus(ctx context.Context, pairingCode string) (protocol.RemoteControlPairingStatusResponse, error) {
	if c == nil || c.client == nil {
		return protocol.RemoteControlPairingStatusResponse{}, &ClosedError{}
	}
	if err := c.client.ensureHighLevelEnabled("remote control pairing status"); err != nil {
		return protocol.RemoteControlPairingStatusResponse{}, err
	}
	if pairingCode == "" {
		return protocol.RemoteControlPairingStatusResponse{}, &ConfigError{Reason: "remote control pairing status requires pairingCode"}
	}
	params := protocol.RemoteControlPairingStatusParams{}
	params.PairingCode = protocol.Some(pairingCode)
	return c.client.Raw().RemoteControlPairingStatus(ctx, params)
}

func (c *RemoteControlClient) ListClients(ctx context.Context, params protocol.RemoteControlClientsListParams) (protocol.RemoteControlClientsListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.RemoteControlClientsListResponse{}, &ClosedError{}
	}
	return c.client.Raw().RemoteControlClientList(ctx, params)
}

func (c *RemoteControlClient) RevokeClient(ctx context.Context, params protocol.RemoteControlClientsRevokeParams) (protocol.RemoteControlClientsRevokeResponse, error) {
	if c == nil || c.client == nil {
		return protocol.RemoteControlClientsRevokeResponse{}, &ClosedError{}
	}
	return c.client.Raw().RemoteControlClientRevoke(ctx, params)
}

func (h *RemoteControlPairingHandle) ID() string {
	if h == nil {
		return ""
	}
	return h.pairingCode
}

func (h *RemoteControlPairingHandle) ManualPairingCode() string {
	if h == nil {
		return ""
	}
	return h.manualPairingCode
}

func (h *RemoteControlPairingHandle) EnvironmentID() string {
	if h == nil {
		return ""
	}
	return h.environmentID
}

func (h *RemoteControlPairingHandle) ExpiresAt() int64 {
	if h == nil {
		return 0
	}
	return h.expiresAt
}

func (h *RemoteControlPairingHandle) Status(ctx context.Context) (protocol.RemoteControlPairingStatusResponse, error) {
	if h == nil || h.client == nil {
		return protocol.RemoteControlPairingStatusResponse{}, &ClosedError{}
	}
	params := h.statusParams()
	return h.client.Raw().RemoteControlPairingStatus(ctx, params)
}

func (h *RemoteControlPairingHandle) Wait(ctx context.Context) (protocol.RemoteControlPairingStatusResponse, error) {
	for {
		status, err := h.Status(ctx)
		if err != nil || status.Claimed {
			return status, err
		}
		timer := time.NewTimer(remoteControlPairingPollInterval)
		select {
		case <-ctx.Done():
			timer.Stop()
			return protocol.RemoteControlPairingStatusResponse{}, ctx.Err()
		case <-timer.C:
		}
	}
}

func (h *RemoteControlPairingHandle) statusParams() protocol.RemoteControlPairingStatusParams {
	params := protocol.RemoteControlPairingStatusParams{}
	if h == nil {
		return params
	}
	if h.pairingCode != "" {
		params.PairingCode = protocol.Some(h.pairingCode)
	} else if h.manualPairingCode != "" {
		params.ManualPairingCode = protocol.Some(h.manualPairingCode)
	}
	return params
}
