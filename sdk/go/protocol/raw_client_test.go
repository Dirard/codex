package protocol

import (
	"context"
	"testing"
)

func TestRawAccountRateLimitsReadKeepsZeroArgumentCompatibility(t *testing.T) {
	var call func(context.Context) (GetAccountRateLimitsResponse, error) = RawClient{}.AccountRateLimitsRead
	_ = call
}
