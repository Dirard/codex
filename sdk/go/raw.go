package codex

import "github.com/openai/codex/sdk/go/protocol"

// Raw returns the generated typed app-server client.
func (c *Client) Raw() *protocol.RawClient {
	if c == nil {
		return nil
	}
	return c.raw
}
