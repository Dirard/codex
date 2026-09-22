package codex

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"time"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

type APIKey string

func (k APIKey) String() string { return "[redacted]" }

func (k APIKey) GoString() string { return k.String() }

func (c *AccountsClient) LoginWithAPIKey(ctx context.Context, key APIKey) error {
	return c.loginWithCredentials(ctx, key, protocol.LoginAccountParams{
		TypeValue: "apiKey",
		APIKey:    protocol.SomeNonNull(string(key)),
	})
}

func (c *AccountsClient) LoginWithAmazonBedrock(ctx context.Context, key APIKey, region string) error {
	if region == "" {
		return &ConfigError{Reason: "amazon Bedrock login requires a region"}
	}
	return c.loginWithCredentials(ctx, key, protocol.LoginAccountParams{
		TypeValue: "amazonBedrock",
		APIKey:    protocol.SomeNonNull(string(key)),
		Region:    protocol.SomeNonNull(region),
	})
}

func (c *AccountsClient) loginWithCredentials(ctx context.Context, key APIKey, params protocol.LoginAccountParams) error {
	if c == nil || c.client == nil {
		return &ClosedError{}
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
	return c.client.Raw().AccountUsageRead(ctx, protocol.NullableGetAccountTokenUsageParams{})
}

func (c *AccountsClient) RateLimits(ctx context.Context) (protocol.GetAccountRateLimitsResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GetAccountRateLimitsResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountRateLimitsRead(ctx)
}

func (c *AccountsClient) RateLimitsWithParams(ctx context.Context, params protocol.GetAccountRateLimitsParams) (protocol.GetAccountRateLimitsResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GetAccountRateLimitsResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountRateLimitsReadWithParams(
		ctx,
		protocol.Some(params),
	)
}

func (c *AccountsClient) ConsumeRateLimitResetCredit(ctx context.Context, params protocol.ConsumeAccountRateLimitResetCreditParams) (protocol.ConsumeAccountRateLimitResetCreditResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ConsumeAccountRateLimitResetCreditResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountRateLimitResetCreditConsume(ctx, params)
}

func (c *AccountsClient) ReadWorkspaceMessages(ctx context.Context) (protocol.GetWorkspaceMessagesResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GetWorkspaceMessagesResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountWorkspaceMessagesRead(ctx)
}

func (c *AccountsClient) SendAddCreditsNudgeEmail(ctx context.Context, params protocol.SendAddCreditsNudgeEmailParams) (protocol.SendAddCreditsNudgeEmailResponse, error) {
	if c == nil || c.client == nil {
		return protocol.SendAddCreditsNudgeEmailResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountSendAddCreditsNudgeEmail(ctx, params)
}

func (c *AccountsClient) GatewayOAuthRead(ctx context.Context) (protocol.GatewayOAuthReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.GatewayOAuthReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().AccountGatewayOAuthRead(ctx)
}

func (c *AccountsClient) StartGatewayOAuthLogin(ctx context.Context) (*GatewayOAuthHandle, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("gateway OAuth login", "account/gatewayOAuth/login"); err != nil {
		return nil, err
	}
	if !c.client.gatewayOAuth {
		return nil, &ConfigError{Reason: "gateway OAuth login requires ClientConfig.ExplicitGatewayOAuth"}
	}
	readiness, err := c.GatewayOAuthRead(ctx)
	if err != nil {
		return nil, err
	}
	if !readiness.Required {
		return nil, &UnsupportedError{Reason: "current provider does not use gateway OAuth"}
	}

	// Gateway cancellation is connection-scoped, so managed logins must not overlap.
	c.client.gatewayLoginMu.Lock()
	if c.client.gatewayLogin != nil {
		c.client.gatewayLoginMu.Unlock()
		return nil, &UnsupportedError{Reason: "a gateway OAuth login is already in progress on this client"}
	}
	stream := c.client.router.subscribeGlobal()
	loginCtx, cancelLogin := context.WithCancel(context.Background())
	handle := &GatewayOAuthHandle{
		client:      c.client,
		providerID:  readiness.ProviderID,
		stream:      stream,
		done:        make(chan struct{}),
		cancelLogin: cancelLogin,
	}
	c.client.gatewayLogin = handle
	c.client.gatewayLoginMu.Unlock()
	go func() {
		_, handle.loginErr = c.client.Raw().AccountGatewayOAuthLogin(loginCtx)
		close(handle.done)
		handle.cleanup()
	}()

	for {
		notification, ok := stream.Next(ctx)
		if !ok {
			if err := stream.Err(); err != nil {
				return nil, handle.abort(err)
			}
			// RPC completion closes the stream even when no status event was emitted.
			if _, err := handle.Wait(ctx); err != nil {
				return nil, err
			}
			return handle, nil
		}
		payload, ok := notification.Payload.(protocol.GatewayOAuthChangedNotification)
		if !ok || payload.ProviderID != readiness.ProviderID || payload.Status != protocol.GatewayOAuthStatusStarted {
			continue
		}
		authURL, ok := payload.AuthURL.Value()
		if !ok || authURL == "" {
			continue
		}
		handle.authURL = authURL
		// Only the initiating connection gets the URL; status alone is global.
		stream.Close()
		return handle, nil
	}
}

type GatewayOAuthHandle struct {
	client      *Client
	providerID  string
	authURL     string
	stream      *NotificationStream
	done        chan struct{}
	loginErr    error
	cancelLogin context.CancelFunc
}

type GatewayOAuthResult struct {
	ProviderID string
	Status     protocol.GatewayOAuthStatus
	Error      string
}

func (h *GatewayOAuthHandle) ProviderID() string {
	if h == nil {
		return ""
	}
	return h.providerID
}

func (h *GatewayOAuthHandle) AuthURL() string {
	if h == nil {
		return ""
	}
	return h.authURL
}

func (h *GatewayOAuthHandle) Wait(ctx context.Context) (*GatewayOAuthResult, error) {
	if h == nil || h.client == nil || h.stream == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("gateway OAuth wait"); err != nil {
		return nil, err
	}
	// The login RPC, not the globally broadcast status, acknowledges credential persistence.
	select {
	case <-h.done:
		h.cleanup()
		if h.loginErr != nil {
			return nil, h.loginErr
		}
		return &GatewayOAuthResult{ProviderID: h.providerID, Status: protocol.GatewayOAuthStatusSucceeded}, nil
	case <-ctx.Done():
		return nil, h.abort(ctx.Err())
	}
}

func (h *GatewayOAuthHandle) Cancel(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("gateway OAuth cancel"); err != nil {
		return err
	}
	h.client.gatewayLoginMu.Lock()
	defer h.client.gatewayLoginMu.Unlock()
	if h.client.gatewayLogin != h {
		return nil
	}
	select {
	case <-h.done:
	default:
		_, err := h.client.Raw().AccountGatewayOAuthCancel(ctx)
		if err != nil {
			return err
		}
	}
	h.client.gatewayLogin = nil
	h.cancelLogin()
	h.stream.Close()
	return nil
}

func (h *GatewayOAuthHandle) abort(cause error) error {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	err := h.Cancel(ctx)
	h.cleanup()
	return errors.Join(cause, err)
}

func (h *GatewayOAuthHandle) cleanup() {
	h.client.gatewayLoginMu.Lock()
	if h.client.gatewayLogin == h {
		h.client.gatewayLogin = nil
	}
	h.client.gatewayLoginMu.Unlock()
	h.cancelLogin()
	h.stream.Close()
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
	verificationURL, _ := response.VerificationURL.Value()
	userCode, _ := response.UserCode.Value()
	return &LoginHandle{
		client:          c.client,
		id:              loginID,
		authURL:         authURL,
		verificationURL: verificationURL,
		userCode:        userCode,
	}, nil
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
	const redacted = "[redacted]"
	value = strings.ReplaceAll(value, secret, redacted)
	encoded, err := json.Marshal(secret)
	if err == nil && len(encoded) >= 2 {
		value = strings.ReplaceAll(value, string(encoded[1:len(encoded)-1]), redacted)
	}
	return value
}

func redactAPIKeyBytes(value []byte, secret string) []byte {
	if len(value) == 0 {
		return nil
	}
	var decoded any
	if err := json.Unmarshal(value, &decoded); err == nil {
		redacted, err := json.Marshal(redactAPIKeyJSONValue(decoded, secret))
		if err == nil {
			return redacted
		}
	}
	return []byte(redactAPIKeyString(string(value), secret))
}

func redactAPIKeyJSONValue(value any, secret string) any {
	switch value := value.(type) {
	case string:
		return redactAPIKeyString(value, secret)
	case []any:
		redacted := make([]any, len(value))
		for i, item := range value {
			redacted[i] = redactAPIKeyJSONValue(item, secret)
		}
		return redacted
	case map[string]any:
		redacted := make(map[string]any, len(value))
		for key, item := range value {
			redacted[redactAPIKeyString(key, secret)] = redactAPIKeyJSONValue(item, secret)
		}
		return redacted
	default:
		return value
	}
}
