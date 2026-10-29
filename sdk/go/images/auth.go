package images

import (
	"context"
	"errors"
	"net/http"
	"os"
	"strings"
)

// Auth sets the credentials of a request.
type Auth interface {
	Apply(ctx context.Context, req *http.Request) error
}

// Refresher is an Auth that can renew its credentials. After a 401 the client calls
// Refresh once and repeats the request.
type Refresher interface {
	Refresh(ctx context.Context) error
}

// AuthFunc adapts a function to Auth.
type AuthFunc func(ctx context.Context, req *http.Request) error

// Apply calls f.
func (f AuthFunc) Apply(ctx context.Context, req *http.Request) error { return f(ctx, req) }

// BearerToken sends a fixed token as "Authorization: Bearer <token>".
func BearerToken(token string) Auth {
	return AuthFunc(func(_ context.Context, req *http.Request) error {
		if strings.TrimSpace(token) == "" {
			return errors.New("empty bearer token")
		}
		req.Header.Set("Authorization", "Bearer "+token)
		return nil
	})
}

// BearerFromEnv reads the token from the environment variable name on every request, so a
// rotated key takes effect without a new client.
func BearerFromEnv(name string) Auth {
	return AuthFunc(func(_ context.Context, req *http.Request) error {
		token := strings.TrimSpace(os.Getenv(name))
		if token == "" {
			return errors.New("environment variable " + name + " is empty")
		}
		req.Header.Set("Authorization", "Bearer "+token)
		return nil
	})
}
