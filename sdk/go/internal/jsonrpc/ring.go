package jsonrpc

import (
	"strings"
	"sync"
)

type Ring struct {
	mu    sync.Mutex
	limit int
	data  []byte
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
	redacted := []byte(redactText(string(p)))
	r.data = append(r.data, redacted...)
	if len(r.data) > r.limit {
		r.data = append([]byte(nil), r.data[len(r.data)-r.limit:]...)
	}
	return len(p), nil
}

func (r *Ring) String() string {
	r.mu.Lock()
	defer r.mu.Unlock()
	return string(append([]byte(nil), r.data...))
}

func redactText(s string) string {
	words := []string{"api_key", "apikey", "token", "secret", "password", "credential", "auth", "cookie"}
	lower := strings.ToLower(s)
	for _, word := range words {
		if strings.Contains(lower, word) {
			return "[redacted]\n"
		}
	}
	return s
}
