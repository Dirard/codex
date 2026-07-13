package jsonrpc

import (
	"bytes"
	"strings"
	"sync"
)

const ringProcessingChunkBytes = 32 * 1024

var sensitiveDiagnosticWords = [...]string{
	"api_key",
	"apikey",
	"token",
	"secret",
	"password",
	"credential",
	"auth",
	"cookie",
}

var maxSensitiveDiagnosticWordBytes = func() int {
	maxBytes := 0
	for _, word := range sensitiveDiagnosticWords {
		if len(word) > maxBytes {
			maxBytes = len(word)
		}
	}
	return maxBytes
}()

type Ring struct {
	mu              sync.Mutex
	limit           int
	data            []byte
	pending         []byte
	pendingRedacted bool
}

func NewRing(limit int) *Ring {
	return &Ring{limit: limit}
}

func (r *Ring) Write(p []byte) (int, error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.limit <= 0 {
		return len(p), nil
	}
	written := len(p)
	for len(p) > 0 {
		newline := bytes.IndexByte(p, '\n')
		if newline < 0 {
			r.appendPending(p)
			break
		}
		r.appendPending(p[:newline])
		if r.pendingRedacted {
			r.appendData([]byte("[redacted]\n"))
		} else {
			r.appendData(append(append([]byte(nil), r.pending...), '\n'))
		}
		r.pending = nil
		r.pendingRedacted = false
		p = p[newline+1:]
	}
	return written, nil
}

func (r *Ring) String() string {
	r.mu.Lock()
	defer r.mu.Unlock()
	result := append([]byte(nil), r.data...)
	if r.pendingRedacted {
		result = append(result, []byte("[redacted]\n")...)
	} else {
		result = append(result, r.pending...)
	}
	if len(result) > r.limit {
		result = result[len(result)-r.limit:]
	}
	return string(result)
}

func (r *Ring) appendPending(p []byte) {
	if r.pendingRedacted {
		return
	}
	pendingLimit := r.limit
	if markerTail := maxSensitiveDiagnosticWordBytes - 1; pendingLimit < markerTail {
		pendingLimit = markerTail
	}
	for len(p) > 0 {
		chunkBytes := min(len(p), ringProcessingChunkBytes)
		r.pending = append(r.pending, p[:chunkBytes]...)
		if containsSensitiveText(string(r.pending)) {
			r.pending = nil
			r.pendingRedacted = true
			return
		}
		if len(r.pending) > pendingLimit {
			r.pending = append([]byte(nil), r.pending[len(r.pending)-pendingLimit:]...)
		}
		p = p[chunkBytes:]
	}
}

func (r *Ring) appendData(p []byte) {
	r.data = append(r.data, p...)
	if len(r.data) > r.limit {
		r.data = append([]byte(nil), r.data[len(r.data)-r.limit:]...)
	}
}

func containsSensitiveText(s string) bool {
	lower := strings.ToLower(s)
	for _, word := range sensitiveDiagnosticWords {
		if strings.Contains(lower, word) {
			return true
		}
	}
	return false
}
