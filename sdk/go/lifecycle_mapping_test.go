package codex

import (
	"reflect"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestLifecycleMappingsMatchGeneratedRustMetadata(t *testing.T) {
	for _, mapping := range lifecycleMappings {
		rust, ok := rustLifecycleByStartMethod(mapping.StartMethod)
		if !ok {
			t.Fatalf("missing Rust lifecycle metadata for %s", mapping.StartMethod)
		}
		if rust.ResourceDomain != mapping.ResourceDomain {
			t.Fatalf("%s domain = %q, want %q", mapping.StartMethod, rust.ResourceDomain, mapping.ResourceDomain)
		}
	}
	for startMethod := range protocol.RoutingLifecycleByStartMethod {
		if _, ok := lifecycleMappingByStartMethod(startMethod); !ok {
			t.Fatalf("missing Go lifecycle mapping for generated start method %s", startMethod)
		}
	}
}

func TestFSWatchLifecycleKeepsExplicitRustUnwatchAndGoCloseTriggers(t *testing.T) {
	mapping, ok := lifecycleMappingByStartMethod("fs/watch")
	if !ok {
		t.Fatal("missing fs/watch lifecycle mapping")
	}
	wantTriggers := []goLifecycleTrigger{
		goLifecycleHandleClose,
		goLifecycleClientClose,
		goLifecycleTimeout,
		goLifecycleOverflow,
	}
	if !reflect.DeepEqual(mapping.GoTriggers, wantTriggers) {
		t.Fatalf("GoTriggers = %#v, want %#v", mapping.GoTriggers, wantTriggers)
	}

	rust, ok := rustLifecycleByStartMethod("fs/watch")
	if !ok {
		t.Fatal("missing generated fs/watch lifecycle metadata")
	}
	if len(rust.CleanupTriggers) != 1 {
		t.Fatalf("cleanup triggers = %#v", rust.CleanupTriggers)
	}
	trigger := rust.CleanupTriggers[0]
	if trigger.Kind != "explicitMethodResponse" || trigger.Method != "fs/unwatch" {
		t.Fatalf("fs/watch cleanup = %#v", trigger)
	}
}

func TestFSWatchLifecycleDependencyUsesNotificationRouteDomain(t *testing.T) {
	lifecycle, ok := rustLifecycleByStartMethod("fs/watch")
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
