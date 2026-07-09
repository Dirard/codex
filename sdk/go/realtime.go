package codex

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/openai/codex/sdk/go/protocol"
)

type RealtimeStartOptions struct {
	ThreadID                   string
	Model                      string
	OutputModality             protocol.RealtimeOutputModality
	Prompt                     string
	Transport                  protocol.ThreadRealtimeStartTransport
	Version                    protocol.RealtimeConversationVersion
	Voice                      protocol.RealtimeVoice
	IncludeStartupContext      *bool
	ClientManagedHandoffs      *bool
	CodexResponsesAsItems      *bool
	CodexResponseHandoffPrefix string
	CodexResponseItemPrefix    string
}

type RealtimeSession struct {
	client   *Client
	threadID string
	id       string
	stopping bool
	streams  map[*NotificationStream]struct{}
}

type RealtimeStream = NotificationStream
type AudioChunk = protocol.ThreadRealtimeAudioChunk

type SpeechInput struct {
	Text string
}

func (c *RealtimeClient) Start(ctx context.Context, opts RealtimeStartOptions) (*RealtimeSession, protocol.ThreadRealtimeStartResponse, error) {
	if c == nil || c.client == nil {
		return nil, nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("realtime start", "thread/realtime/start"); err != nil {
		return nil, nil, err
	}
	if opts.ThreadID == "" {
		return nil, nil, &ConfigError{Reason: "realtime start requires ThreadID"}
	}
	session, err := c.reserveSession(opts.ThreadID)
	if err != nil {
		return nil, nil, err
	}
	c.dropPendingRealtimeNotifications(opts.ThreadID)
	response, err := c.client.Raw().ThreadRealtimeStart(ctx, realtimeStartParams(opts, session.id))
	if err != nil {
		c.releaseSession(opts.ThreadID, session.id)
		return nil, response, err
	}
	return session, response, nil
}

func (c *RealtimeClient) ListVoices(ctx context.Context, params protocol.ThreadRealtimeListVoicesParams) (protocol.ThreadRealtimeListVoicesResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadRealtimeListVoicesResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadRealtimeListVoices(ctx, params)
}

func (s *RealtimeSession) ID() string {
	if s == nil {
		return ""
	}
	return s.id
}

func (s *RealtimeSession) ThreadID() string {
	if s == nil {
		return ""
	}
	return s.threadID
}

func (s *RealtimeSession) Stream(ctx context.Context) (*RealtimeStream, error) {
	if s == nil || s.client == nil || s.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := s.client.ensureHighLevelEnabled("realtime stream"); err != nil {
		return nil, err
	}
	if err := s.ensureActive(); err != nil {
		return nil, err
	}
	stream := s.client.router.subscribeKeys(
		[]routerKey{{domain: realtimeRouterDomain, identity: s.threadID}},
		realtimeThreadFilter(s.threadID),
	)
	onClose := stream.onClose
	stream.onClose = func() {
		if onClose != nil {
			onClose()
		}
		if s.client.Realtime != nil {
			s.client.Realtime.unregisterSessionStream(s.threadID, s.id, stream)
		}
	}
	if err := s.client.Realtime.registerSessionStream(s.threadID, s.id, stream); err != nil {
		_ = stream.Close()
		return nil, err
	}
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (s *RealtimeSession) AppendAudio(ctx context.Context, audio AudioChunk) error {
	if s == nil || s.client == nil {
		return &ClosedError{}
	}
	if err := s.client.ensureHighLevelEnabled("realtime append audio"); err != nil {
		return err
	}
	if err := s.ensureActive(); err != nil {
		return err
	}
	_, err := s.client.Raw().ThreadRealtimeAppendAudio(ctx, protocol.ThreadRealtimeAppendAudioParams{
		ThreadID: s.threadID,
		Audio:    audio,
	})
	return err
}

func (s *RealtimeSession) AppendText(ctx context.Context, text string) error {
	if s == nil || s.client == nil {
		return &ClosedError{}
	}
	if err := s.client.ensureHighLevelEnabled("realtime append text"); err != nil {
		return err
	}
	if err := s.ensureActive(); err != nil {
		return err
	}
	_, err := s.client.Raw().ThreadRealtimeAppendText(ctx, protocol.ThreadRealtimeAppendTextParams{
		ThreadID: s.threadID,
		Text:     text,
	})
	return err
}

func (s *RealtimeSession) AppendSpeech(ctx context.Context, input SpeechInput) error {
	if s == nil || s.client == nil {
		return &ClosedError{}
	}
	if err := s.client.ensureHighLevelEnabled("realtime append speech"); err != nil {
		return err
	}
	if err := s.ensureActive(); err != nil {
		return err
	}
	_, err := s.client.Raw().ThreadRealtimeAppendSpeech(ctx, protocol.ThreadRealtimeAppendSpeechParams{
		ThreadID: s.threadID,
		Text:     input.Text,
	})
	return err
}

func (s *RealtimeSession) Stop(ctx context.Context) error {
	if s == nil || s.client == nil {
		return &ClosedError{}
	}
	if err := s.client.ensureHighLevelEnabled("realtime stop"); err != nil {
		return err
	}
	if err := s.ensureActive(); err != nil {
		return err
	}
	_, err := s.client.Raw().ThreadRealtimeStop(ctx, protocol.ThreadRealtimeStopParams{ThreadID: s.threadID})
	if err != nil {
		return err
	}
	if s.client.Realtime != nil {
		closeRealtimeStreams(s.client.Realtime.markSessionStopping(s.threadID, s.id))
	}
	return nil
}

func (s *RealtimeSession) ensureActive() error {
	if s == nil || s.client == nil || s.client.Realtime == nil {
		return &ClosedError{}
	}
	if !s.client.Realtime.isSessionActive(s.threadID, s.id) {
		return &ConflictError{Reason: fmt.Sprintf("realtime session %s is no longer active for thread %s", s.id, s.threadID)}
	}
	return nil
}

func (c *RealtimeClient) reserveSession(threadID string) (*RealtimeSession, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.activeByThread == nil {
		c.activeByThread = map[string]*RealtimeSession{}
	}
	if session := c.activeByThread[threadID]; session != nil {
		return nil, &ConflictError{Reason: fmt.Sprintf("realtime session %s is already active for thread %s", session.id, threadID)}
	}
	c.nextSessionID++
	session := &RealtimeSession{
		client:   c.client,
		threadID: threadID,
		id:       fmt.Sprintf("go-realtime-%d", c.nextSessionID),
	}
	c.activeByThread[threadID] = session
	return session, nil
}

func (c *RealtimeClient) releaseSession(threadID string, sessionID string) {
	c.mu.Lock()
	var streams []*NotificationStream
	if session := c.activeByThread[threadID]; session != nil && session.id == sessionID {
		delete(c.activeByThread, threadID)
		streams = drainRealtimeStreams(session)
	}
	c.mu.Unlock()
	closeRealtimeStreams(streams)
}

func (c *RealtimeClient) markSessionStopping(threadID string, sessionID string) []*NotificationStream {
	c.mu.Lock()
	defer c.mu.Unlock()
	if session := c.activeByThread[threadID]; session != nil && session.id == sessionID {
		session.stopping = true
		return drainRealtimeStreams(session)
	}
	return nil
}

func (c *RealtimeClient) isSessionActive(threadID string, sessionID string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	session := c.activeByThread[threadID]
	return session != nil && session.id == sessionID && !session.stopping
}

func (c *RealtimeClient) releaseThreadSession(threadID string) {
	c.mu.Lock()
	var streams []*NotificationStream
	if session := c.activeByThread[threadID]; session != nil {
		streams = drainRealtimeStreams(session)
	}
	delete(c.activeByThread, threadID)
	c.mu.Unlock()
	closeRealtimeStreams(streams)
}

func (c *RealtimeClient) releaseThreadSessionFromClosed(threadID string) {
	c.mu.Lock()
	var streams []*NotificationStream
	if session := c.activeByThread[threadID]; session != nil {
		streams = drainRealtimeStreams(session)
	}
	delete(c.activeByThread, threadID)
	c.mu.Unlock()
	closeRealtimeStreams(streams)
}

func (c *RealtimeClient) registerSessionStream(threadID string, sessionID string, stream *NotificationStream) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	session := c.activeByThread[threadID]
	if session == nil || session.id != sessionID || session.stopping {
		return &ConflictError{Reason: fmt.Sprintf("realtime session %s is no longer active for thread %s", sessionID, threadID)}
	}
	if session.streams == nil {
		session.streams = map[*NotificationStream]struct{}{}
	}
	session.streams[stream] = struct{}{}
	return nil
}

func (c *RealtimeClient) unregisterSessionStream(threadID string, sessionID string, stream *NotificationStream) {
	c.mu.Lock()
	defer c.mu.Unlock()
	session := c.activeByThread[threadID]
	if session == nil || session.id != sessionID || session.streams == nil {
		return
	}
	delete(session.streams, stream)
	if len(session.streams) == 0 {
		session.streams = nil
	}
}

func (c *RealtimeClient) dropPendingRealtimeNotifications(threadID string) {
	if c == nil || c.client == nil || c.client.router == nil {
		return
	}
	c.client.router.dropPendingNotifications(routerKey{domain: realtimeRouterDomain, identity: threadID}, func(notification Notification) bool {
		return strings.HasPrefix(notification.Method, "thread/realtime/")
	})
}

func drainRealtimeStreams(session *RealtimeSession) []*NotificationStream {
	if session == nil || len(session.streams) == 0 {
		return nil
	}
	streams := make([]*NotificationStream, 0, len(session.streams))
	for stream := range session.streams {
		streams = append(streams, stream)
	}
	session.streams = nil
	return streams
}

func closeRealtimeStreams(streams []*NotificationStream) {
	for _, stream := range streams {
		_ = stream.Close()
	}
}

func (c *Client) observeRealtimeLifecycle(method string, params json.RawMessage) {
	threadID, ok := realtimeClosedThreadID(method, params)
	if c == nil || c.Realtime == nil || !ok {
		return
	}
	c.Realtime.releaseThreadSessionFromClosed(threadID)
}

func realtimeClosedThreadID(method string, params json.RawMessage) (string, bool) {
	if method != "thread/realtime/closed" {
		return "", false
	}
	var payload protocol.ThreadRealtimeClosedNotification
	if err := json.Unmarshal(params, &payload); err != nil || payload.ThreadID == "" {
		return "", false
	}
	return payload.ThreadID, true
}

func realtimeStartParams(opts RealtimeStartOptions, sessionID string) protocol.ThreadRealtimeStartParams {
	outputModality := opts.OutputModality
	if outputModality == "" {
		outputModality = protocol.RealtimeOutputModalityAudio
	}
	params := protocol.ThreadRealtimeStartParams{
		OutputModality:    outputModality,
		RealtimeSessionID: protocol.Some(sessionID),
		ThreadID:          opts.ThreadID,
	}
	if opts.Model != "" {
		params.Model = protocol.Some(opts.Model)
	}
	if opts.Prompt != "" {
		params.Prompt = protocol.Some(opts.Prompt)
	}
	if opts.Transport.TypeValue != "" || opts.Transport.Sdp.IsSet() || len(opts.Transport.RawJSON) > 0 {
		params.Transport = protocol.Some(opts.Transport)
	}
	if opts.Version != "" {
		params.Version = protocol.Some(opts.Version)
	}
	if opts.Voice != "" {
		params.Voice = protocol.Some(opts.Voice)
	}
	if opts.IncludeStartupContext != nil {
		params.IncludeStartupContext = protocol.Some(*opts.IncludeStartupContext)
	}
	if opts.ClientManagedHandoffs != nil {
		params.ClientManagedHandoffs = protocol.Some(*opts.ClientManagedHandoffs)
	}
	if opts.CodexResponsesAsItems != nil {
		params.CodexResponsesAsItems = protocol.Some(*opts.CodexResponsesAsItems)
	}
	if opts.CodexResponseHandoffPrefix != "" {
		params.CodexResponseHandoffPrefix = protocol.Some(opts.CodexResponseHandoffPrefix)
	}
	if opts.CodexResponseItemPrefix != "" {
		params.CodexResponseItemPrefix = protocol.Some(opts.CodexResponseItemPrefix)
	}
	return params
}

func realtimeThreadFilter(threadID string) func(Notification) bool {
	return func(notification Notification) bool {
		if !strings.HasPrefix(notification.Method, "thread/realtime/") {
			return false
		}
		gotThreadID, ok := realtimeNotificationThreadID(notification.Payload)
		return ok && gotThreadID == threadID
	}
}

func realtimeNotificationThreadID(payload any) (string, bool) {
	switch p := payload.(type) {
	case protocol.ThreadRealtimeClosedNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeErrorNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeItemAddedNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeOutputAudioDeltaNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeSdpNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeStartedNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeTranscriptDeltaNotification:
		return p.ThreadID, true
	case protocol.ThreadRealtimeTranscriptDoneNotification:
		return p.ThreadID, true
	default:
		return "", false
	}
}
