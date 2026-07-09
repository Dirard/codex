package codex

import (
	"bytes"
	"context"
	"errors"
	"strings"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

type APIKey string

func (k APIKey) String() string { return "[redacted]" }

func (c *AccountsClient) LoginWithAPIKey(ctx context.Context, key APIKey) error {
	if c == nil || c.client == nil {
		return &ClosedError{}
	}
	params := protocol.LoginAccountParams{
		TypeValue: "apiKey",
		APIKey:    protocol.SomeNonNull(string(key)),
	}
	_, err := c.client.Raw().AccountLoginStart(ctx, params)
	if err != nil {
		return redactAPIKeyError(err, key)
	}
	return err
}

func (c *AccountsClient) StartChatGPTLogin(ctx context.Context) (*LoginHandle, error) {
	return c.startLogin(ctx, protocol.LoginAccountParams{TypeValue: "chatgpt"})
}

func (c *AccountsClient) StartDeviceCodeLogin(ctx context.Context) (*LoginHandle, error) {
	return c.startLogin(ctx, protocol.LoginAccountParams{TypeValue: "chatgptDeviceCode"})
}

func (c *AccountsClient) Read(ctx context.Context, refreshToken bool) (protocol.GetAccountResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GetAccountResponse{}, &ClosedError{}
	}
	params := protocol.GetAccountParams{}
	if refreshToken {
		params.RefreshToken = protocol.SomeNonNull(true)
	}
	return c.client.Raw().AccountRead(ctx, params)
}

func (c *AccountsClient) Logout(ctx context.Context) error {
	if c == nil || c.client == nil {
		return &ClosedError{}
	}
	_, err := c.client.Raw().AccountLogout(ctx)
	return err
}

func (c *AccountsClient) Usage(ctx context.Context) (protocol.GetAccountTokenUsageResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GetAccountTokenUsageResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountUsageRead(ctx)
}

func (c *AccountsClient) RateLimits(ctx context.Context) (protocol.GetAccountRateLimitsResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GetAccountRateLimitsResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountRateLimitsRead(ctx)
}

func (c *AccountsClient) startLogin(ctx context.Context, params protocol.LoginAccountParams) (*LoginHandle, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("account login", "account/login/start"); err != nil {
		return nil, err
	}
	response, err := c.client.Raw().AccountLoginStart(ctx, params)
	if err != nil {
		return nil, err
	}
	loginID, ok := response.LoginID.Value()
	if !ok || loginID == "" {
		return nil, &UnsupportedError{Reason: "login response did not include loginId"}
	}
	authURL, _ := response.AuthURL.Value()
	return &LoginHandle{client: c.client, id: loginID, authURL: authURL}, nil
}

type redactedAPIKeyError struct {
	sanitized error
}

func redactAPIKeyError(err error, key APIKey) error {
	if err == nil || key == "" {
		return err
	}
	return &redactedAPIKeyError{sanitized: sanitizedAPIKeyError(err, string(key))}
}

func (e *redactedAPIKeyError) Error() string {
	return e.sanitized.Error()
}

func (e *redactedAPIKeyError) Unwrap() error {
	return e.sanitized
}

func sanitizedAPIKeyError(err error, secret string) error {
	var rpcErr *jsonrpc.RPCError
	if errors.As(err, &rpcErr) {
		return &jsonrpc.RPCError{
			Code:    rpcErr.Code,
			Message: redactAPIKeyString(rpcErr.Message, secret),
			Data:    redactAPIKeyBytes(rpcErr.Data, secret),
		}
	}
	return redactedStringError{message: redactAPIKeyString(err.Error(), secret)}
}

type redactedStringError struct {
	message string
}

func (e redactedStringError) Error() string {
	return e.message
}

func redactAPIKeyString(value string, secret string) string {
	return strings.ReplaceAll(value, secret, "[redacted]")
}

func redactAPIKeyBytes(value []byte, secret string) []byte {
	if len(value) == 0 {
		return nil
	}
	return bytes.ReplaceAll(value, []byte(secret), []byte("[redacted]"))
}
