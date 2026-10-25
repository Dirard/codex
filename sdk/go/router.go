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
	overflow       map[routerKey]overflowSentinel
	timers         map[routerKey]*time.Timer
	nextPendingSeq uint64
	closed         bool
	terminalErr    error
}

// overflowSentinel records that a key lost events, with the error its late subscriber
// gets; seq orders sentinels by age.
type overflowSentinel struct {
	err error
	seq uint64
}

type pendingNotification struct {
	seq           uint64
	notification  Notification
	params        *paramIndex
	retainedBytes int64
}

func newNotificationRouter(limits ClientLimits) *notificationRouter {
	return &notificationRouter{
		limits:   limits,
		streams:  map[routerKey]map[*NotificationStream]struct{}{},
		global:   map[*NotificationStream]struct{}{},
		pending:  map[routerKey][]pendingNotification{},
		overflow: map[routerKey]overflowSentinel{},
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
	return r.subscribeKeysWithOnClose(turnScopedRouterKeys(turnID), nil, indexedTurnFilter(threadID, turnID))
}

func (r *notificationRouter) subscribeKeys(keys []routerKey, filter ...func(Notification) bool) *NotificationStream {
	var streamFilter func(Notification, *paramIndex) bool
	if len(filter) > 0 {
		streamFilter = plainFilter(filter[0])
	}
	return r.subscribeKeysWithOnClose(keys, nil, streamFilter)
}

func (r *notificationRouter) subscribeKeysWithOnClose(
	keys []routerKey,
	onClose func(*NotificationStream),
	streamFilter func(Notification, *paramIndex) bool,
) *NotificationStream {
	keys = dedupeRouterKeys(keys)
	var stream *NotificationStream
	stream = newFilteredNotificationStream(
		r.limits.ResourceStreamQueue,
		r.limits.ResourceStreamQueueBytes,
		func() {
			r.unsubscribe(stream, keys)
			if onClose != nil {
				onClose(stream)
			}
		},
		streamFilter,
	)
	for {
		r.mu.Lock()
		if r.closed {
			err := r.terminalErr
			r.mu.Unlock()
			stream.closeWithError(err)
			return stream
		}
		var candidates []pendingNotification
		for _, key := range keys {
			candidates = append(candidates, r.pending[key]...)
		}
		r.mu.Unlock()

		sort.SliceStable(candidates, func(i, j int) bool {
			return candidates[i].seq < candidates[j].seq
		})
		observedSequences := make(map[uint64]struct{}, len(candidates))
		acceptedSequences := make(map[uint64]struct{}, len(candidates))
		for _, candidate := range candidates {
			if _, ok := observedSequences[candidate.seq]; ok {
				continue
			}
			observedSequences[candidate.seq] = struct{}{}
			if stream.accepts(candidate.notification, candidate.params) {
				acceptedSequences[candidate.seq] = struct{}{}
			}
		}

		r.mu.Lock()
		if r.closed {
			err := r.terminalErr
			r.mu.Unlock()
			stream.closeWithError(err)
			return stream
		}
		var pending []pendingNotification
		var overflowErr error
		hasUnobservedPending := false
		for _, key := range keys {
			for _, notification := range r.pending[key] {
				if _, ok := observedSequences[notification.seq]; !ok {
					hasUnobservedPending = true
					continue
				}
				if _, ok := acceptedSequences[notification.seq]; ok {
					pending = append(pending, notification)
				}
			}
			if sentinel, ok := r.overflow[key]; ok && overflowErr == nil {
				overflowErr = sentinel.err
			}
			delete(r.overflow, key)
			if len(r.pending[key]) == 0 {
				if timer := r.timers[key]; timer != nil {
					timer.Stop()
					delete(r.timers, key)
				}
			}
		}
		if len(pending) > 0 {
			claimedSequences := make(map[uint64]struct{}, len(pending))
			for _, notification := range pending {
				claimedSequences[notification.seq] = struct{}{}
			}
			r.dropPendingSequencesLocked(claimedSequences)
		}
		if len(pending) == 0 && overflowErr == nil && !hasUnobservedPending {
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
			// Every replayed notification passed the filter when it was collected.
			notification := pendingNotification.notification
			if !stream.sendAccepted(notification) {
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
			r.routeMalformed(ctx, notification, metadata, &DecodeError{
				Reason: "invalid " + method + " notification payload",
				cause:  err,
			})
			return nil
		}
		notification.Payload = payload
	} else {
		notification.Payload = UnknownNotification{
			Method: method,
			Params: append([]byte(nil), params...),
			Trace:  append([]byte(nil), trace...),
		}
	}
	index := newParamIndex(notification.RawParams)
	keys := routingKeys(metadata, index)
	pendingKeys := pendingRoutingKeys(metadata, index)
	realtimeKeys := realtimeRoutingKeys(method, index)
	keys = append(keys, realtimeKeys...)
	pendingKeys = append(pendingKeys, realtimeKeys...)
	r.deliver(ctx, notification, index, keys, pendingKeys, true)
	return nil
}

// routeMalformed fails only the streams a malformed known notification routes to and
// shows it to global observers, so one bad payload cannot end the whole connection.
func (r *notificationRouter) routeMalformed(
	ctx context.Context,
	notification Notification,
	metadata protocol.ServerNotificationRoutingMetadata,
	decodeErr *DecodeError,
) {
	index := newParamIndex(notification.RawParams)
	keys := routingKeys(metadata, index)
	keys = append(keys, pendingRoutingKeys(metadata, index)...)
	keys = append(keys, realtimeRoutingKeys(notification.Method, index)...)
	keys = dedupeRouterKeys(keys)
	r.closeKeys(keys, decodeErr)
	// A late subscriber would wait forever for a terminal event that never decoded, so it
	// fails the same way. Other keys only lose this one event.
	r.mu.Lock()
	if !r.closed {
		for _, key := range keys {
			if isTerminalNotification(notification.Method, key.domain) {
				r.setPendingSentinelLocked(key, decodeErr)
			}
		}
	}
	r.mu.Unlock()
	notification.Payload = MalformedNotification{
		Method: notification.Method,
		Params: notification.RawParams,
		Trace:  notification.Trace,
		Err:    decodeErr,
	}
	r.deliver(ctx, notification, index, nil, nil, true)
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
	r.overflow = map[routerKey]overflowSentinel{}
	r.timers = map[routerKey]*time.Timer{}
	r.mu.Unlock()

	for stream := range streams {
		stream.closeWithError(err)
	}
}

// deliver hands a notification to the global streams and the streams of its keys, or
// buffers it under its pending keys when no keyed stream takes it. params indexes the
// notification's params for the stream filters.
func (r *notificationRouter) deliver(_ context.Context, notification Notification, params *paramIndex, keys []routerKey, pendingKeys []routerKey, global bool) {
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
			if !stream.accepts(notification, params) {
				continue
			}
			seen[stream] = struct{}{}
			streams = append(streams, stream)
		}
	}
	for _, key := range keys {
		keyStreams := r.streams[key]
		for stream := range keyStreams {
			if !stream.accepts(notification, params) {
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
			params:        params,
			retainedBytes: notificationRetainedBytes(notification),
		}
		for _, key := range dedupeRouterKeys(pendingKeys) {
			r.appendPendingLocked(key, pending)
		}
	}
	r.mu.Unlock()

	for _, stream := range streams {
		stream.sendAccepted(notification)
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

// appendPendingLocked buffers an event for a key nobody subscribed to yet. A full pending
// map evicts its oldest key, which keeps an overflow sentinel, so memory stays bounded
// without closing the router.
func (r *notificationRouter) appendPendingLocked(key routerKey, notification pendingNotification) {
	_, exists := r.pending[key]
	if !exists && len(r.pending) >= r.limits.PendingTurnMap {
		r.evictOldestPendingLocked()
	}
	if notification.retainedBytes <= 0 {
		notification.retainedBytes = notificationRetainedBytes(notification.notification)
	}
	if notification.retainedBytes > r.limits.PendingNotificationBytes-r.pendingBytes {
		r.setPendingOverflowLocked(key, "pending notification byte budget exceeded")
		return
	}
	pending := append(r.pending[key], notification)
	r.pendingBytes += notification.retainedBytes
	if len(pending) > r.limits.PendingTurnQueue {
		r.setPendingOverflowLocked(key, "pending notification queue overflow")
		droppedCount := len(pending) - r.limits.PendingTurnQueue
		for _, dropped := range pending[:droppedCount] {
			r.releasePendingBytesLocked(dropped)
		}
		clear(pending[:droppedCount])
		pending = pending[droppedCount:]
	}
	r.pending[key] = pending
	r.ensurePendingTimerLocked(key)
}

// evictOldestPendingLocked drops the key whose oldest buffered event is the oldest overall.
func (r *notificationRouter) evictOldestPendingLocked() {
	var oldest routerKey
	oldestSeq := uint64(0)
	for key, pending := range r.pending {
		if len(pending) == 0 {
			continue
		}
		if oldestSeq == 0 || pending[0].seq < oldestSeq {
			oldest, oldestSeq = key, pending[0].seq
		}
	}
	if oldestSeq == 0 {
		return
	}
	r.removePendingLocked(oldest)
	r.setPendingOverflowLocked(oldest, "pending notification map overflow")
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

// setPendingOverflowLocked records that key lost events. A full sentinel map forgets its
// oldest sentinel, the same way an inactive key expires.
func (r *notificationRouter) setPendingOverflowLocked(key routerKey, reason string) {
	r.setPendingSentinelLocked(key, &OverflowError{Reason: reason})
}

func (r *notificationRouter) setPendingSentinelLocked(key routerKey, err error) {
	sentinel, exists := r.overflow[key]
	if !exists {
		if len(r.overflow) >= r.limits.PendingTurnMap {
			r.dropOldestOverflowLocked()
		}
		sentinel.seq = r.nextPendingSeq
	}
	sentinel.err = err
	r.overflow[key] = sentinel
	r.ensurePendingTimerLocked(key)
}

func (r *notificationRouter) dropOldestOverflowLocked() {
	var oldest routerKey
	found := false
	for key, sentinel := range r.overflow {
		if !found || sentinel.seq < r.overflow[oldest].seq {
			oldest, found = key, true
		}
	}
	if !found {
		return
	}
	// Forget the key entirely: a truncated buffer without its sentinel would replay a
	// partial stream as if nothing was lost.
	delete(r.overflow, oldest)
	r.removePendingLocked(oldest)
	if timer := r.timers[oldest]; timer != nil {
		timer.Stop()
		delete(r.timers, oldest)
	}
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

func routingKeys(metadata protocol.ServerNotificationRoutingMetadata, params *paramIndex) []routerKey {
	if metadata.Method == "" {
		return nil
	}
	keys := make([]routerKey, 0, len(metadata.Routes))
	seen := map[routerKey]struct{}{}
	for _, route := range metadata.Routes {
		for _, extractor := range route.IdentityExtractors {
			value, ok := params.stringAt(extractor.FieldPath)
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

func pendingRoutingKeys(metadata protocol.ServerNotificationRoutingMetadata, params *paramIndex) []routerKey {
	if metadata.Method == "" {
		return nil
	}
	var keys []routerKey
	for _, route := range metadata.Routes {
		for _, extractor := range route.IdentityExtractors {
			if !claimablePendingIdentity(metadata.Method, route.ResourceDomain, extractor.FieldPath) {
				continue
			}
			value, ok := params.stringAt(extractor.FieldPath)
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

func realtimeRoutingKeys(method string, params *paramIndex) []routerKey {
	if !strings.HasPrefix(method, "thread/realtime/") {
		return nil
	}
	threadID, ok := params.stringAt("threadId")
	if !ok || threadID == "" {
		return nil
	}
	return []routerKey{{domain: realtimeRouterDomain, identity: threadID}}
}

// notificationTurnFilter keeps the notifications of one turn of one thread.
func notificationTurnFilter(threadID string, turnID string) func(Notification) bool {
	filter := indexedTurnFilter(threadID, turnID)
	return func(notification Notification) bool { return filter(notification, nil) }
}

func indexedTurnFilter(threadID string, turnID string) func(Notification, *paramIndex) bool {
	return func(notification Notification, params *paramIndex) bool {
		params = paramIndexFor(params, notification)
		if turnID != "" {
			value, ok := params.stringAt("turnId")
			if !ok {
				value, ok = params.stringAt("turn.id")
			}
			if !ok || value != turnID {
				return false
			}
		}
		if threadID != "" {
			value, ok := params.stringAt("threadId")
			if !ok || value != threadID {
				return false
			}
		}
		return true
	}
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
