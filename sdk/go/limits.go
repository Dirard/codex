package codex

import (
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

const (
	DefaultMaxFrameBytes                  int64 = 16 * 1024 * 1024
	DefaultMaxLocalInputBytes             int64 = 16 * 1024 * 1024
	DefaultMaxAdditionalContextEntries          = protocol.MaxAdditionalContextEntries
	DefaultMaxAdditionalContextKeyBytes   int64 = protocol.MaxAdditionalContextKeyBytes
	DefaultMaxAdditionalContextValueBytes int64 = protocol.MaxAdditionalContextValueBytes
	DefaultMaxAdditionalContextTotalBytes int64 = protocol.MaxAdditionalContextTotalBytes
	DefaultResourceStreamQueue                  = 256
	DefaultPendingTurnQueue                     = 512
	DefaultPendingTurnMap                       = 128
	DefaultGlobalSubscriberQueue                = 512
	DefaultHandlerConcurrency                   = 16
	DefaultHandlerQueue                         = 256
	DefaultStderrRingBytes                      = 64 * 1024
)

const (
	DefaultHandlerTimeout             = 60 * time.Second
	DefaultLifecycleInactivityTimeout = 5 * time.Minute
)

func normalizeLimits(l ClientLimits) (ClientLimits, error) {
	if l.MaxFrameBytes < 0 ||
		l.MaxLocalInputBytes < 0 ||
		l.MaxAdditionalContextEntries < 0 ||
		l.MaxAdditionalContextKeyBytes < 0 ||
		l.MaxAdditionalContextValueBytes < 0 ||
		l.MaxAdditionalContextTotalBytes < 0 ||
		l.ResourceStreamQueue < 0 ||
		l.PendingTurnQueue < 0 ||
		l.PendingTurnMap < 0 ||
		l.GlobalSubscriberQueue < 0 ||
		l.HandlerConcurrency < 0 ||
		l.HandlerQueue < 0 ||
		l.HandlerTimeout < 0 ||
		l.StderrRingBytes < 0 ||
		l.LifecycleInactivityTimeout < 0 {
		return ClientLimits{}, &ConfigError{Reason: "limits must be zero for defaults or positive overrides"}
	}
	if l.MaxFrameBytes == 0 {
		l.MaxFrameBytes = DefaultMaxFrameBytes
	}
	if l.MaxLocalInputBytes == 0 {
		l.MaxLocalInputBytes = DefaultMaxLocalInputBytes
	}
	if l.MaxAdditionalContextEntries == 0 {
		l.MaxAdditionalContextEntries = DefaultMaxAdditionalContextEntries
	}
	if l.MaxAdditionalContextKeyBytes == 0 {
		l.MaxAdditionalContextKeyBytes = DefaultMaxAdditionalContextKeyBytes
	}
	if l.MaxAdditionalContextValueBytes == 0 {
		l.MaxAdditionalContextValueBytes = DefaultMaxAdditionalContextValueBytes
	}
	if l.MaxAdditionalContextTotalBytes == 0 {
		l.MaxAdditionalContextTotalBytes = DefaultMaxAdditionalContextTotalBytes
	}
	if l.MaxAdditionalContextEntries > DefaultMaxAdditionalContextEntries {
		l.MaxAdditionalContextEntries = DefaultMaxAdditionalContextEntries
	}
	if l.MaxAdditionalContextKeyBytes > DefaultMaxAdditionalContextKeyBytes {
		l.MaxAdditionalContextKeyBytes = DefaultMaxAdditionalContextKeyBytes
	}
	if l.MaxAdditionalContextValueBytes > DefaultMaxAdditionalContextValueBytes {
		l.MaxAdditionalContextValueBytes = DefaultMaxAdditionalContextValueBytes
	}
	if l.MaxAdditionalContextTotalBytes > DefaultMaxAdditionalContextTotalBytes {
		l.MaxAdditionalContextTotalBytes = DefaultMaxAdditionalContextTotalBytes
	}
	if l.ResourceStreamQueue == 0 {
		l.ResourceStreamQueue = DefaultResourceStreamQueue
	}
	if l.PendingTurnQueue == 0 {
		l.PendingTurnQueue = DefaultPendingTurnQueue
	}
	if l.PendingTurnMap == 0 {
		l.PendingTurnMap = DefaultPendingTurnMap
	}
	if l.GlobalSubscriberQueue == 0 {
		l.GlobalSubscriberQueue = DefaultGlobalSubscriberQueue
	}
	if l.HandlerConcurrency == 0 {
		l.HandlerConcurrency = DefaultHandlerConcurrency
	}
	if l.HandlerQueue == 0 {
		l.HandlerQueue = DefaultHandlerQueue
	}
	if l.HandlerTimeout == 0 {
		l.HandlerTimeout = DefaultHandlerTimeout
	}
	if l.StderrRingBytes == 0 {
		l.StderrRingBytes = DefaultStderrRingBytes
	}
	if l.LifecycleInactivityTimeout == 0 {
		l.LifecycleInactivityTimeout = DefaultLifecycleInactivityTimeout
	}
	return l, nil
}
