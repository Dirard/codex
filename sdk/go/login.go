package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

type LoginHandle struct {
	client          *Client
	id              string
	authURL         string
	verificationURL string
	userCode        string
}

type LoginResult struct {
	LoginID string
	Success bool
	Error   string
}

func (h *LoginHandle) ID() string {
	if h == nil {
		return ""
	}
	return h.id
}

func (h *LoginHandle) AuthURL() string {
	if h == nil {
		return ""
	}
	return h.authURL
}

func (h *LoginHandle) VerificationURL() string {
	if h == nil {
		return ""
	}
	return h.verificationURL
}

func (h *LoginHandle) UserCode() string {
	if h == nil {
		return ""
	}
	return h.userCode
}

func (h *LoginHandle) Wait(ctx context.Context) (*LoginResult, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("account login wait"); err != nil {
		return nil, err
	}
	accountUpdates := h.client.router.subscribeGlobal()
	defer accountUpdates.Close()
	stream := h.client.router.subscribeKeys([]routerKey{
		{domain: "account", identity: h.id},
		{domain: "account", identity: ""},
	})
	defer stream.Close()
	for {
		notification, ok := stream.Next(ctx)
		if !ok {
			if err := stream.Err(); err != nil {
				return nil, err
			}
			return nil, &ClosedError{}
		}
		payload, ok := notification.Payload.(protocol.AccountLoginCompletedNotification)
		if !ok {
			continue
		}
		loginID, ok := payload.LoginID.Value()
		if !ok {
			return nil, &UnsupportedError{Reason: "account login completion did not include loginId"}
		}
		if loginID != h.id {
			continue
		}
		errorText, _ := payload.Error.Value()
		result := &LoginResult{LoginID: loginID, Success: payload.Success, Error: errorText}
		if !payload.Success {
			return result, nil
		}
		account, err := h.client.Accounts.Read(ctx, false)
		if err != nil {
			return nil, err
		}
		if _, ok := account.Account.Value(); ok {
			return result, nil
		}
		for {
			notification, ok := accountUpdates.Next(ctx)
			if !ok {
				if err := accountUpdates.Err(); err != nil {
					return nil, err
				}
				return nil, &ClosedError{}
			}
			if _, ok := notification.Payload.(protocol.AccountUpdatedNotification); ok {
				return result, nil
			}
		}
	}
}

func (h *LoginHandle) Cancel(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("account login cancel"); err != nil {
		return err
	}
	_, err := h.client.Raw().AccountLoginCancel(ctx, protocol.CancelLoginAccountParams{LoginID: h.id})
	return err
}
