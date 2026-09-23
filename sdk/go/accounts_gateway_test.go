package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

type gatewayLoginSendGate struct {
	*scriptedTransport
	started chan struct{}
}

func (tr *gatewayLoginSendGate) Send(ctx context.Context, frame json.RawMessage) error {
	var request struct {
		Method string `json:"method"`
	}
	if err := json.Unmarshal(frame, &request); err != nil {
		return err
	}
	if request.Method == "account/gatewayOAuth/login" {
		close(tr.started)
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-tr.done:
			return &ClosedError{}
		}
	}
	return tr.scriptedTransport.Send(ctx, frame)
}

func TestGatewayOAuthCancellationBeforeLoginIsSent(t *testing.T) {
	transport := &gatewayLoginSendGate{
		scriptedTransport: newScriptedInitializedTransport(t, nil),
		started:           make(chan struct{}),
	}
	transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
		ProviderID: "gateway",
		Required:   true,
	})
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:            transport,
		ExplicitGatewayOAuth: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() {
		_, err := client.Accounts.StartGatewayOAuthLogin(ctx)
		done <- err
	}()
	select {
	case <-transport.started:
	case <-time.After(time.Second):
		t.Fatal("login did not reach the transport")
	}
	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("err = %v, want context cancellation", err)
		}
	case <-time.After(time.Second):
		t.Fatal("cancellation did not stop the unsent login")
	}
	for _, method := range []string{"account/gatewayOAuth/login", "account/gatewayOAuth/cancel"} {
		if methodWasSent(t, transport.scriptedTransport, method) {
			t.Fatalf("unexpected request after cancellation: %s", method)
		}
	}
}

func TestGatewayOAuthFailedCancellationKeepsLoginOwnership(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
		ProviderID: "gateway",
		Required:   true,
	})
	releaseLogin := transport.deferResponse("account/gatewayOAuth/login", json.RawMessage(`{}`))
	defer releaseLogin()
	failMethod(transport, "account/gatewayOAuth/cancel")
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:            transport,
		ExplicitGatewayOAuth: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	go func() {
		waitForMethod(t, transport, "account/gatewayOAuth/login")
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			AuthURL:    protocol.Some("https://example.test/gateway"),
			ProviderID: "gateway",
			Status:     protocol.GatewayOAuthStatusStarted,
		}), nil)
	}()
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	handle, err := client.Accounts.StartGatewayOAuthLogin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	cancel()
	_, err = handle.Wait(ctx)
	var rpcErr *RPCError
	if !errors.Is(err, context.Canceled) || !errors.As(err, &rpcErr) {
		t.Fatalf("err = %v, want context cancellation and remote cancellation failure", err)
	}
	nextCtx, nextCancel := context.WithTimeout(context.Background(), time.Second)
	defer nextCancel()
	_, err = client.Accounts.StartGatewayOAuthLogin(nextCtx)
	var unsupported *UnsupportedError
	if !errors.As(err, &unsupported) {
		t.Fatalf("overlapping login err = %v, want UnsupportedError until cleanup is acknowledged", err)
	}
}
