package codex

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"sync/atomic"

	"github.com/openai/codex/sdk/go/protocol"
)

var fuzzySearchClientPrefixFallback atomic.Uint64

type FuzzySearchSessionOptions struct {
	Roots []string
}

type FuzzySearchUpdate struct {
	Query string
}

type FuzzySearchSession struct {
	client    *Client
	sessionID string
}

func (c *FuzzyFileSearchClient) Search(ctx context.Context, params protocol.FuzzyFileSearchParams) (protocol.FuzzyFileSearchResponse, error) {
	if c == nil || c.client == nil {
		return protocol.FuzzyFileSearchResponse{}, &ClosedError{}
	}
	return c.client.Raw().FuzzyFileSearch(ctx, params)
}

func (c *FuzzyFileSearchClient) StartSession(ctx context.Context, opts FuzzySearchSessionOptions) (*FuzzySearchSession, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("fuzzy file search session", "fuzzyFileSearch/sessionStart"); err != nil {
		return nil, err
	}
	session := c.reserveSession()
	_, err := c.client.Raw().FuzzyFileSearchSessionStart(ctx, protocol.FuzzyFileSearchSessionStartParams{
		Roots:     opts.Roots,
		SessionID: session.sessionID,
	})
	if err != nil {
		c.releaseSession(session.sessionID)
		return nil, err
	}
	return session, nil
}

func (s *FuzzySearchSession) ID() string {
	if s == nil {
		return ""
	}
	return s.sessionID
}

func (s *FuzzySearchSession) Stream(ctx context.Context) (*NotificationStream, error) {
	if s == nil || s.client == nil || s.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := s.ensureActive(); err != nil {
		return nil, err
	}
	stream := s.client.router.subscribe("fuzzyFileSearch", s.sessionID)
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (s *FuzzySearchSession) Update(ctx context.Context, update FuzzySearchUpdate) error {
	if s == nil || s.client == nil {
		return &ClosedError{}
	}
	if err := s.ensureActive(); err != nil {
		return err
	}
	_, err := s.client.Raw().FuzzyFileSearchSessionUpdate(ctx, protocol.FuzzyFileSearchSessionUpdateParams{
		Query:     update.Query,
		SessionID: s.sessionID,
	})
	return err
}

func (s *FuzzySearchSession) Close(ctx context.Context) error {
	if s == nil || s.client == nil {
		return &ClosedError{}
	}
	if err := s.ensureKnown(); err != nil {
		return err
	}
	_, err := s.client.Raw().FuzzyFileSearchSessionStop(ctx, protocol.FuzzyFileSearchSessionStopParams{SessionID: s.sessionID})
	if err != nil {
		return err
	}
	if s.client.FuzzyFileSearch != nil {
		s.client.FuzzyFileSearch.releaseSession(s.sessionID)
	}
	if s.client.router != nil {
		s.client.router.closeKeys([]routerKey{{domain: "fuzzyFileSearch", identity: s.sessionID}}, nil)
	}
	return nil
}

func (s *FuzzySearchSession) ensureActive() error {
	if s == nil || s.client == nil || s.client.FuzzyFileSearch == nil {
		return &ClosedError{}
	}
	if !s.client.FuzzyFileSearch.isSessionActive(s.sessionID) {
		return &ConflictError{Reason: fmt.Sprintf("fuzzy file search session %s is no longer active", s.sessionID)}
	}
	return nil
}

func (s *FuzzySearchSession) ensureKnown() error {
	if s == nil || s.client == nil || s.client.FuzzyFileSearch == nil {
		return &ClosedError{}
	}
	if !s.client.FuzzyFileSearch.isSessionKnown(s.sessionID) {
		return &ConflictError{Reason: fmt.Sprintf("fuzzy file search session %s is no longer active", s.sessionID)}
	}
	return nil
}

func (c *FuzzyFileSearchClient) reserveSession() *FuzzySearchSession {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.sessions == nil {
		c.sessions = map[string]*FuzzySearchSession{}
	}
	c.nextSessionID++
	prefix := c.sessionPrefixLocked()
	session := &FuzzySearchSession{
		client:    c.client,
		sessionID: fmt.Sprintf("%s-%d", prefix, c.nextSessionID),
	}
	c.sessions[session.sessionID] = session
	return session
}

func (c *FuzzyFileSearchClient) sessionPrefixLocked() string {
	if c.sessionPrefix == "" {
		c.sessionPrefix = newFuzzySearchSessionPrefix()
	}
	return c.sessionPrefix
}

func newFuzzySearchSessionPrefix() string {
	var token [12]byte
	if _, err := rand.Read(token[:]); err == nil {
		return "go-fuzzy-file-search-" + hex.EncodeToString(token[:])
	}
	return fmt.Sprintf("go-fuzzy-file-search-fallback-%d", fuzzySearchClientPrefixFallback.Add(1))
}

func (c *FuzzyFileSearchClient) releaseSession(sessionID string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	delete(c.sessions, sessionID)
}

func (c *FuzzyFileSearchClient) closeSessionRoute(sessionID string) {
	if c == nil || sessionID == "" {
		return
	}
	if c.client != nil && c.client.router != nil {
		c.client.router.closeKeys([]routerKey{{domain: "fuzzyFileSearch", identity: sessionID}}, nil)
	}
}

func (c *FuzzyFileSearchClient) isSessionActive(sessionID string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.sessions[sessionID] != nil
}

func (c *FuzzyFileSearchClient) isSessionKnown(sessionID string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.sessions[sessionID] != nil
}

func (c *Client) observeFuzzyFileSearchLifecycle(method string, params json.RawMessage) {
	if c == nil || c.FuzzyFileSearch == nil || method != "fuzzyFileSearch/sessionCompleted" {
		return
	}
	var payload protocol.FuzzyFileSearchSessionCompletedNotification
	if err := json.Unmarshal(params, &payload); err != nil || payload.SessionID == "" {
		return
	}
	c.FuzzyFileSearch.closeSessionRoute(payload.SessionID)
}
