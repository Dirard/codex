package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

// LoginHandle follows one account login. Wait records one outcome for every caller.
type LoginHandle struct {
	client          *Client
	id              string
	authURL         string
	verificationURL string
	userCode        string

	waiter memoWait[*LoginResult]
	// completion is the observed account/login/completed event; only the caller holding
	// the waiter's slot reads or writes it.
	completion *LoginResult
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

// Wait returns the login outcome once account/login/completed arrives and, after a
// successful login, the account is visible. Every call gets the same outcome; when ctx ends
// first, Wait returns its error and a later Wait resumes where it stopped.
func (h *LoginHandle) Wait(ctx context.Context) (*LoginResult, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("account login wait"); err != nil {
		return nil, err
	}
	result, err := h.waiter.wait(ctx, h.waitLogin)
	return result.clone(), err
}

func (h *LoginHandle) waitLogin(ctx context.Context) (*LoginResult, error) {
	// Subscribe before reading the account so an update right after it is not missed.
	accountUpdates := h.client.router.subscribeGlobal()
	defer accountUpdates.Close()
	if h.completion == nil {
		completion, err := h.waitCompletion(ctx)
		if err != nil {
			return nil, err
		}
		h.completion = completion
	}
	result := h.completion.clone()
	if !result.Success {
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

func (h *LoginHandle) waitCompletion(ctx context.Context) (*LoginResult, error) {
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
		return &LoginResult{LoginID: loginID, Success: payload.Success, Error: errorText}, nil
	}
}

func (r *LoginResult) clone() *LoginResult {
	if r == nil {
		return nil
	}
	copied := *r
	return &copied
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
