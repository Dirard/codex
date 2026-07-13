package codex

import (
	"context"
	"encoding/json"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

const realtimeRouterDomain = "realtime"

type routerKey struct {
	domain   string
	identity string
}

type notificationRouter struct {
	mu             sync.Mutex
	limits         ClientLimits
	streams        map[routerKey]map[*NotificationStream]struct{}
	global         map[*NotificationStream]struct{}
	pending        map[routerKey][]pendingNotification
	pendingBytes   int64
	overflow       map[routerKey]*OverflowError
	timers         map[routerKey]*time.Timer
	nextPendingSeq uint64
	closed         bool
	terminalErr    error
}

type pendingNotification struct {
	seq           uint64
	notification  Notification
	retainedBytes int64
}

func newNotificationRouter(limits ClientLimits) *notificationRouter {
	return &notificationRouter{
		limits:   limits,
		streams:  map[routerKey]map[*NotificationStream]struct{}{},
		global:   map[*NotificationStream]struct{}{},
		pending:  map[routerKey][]pendingNotification{},
		overflow: map[routerKey]*OverflowError{},
		timers:   map[routerKey]*time.Timer{},
	}
}

func (r *notificationRouter) subscribe(domain string, identities ...string) *NotificationStream {
	if len(identities) == 0 {
		identities = []string{""}
	}
	keys := make([]routerKey, 0, len(identities))
	for _, identity := range identities {
		keys = append(keys, routerKey{domain: domain, identity: identity})
	}
	return r.subscribeKeys(keys)
}

func (r *notificationRouter) subscribeTurn(threadID string, turnID string) *NotificationStream {
	return r.subscribeKeys(turnScopedRouterKeys(turnID), notificationTurnFilter(threadID, turnID))
}

func (r *notificationRouter) subscribeKeys(keys []routerKey, filter ...func(Notification) bool) *NotificationStream {
	keys = dedupeRouterKeys(keys)
	var streamFilter func(Notification) bool
	if len(filter) > 0 {
		streamFilter = filter[0]
	}
	stream := newFilteredNotificationStream(
		r.limits.ResourceStreamQueue,
		r.limits.ResourceStreamQueueBytes,
		nil,
		streamFilter,
	)
	stream.onClose = func() { r.unsubscribe(stream, keys) }
	for {
		r.mu.Lock()
		if r.closed {
			err := r.terminalErr
			r.mu.Unlock()
			stream.closeWithError(err)
			return stream
		}
		var pending []pendingNotification
		var overflowErr error
		for _, key := range keys {
			if timer := r.timers[key]; timer != nil {
				timer.Stop()
				delete(r.timers, key)
			}
			pending = append(pending, r.removePendingLocked(key)...)
			if err := r.overflow[key]; err != nil && overflowErr == nil {
				overflowErr = err
			}
			delete(r.overflow, key)
		}
		if len(pending) > 0 {
			claimedSequences := make(map[uint64]struct{}, len(pending))
			for _, notification := range pending {
				claimedSequences[notification.seq] = struct{}{}
			}
			r.dropPendingSequencesLocked(claimedSequences)
		}
		if len(pending) == 0 && overflowErr == nil {
			for _, key := range keys {
				if r.streams[key] == nil {
					r.streams[key] = map[*NotificationStream]struct{}{}
				}
				r.streams[key][stream] = struct{}{}
			}
			r.mu.Unlock()
			return stream
		}
		r.mu.Unlock()
		sort.SliceStable(pending, func(i, j int) bool {
			return pending[i].seq < pending[j].seq
		})
		terminal := false
		seenPending := map[uint64]struct{}{}
		for _, pendingNotification := range pending {
			if _, ok := seenPending[pendingNotification.seq]; ok {
				continue
			}
			seenPending[pendingNotification.seq] = struct{}{}
			notification := pendingNotification.notification
			if !stream.accepts(notification) {
				continue
			}
			if !stream.send(notification) {
				return stream
			}
			if isTerminalForAnyKey(notification, keys) {
				terminal = true
			}
		}
		if overflowErr != nil {
			stream.closeWithError(overflowErr)
			return stream
		}
		if terminal {
			stream.closeWithError(nil)
			return stream
		}
	}
}

func dedupeRouterKeys(keys []routerKey) []routerKey {
	seen := map[routerKey]struct{}{}
	out := make([]routerKey, 0, len(keys))
	for _, key := range keys {
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, key)
	}
	return out
}

func turnScopedRouterKeys(turnID string) []routerKey {
	domains := map[string]struct{}{}
	for _, metadata := range protocol.ServerNotificationRoutingByMethod {
		for _, route := range metadata.Routes {
			if routeHasTurnIdentity(route) {
				domains[route.ResourceDomain] = struct{}{}
			}
		}
	}
	ordered := make([]string, 0, len(domains))
	for domain := range domains {
		ordered = append(ordered, domain)
	}
	sort.Strings(ordered)
	keys := make([]routerKey, 0, len(ordered))
	for _, domain := range ordered {
		keys = append(keys, routerKey{domain: domain, identity: turnID})
	}
	return keys
}

func routeHasTurnIdentity(route protocol.ServerNotificationRouteMetadata) bool {
	for _, extractor := range route.IdentityExtractors {
		if extractor.FieldPath == "turnId" || extractor.FieldPath == "turn.id" {
			return true
		}
	}
	return false
}

func (r *notificationRouter) subscribeGlobal() *NotificationStream {
	stream := newNotificationStream(
		r.limits.GlobalSubscriberQueue,
		r.limits.GlobalSubscriberQueueBytes,
		nil,
	)
	stream.onClose = func() { r.unsubscribeGlobal(stream) }
	r.mu.Lock()
	if r.closed {
		err := r.terminalErr
		r.mu.Unlock()
		stream.closeWithError(err)
		return stream
	}
	r.global[stream] = struct{}{}
	r.mu.Unlock()
	return stream
}

func (r *notificationRouter) route(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) error {
	notification := Notification{
		Method:    method,
		RawParams: append([]byte(nil), params...),
		Trace:     append([]byte(nil), trace...),
	}
	metadata, known := protocol.ServerNotificationRoutingByMethod[method]
	if known {
		payload, err := decodeKnownNotification(method, params)
		if err != nil {
			decodeErr := &DecodeError{
				Reason: "invalid " + method + " notification payload",
				cause:  err,
			}
			r.closeWithError(decodeErr)
			return decodeErr
		}
		notification.Payload = payload
	} else {
		notification.Payload = UnknownNotification{
			Method: method,
			Params: append([]byte(nil), params...),
			Trace:  append([]byte(nil), trace...),
		}
	}
	keys := routingKeys(metadata, params)
	pendingKeys := pendingRoutingKeys(metadata, params)
	realtimeKeys := realtimeRoutingKeys(method, params)
	keys = append(keys, realtimeKeys...)
	pendingKeys = append(pendingKeys, realtimeKeys...)
	r.deliver(ctx, notification, keys, pendingKeys, true)
	return nil
}

func (r *notificationRouter) close() {
	r.closeWithError(&ClosedError{})
}

func (r *notificationRouter) closeWithError(err error) {
	if err == nil {
		err = &ClosedError{}
	}
	r.mu.Lock()
	if r.closed {
		r.mu.Unlock()
		return
	}
	streams := make(map[*NotificationStream]struct{})
	for _, byStream := range r.streams {
		for stream := range byStream {
			streams[stream] = struct{}{}
		}
	}
	for stream := range r.global {
		streams[stream] = struct{}{}
	}
	for _, timer := range r.timers {
		timer.Stop()
	}
	r.closed = true
	r.terminalErr = err
	r.streams = map[routerKey]map[*NotificationStream]struct{}{}
	r.global = map[*NotificationStream]struct{}{}
	r.pending = map[routerKey][]pendingNotification{}
	r.pendingBytes = 0
	r.overflow = map[routerKey]*OverflowError{}
	r.timers = map[routerKey]*time.Timer{}
	r.mu.Unlock()

	for stream := range streams {
		stream.closeWithError(err)
	}
}

func (r *notificationRouter) deliver(_ context.Context, notification Notification, keys []routerKey, pendingKeys []routerKey, global bool) {
	r.mu.Lock()
	if r.closed {
		r.mu.Unlock()
		return
	}
	var streams []*NotificationStream
	terminalStreams := map[*NotificationStream]struct{}{}
	seen := map[*NotificationStream]struct{}{}
	deliveredByRoute := false
	if global {
		for stream := range r.global {
			if !stream.accepts(notification) {
				continue
			}
			seen[stream] = struct{}{}
			streams = append(streams, stream)
		}
	}
	for _, key := range keys {
		keyStreams := r.streams[key]
		for stream := range keyStreams {
			if !stream.accepts(notification) {
				continue
			}
			deliveredByRoute = true
			if isTerminalNotification(methodForNotification(notification), key.domain) {
				terminalStreams[stream] = struct{}{}
			}
			if _, ok := seen[stream]; ok {
				continue
			}
			seen[stream] = struct{}{}
			streams = append(streams, stream)
		}
	}
	if !deliveredByRoute {
		r.nextPendingSeq++
		pending := pendingNotification{
			seq:           r.nextPendingSeq,
			notification:  notification,
			retainedBytes: notificationRetainedBytes(notification),
		}
		for _, key := range dedupeRouterKeys(pendingKeys) {
			if len(r.streams[key]) == 0 {
				if err := r.appendPendingLocked(key, pending); err != nil {
					r.mu.Unlock()
					r.closeWithError(err)
					return
				}
			}
		}
	}
	r.mu.Unlock()

	for _, stream := range streams {
		stream.send(notification)
	}
	for stream := range terminalStreams {
		stream.closeWithError(nil)
	}
}

func (r *notificationRouter) dropPendingSequencesLocked(sequences map[uint64]struct{}) {
	for key, pending := range r.pending {
		kept := pending[:0]
		for _, notification := range pending {
			if _, claimed := sequences[notification.seq]; claimed {
				r.releasePendingBytesLocked(notification)
				continue
			}
			kept = append(kept, notification)
		}
		clear(pending[len(kept):])
		if len(kept) > 0 {
			r.pending[key] = kept
			continue
		}
		delete(r.pending, key)
		delete(r.overflow, key)
		if timer := r.timers[key]; timer != nil {
			timer.Stop()
			delete(r.timers, key)
		}
	}
}

func (r *notificationRouter) appendPendingLocked(key routerKey, notification pendingNotification) error {
	_, exists := r.pending[key]
	if !exists && len(r.pending) >= r.limits.PendingTurnMap {
		return r.setPendingOverflowLocked(key, "pending notification map overflow")
	}
	if notification.retainedBytes <= 0 {
		notification.retainedBytes = notificationRetainedBytes(notification.notification)
	}
	if notification.retainedBytes > r.limits.PendingNotificationBytes-r.pendingBytes {
		return r.setPendingOverflowLocked(key, "pending notification byte budget exceeded")
	}
	pending := append(r.pending[key], notification)
	r.pendingBytes += notification.retainedBytes
	if len(pending) > r.limits.PendingTurnQueue {
		if err := r.setPendingOverflowLocked(key, "pending notification queue overflow"); err != nil {
			return err
		}
		droppedCount := len(pending) - r.limits.PendingTurnQueue
		for _, dropped := range pending[:droppedCount] {
			r.releasePendingBytesLocked(dropped)
		}
		clear(pending[:droppedCount])
		pending = pending[droppedCount:]
	}
	r.pending[key] = pending
	r.ensurePendingTimerLocked(key)
	return nil
}

func (r *notificationRouter) dropPendingNotifications(key routerKey, drop func(Notification) bool) {
	if r == nil || drop == nil {
		return
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	pending := r.pending[key]
	if len(pending) == 0 {
		delete(r.overflow, key)
		if timer := r.timers[key]; timer != nil {
			timer.Stop()
			delete(r.timers, key)
		}
		return
	}
	kept := pending[:0]
	for _, notification := range pending {
		if drop(notification.notification) {
			r.releasePendingBytesLocked(notification)
			continue
		}
		kept = append(kept, notification)
	}
	clear(pending[len(kept):])
	if len(kept) > 0 {
		r.pending[key] = kept
		return
	}
	delete(r.pending, key)
	delete(r.overflow, key)
	if timer := r.timers[key]; timer != nil {
		timer.Stop()
		delete(r.timers, key)
	}
}

func (r *notificationRouter) ensurePendingTimerLocked(key routerKey) {
	if r.timers[key] != nil {
		return
	}
	timeout := r.limits.LifecycleInactivityTimeout
	r.timers[key] = time.AfterFunc(timeout, func() {
		r.mu.Lock()
		r.removePendingLocked(key)
		delete(r.overflow, key)
		delete(r.timers, key)
		r.mu.Unlock()
	})
}

func (r *notificationRouter) setPendingOverflowLocked(key routerKey, reason string) error {
	if _, exists := r.overflow[key]; !exists && len(r.overflow) >= r.limits.PendingTurnMap {
		return &OverflowError{Reason: "pending overflow sentinel capacity exceeded"}
	}
	r.overflow[key] = &OverflowError{Reason: reason}
	r.ensurePendingTimerLocked(key)
	return nil
}

func (r *notificationRouter) closeKeys(keys []routerKey, err error) {
	r.mu.Lock()
	streams := make(map[*NotificationStream]struct{})
	for _, key := range keys {
		for stream := range r.streams[key] {
			streams[stream] = struct{}{}
		}
		delete(r.streams, key)
		r.removePendingLocked(key)
		delete(r.overflow, key)
		if timer := r.timers[key]; timer != nil {
			timer.Stop()
			delete(r.timers, key)
		}
	}
	r.mu.Unlock()
	for stream := range streams {
		stream.closeWithError(err)
	}
}

func (r *notificationRouter) removePendingLocked(key routerKey) []pendingNotification {
	pending := r.pending[key]
	for _, notification := range pending {
		r.releasePendingBytesLocked(notification)
	}
	delete(r.pending, key)
	return pending
}

func (r *notificationRouter) releasePendingBytesLocked(notification pendingNotification) {
	r.pendingBytes -= notification.retainedBytes
}

func (r *notificationRouter) unsubscribe(stream *NotificationStream, keys []routerKey) {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, key := range keys {
		delete(r.streams[key], stream)
		if len(r.streams[key]) == 0 {
			delete(r.streams, key)
		}
	}
}

func (r *notificationRouter) unsubscribeGlobal(stream *NotificationStream) {
	r.mu.Lock()
	defer r.mu.Unlock()
	delete(r.global, stream)
}

func routingKeys(metadata protocol.ServerNotificationRoutingMetadata, params json.RawMessage) []routerKey {
	if metadata.Method == "" {
		return nil
	}
	var raw map[string]any
	if len(params) > 0 {
		_ = json.Unmarshal(params, &raw)
	}
	keys := make([]routerKey, 0, len(metadata.Routes))
	seen := map[routerKey]struct{}{}
	for _, route := range metadata.Routes {
		for _, extractor := range route.IdentityExtractors {
			value, ok := stringAtJSONPath(raw, extractor.FieldPath)
			if !ok {
				value = ""
			}
			key := routerKey{domain: route.ResourceDomain, identity: value}
			if _, exists := seen[key]; exists {
				continue
			}
			seen[key] = struct{}{}
			keys = append(keys, key)
		}
	}
	return keys
}

func pendingRoutingKeys(metadata protocol.ServerNotificationRoutingMetadata, params json.RawMessage) []routerKey {
	if metadata.Method == "" {
		return nil
	}
	var raw map[string]any
	if len(params) > 0 {
		_ = json.Unmarshal(params, &raw)
	}
	var keys []routerKey
	for _, route := range metadata.Routes {
		for _, extractor := range route.IdentityExtractors {
			if !claimablePendingIdentity(metadata.Method, route.ResourceDomain, extractor.FieldPath) {
				continue
			}
			value, ok := stringAtJSONPath(raw, extractor.FieldPath)
			if !ok {
				if route.ResourceDomain == "account" && extractor.FieldPath == "loginId" && extractor.Optional {
					value = ""
				} else {
					continue
				}
			}
			if value == "" && route.ResourceDomain != "account" {
				continue
			}
			keys = append(keys, routerKey{domain: route.ResourceDomain, identity: value})
		}
	}
	return dedupeRouterKeys(keys)
}

func claimablePendingIdentity(method string, domain string, fieldPath string) bool {
	if fieldPath == "turnId" || fieldPath == "turn.id" {
		return true
	}
	switch domain {
	case "command", "fs", "fuzzyFileSearch", "process":
		return true
	case "account":
		return method == "account/login/completed" && fieldPath == "loginId"
	case "mcpServer":
		return method == "mcpServer/oauthLogin/completed" && fieldPath == "name"
	default:
		return false
	}
}

func realtimeRoutingKeys(method string, params json.RawMessage) []routerKey {
	if !strings.HasPrefix(method, "thread/realtime/") {
		return nil
	}
	var raw map[string]any
	if len(params) > 0 {
		_ = json.Unmarshal(params, &raw)
	}
	threadID, ok := stringAtJSONPath(raw, "threadId")
	if !ok || threadID == "" {
		return nil
	}
	return []routerKey{{domain: realtimeRouterDomain, identity: threadID}}
}

func notificationTurnFilter(threadID string, turnID string) func(Notification) bool {
	return func(notification Notification) bool {
		var raw map[string]any
		if len(notification.RawParams) > 0 {
			_ = json.Unmarshal(notification.RawParams, &raw)
		}
		if turnID != "" {
			value, ok := stringAtJSONPath(raw, "turnId")
			if !ok {
				value, ok = stringAtJSONPath(raw, "turn.id")
			}
			if !ok || value != turnID {
				return false
			}
		}
		if threadID != "" {
			value, ok := stringAtJSONPath(raw, "threadId")
			if !ok || value != threadID {
				return false
			}
		}
		return true
	}
}

func stringAtJSONPath(raw map[string]any, path string) (string, bool) {
	if len(raw) == 0 || path == "" {
		return "", false
	}
	var current any = raw
	for _, part := range splitPath(path) {
		object, ok := current.(map[string]any)
		if !ok {
			return "", false
		}
		current, ok = object[part]
		if !ok {
			return "", false
		}
	}
	value, ok := current.(string)
	return value, ok
}

func splitPath(path string) []string {
	var parts []string
	start := 0
	for i := 0; i < len(path); i++ {
		if path[i] == '.' {
			parts = append(parts, path[start:i])
			start = i + 1
		}
	}
	parts = append(parts, path[start:])
	return parts
}

func isTerminalNotification(method string, domain string) bool {
	for _, lifecycle := range protocol.RoutingLifecycleByStartMethod {
		for _, trigger := range lifecycle.CleanupTriggers {
			if trigger.Kind == "terminalNotification" && trigger.Method == method && terminalRoutesThroughDomain(lifecycle, method, domain) {
				return true
			}
		}
	}
	return false
}

func terminalRoutesThroughDomain(lifecycle protocol.RoutingLifecycleMetadata, method string, domain string) bool {
	if lifecycle.ResourceDomain == domain {
		return true
	}
	if domain == realtimeRouterDomain && strings.HasPrefix(method, "thread/realtime/") {
		return true
	}
	metadata, ok := protocol.ServerNotificationRoutingByMethod[method]
	if !ok {
		return false
	}
	for _, route := range metadata.Routes {
		if route.ResourceDomain == domain {
			return true
		}
	}
	return false
}

func isTerminalForAnyKey(notification Notification, keys []routerKey) bool {
	method := methodForNotification(notification)
	for _, key := range keys {
		if isTerminalNotification(method, key.domain) {
			return true
		}
	}
	return false
}

func methodForNotification(notification Notification) string {
	return notification.Method
}

func decodeKnownNotification(method string, params json.RawMessage) (any, error) {
	return protocol.DecodeServerNotificationPayload(method, params)
}
