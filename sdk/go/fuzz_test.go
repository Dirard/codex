package codex

import (
	"context"
	"encoding/json"
	"sort"
	"strings"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

var routerFuzzSeeds = map[string][]string{
	"item/completed": {`{"threadId":"t","turnId":"u","completedAtMs":5,"item":{"type":"agentMessage","id":"i","text":"done","phase":"final_answer"}}`},
	"turn/completed": {
		`{"threadId":"t","turn":{"id":"u","items":[],"status":"completed"}}`,
		`{"threadId":"t","turn":{"id":"u","status":7}}`,
	},
	"item/agentMessage/delta":   {`{"threadId":"t","turnId":"u","itemId":"i","delta":"he"}`},
	"process/exited":            {`{"processHandle":"p","exitCode":0,"stdout":"","stderr":"","stdoutCapReached":false,"stderrCapReached":false}`},
	"thread/realtime/closed":    {`{"threadId":"t"}`},
	"account/login/completed":   {`{"loginId":"l","success":true}`, `{"success":true}`},
	"thread/tokenUsage/updated": {`{"threadId":"t","turnId":"u","tokenUsage":{}}`},
}

// FuzzNotificationRouterRoute checks that no single notification, however malformed,
// panics the router, closes it, or escapes global observers.
func FuzzNotificationRouterRoute(f *testing.F) {
	methods := make([]string, 0, len(protocol.ServerNotificationRoutingByMethod))
	for method := range protocol.ServerNotificationRoutingByMethod {
		methods = append(methods, method)
	}
	sort.Strings(methods)
	for _, method := range methods {
		f.Add(method, []byte(`{"threadId":"t","turnId":"u","turn":{"id":"u"}}`))
		for _, seed := range routerFuzzSeeds[method] {
			f.Add(method, []byte(seed))
		}
	}
	f.Add("thread/realtime/unknownKind", []byte(`{"threadId":"t"}`))
	f.Add("future/method", []byte(`[1,2]`))
	limits, err := normalizeLimits(ClientLimits{PendingTurnMap: 4, PendingTurnQueue: 4})
	if err != nil {
		f.Fatal(err)
	}
	f.Fuzz(func(t *testing.T, method string, params []byte) {
		router := newNotificationRouter(limits)
		defer router.close()
		global := router.subscribeGlobal()
		turn := router.subscribeTurn("t", "u")
		defer turn.Close()
		if err := router.route(context.Background(), method, json.RawMessage(params), nil); err != nil {
			t.Fatalf("route returned %v", err)
		}
		router.mu.Lock()
		closed := router.closed
		withinBudget := router.pendingBytes >= 0 && router.pendingBytes <= limits.PendingNotificationBytes
		router.mu.Unlock()
		if closed {
			t.Fatal("one notification closed the router")
		}
		if !withinBudget {
			t.Fatal("pending byte accounting left its budget")
		}
		select {
		case notification := <-global.Events():
			_, known := protocol.ServerNotificationRoutingByMethod[method]
			switch notification.Payload.(type) {
			case UnknownNotification:
				if known {
					t.Fatalf("known method %s delivered as unknown", method)
				}
			case MalformedNotification:
				if !known {
					t.Fatalf("unknown method %s delivered as malformed", method)
				}
			default:
				if !known {
					t.Fatalf("unknown method %s delivered as %T", method, notification.Payload)
				}
			}
		default:
			t.Fatal("global observers did not receive the notification")
		}
	})
}

// FuzzInitializeCompatibility checks the initialize response check: no panic, strict mode
// accepts only this build's digest and mode, and no policy accepts a PATH-discovered
// runtime whose protocol does not match.
func FuzzInitializeCompatibility(f *testing.F) {
	for _, payload := range []json.RawMessage{currentInitializePayload(), stableInitializePayload(), legacyInitializePayload()} {
		f.Add([]byte(payload), uint8(0), uint8(0), uint8(0))
		f.Add([]byte(payload), uint8(1), uint8(1), uint8(1))
		f.Add([]byte(payload), uint8(0), uint8(2), uint8(2))
	}
	mismatched := strings.Replace(string(currentInitializePayload()), protocol.ExperimentalProtocolDigest, "sha256:other", 1)
	f.Add([]byte(mismatched), uint8(0), uint8(1), uint8(0))
	f.Add([]byte(mismatched), uint8(0), uint8(1), uint8(2))
	release := strings.Replace(mismatched, "codex_app_server/0.0.0", "codex_app_server/0.164.0", 1)
	f.Add([]byte(release), uint8(0), uint8(1), uint8(1))
	f.Fuzz(func(t *testing.T, payload []byte, mode uint8, policy uint8, source uint8) {
		env, err := decodeInitializeCompatibility(json.RawMessage(payload))
		if err != nil {
			return
		}
		cfg := ClientConfig{
			ProtocolMode:  ProtocolMode(mode % 2),
			Compatibility: CompatibilityPolicy(policy % 3),
		}
		origin := runtimeSource(source % 3)
		if _, _, _, err := validateInitializeCompatibility(env, cfg, origin, "/fuzz/codex"); err != nil {
			return
		}
		expectedDigest, expectedMode := expectedProtocol(cfg.ProtocolMode)
		matches := observedProtocolDigest(env, cfg.ProtocolMode) == expectedDigest &&
			env.ActiveProtocolMode != nil && *env.ActiveProtocolMode == expectedMode
		if matches {
			return
		}
		if cfg.Compatibility == CompatibilityStrict {
			t.Fatalf("strict mode accepted a mismatched runtime: %s", payload)
		}
		if origin == runtimeSourcePathLookup {
			t.Fatalf("a PATH runtime with a mismatched protocol was accepted under %v: %s", cfg.Compatibility, payload)
		}
		if !looksLikeDevBuild(env.UserAgent) {
			t.Fatalf("an override accepted a runtime that is not a dev build (%q)", env.UserAgent)
		}
	})
}
