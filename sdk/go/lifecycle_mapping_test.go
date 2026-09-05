package codex

import (
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestFSWatchLifecycleDependencyUsesNotificationRouteDomain(t *testing.T) {
	lifecycle, ok := protocol.RoutingLifecycleByStartMethod["fs/watch"]
	if !ok {
		t.Fatal("missing generated fs/watch lifecycle metadata")
	}
	notification, ok := protocol.ServerNotificationRoutingByMethod["fs/changed"]
	if !ok {
		t.Fatal("missing generated fs/changed routing metadata")
	}
	for _, route := range notification.Routes {
		if route.ResourceDomain == lifecycle.ResourceDomain {
			return
		}
	}
	t.Fatalf("fs/watch lifecycle domain %q has no matching fs/changed route: %#v", lifecycle.ResourceDomain, notification.Routes)
}
