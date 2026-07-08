package protocodex

import (
	"fmt"
	"sort"
	"strings"
)

type ResourceAPIMapping struct {
	Method                 string
	ResourceOwner          string
	WrapperName            string
	WrapperFile            string
	PublicSignature        string
	SignatureConventionID  string
	CompileCallsite        string
	UnitTestOwner          string
	SafeIntegrationOwner   string
	SafeIntegrationReason  string
	DocsExampleOwner       string
	ServerHandlerLinks     []string
	GeneratedOnlyException string
	ReviewNote             string
}

type ServerHandlerMapping struct {
	Method                 string
	HandlerOwner           string
	Visibility             string
	Capability             string
	UnitTestOwner          string
	DocsExampleOwner       string
	GeneratedOnlyException string
	ReviewNote             string
}

type ServerNotificationResourceMapping struct {
	Method         string
	ResourceOwners []string
	ReviewNote     string
}

type ValidationError struct {
	Problems []string
}

func (e *ValidationError) Error() string {
	if len(e.Problems) == 0 {
		return "validation failed"
	}
	problems := append([]string(nil), e.Problems...)
	sort.Strings(problems)
	return "validation failed:\n- " + strings.Join(problems, "\n- ")
}

func ValidateResourceMappings(manifest *Manifest, schema *SchemaBundle, resourceMappings []ResourceAPIMapping, handlerMappings []ServerHandlerMapping) error {
	if manifest == nil {
		return &ValidationError{Problems: []string{"manifest is nil"}}
	}
	if schema == nil {
		return &ValidationError{Problems: []string{"schema bundle is nil"}}
	}

	var problems []string
	if err := validateProtocolSchemaManifestMode("experimental", manifest.Experimental, schema); err != nil {
		problems = append(problems, err.Error())
	}
	resourceByMethod := mapResourceMappings(resourceMappings, &problems)
	handlerByMethod := mapServerHandlerMappings(handlerMappings, &problems)
	clientRequests := mapClientRequests(manifest.Experimental.ClientRequests)
	serverRequests := mapServerRequests(manifest.Experimental.ServerRequests)

	for method := range resourceByMethod {
		if _, ok := clientRequests[method]; !ok {
			problems = append(problems, fmt.Sprintf("resource mapping for absent client method %q", method))
		}
	}
	for method := range handlerByMethod {
		if _, ok := serverRequests[method]; !ok {
			problems = append(problems, fmt.Sprintf("server handler mapping for absent server request %q", method))
		}
	}

	for _, entry := range manifest.Experimental.ClientRequests {
		mapping, ok := resourceByMethod[entry.Method]
		if !ok {
			problems = append(problems, fmt.Sprintf("missing resource mapping for client method %q", entry.Method))
			continue
		}
		validateResourceMapping(entry, mapping, &problems)
	}
	for _, entry := range manifest.Experimental.ServerRequests {
		mapping, ok := handlerByMethod[entry.Method]
		if !ok {
			problems = append(problems, fmt.Sprintf("missing server handler mapping for server request %q", entry.Method))
			continue
		}
		validateServerHandlerMapping(entry, mapping, &problems)
	}
	validateServerNotificationResourceMappings(manifest.Experimental.ServerNotifications, &problems)

	if len(problems) > 0 {
		return &ValidationError{Problems: problems}
	}
	return nil
}

func mapResourceMappings(mappings []ResourceAPIMapping, problems *[]string) map[string]ResourceAPIMapping {
	byMethod := make(map[string]ResourceAPIMapping, len(mappings))
	for _, mapping := range mappings {
		if mapping.Method == "" {
			*problems = append(*problems, "resource mapping with empty method")
			continue
		}
		if _, ok := byMethod[mapping.Method]; ok {
			*problems = append(*problems, fmt.Sprintf("duplicate resource mapping for %q", mapping.Method))
			continue
		}
		byMethod[mapping.Method] = mapping
	}
	return byMethod
}

func mapServerHandlerMappings(mappings []ServerHandlerMapping, problems *[]string) map[string]ServerHandlerMapping {
	byMethod := make(map[string]ServerHandlerMapping, len(mappings))
	for _, mapping := range mappings {
		if mapping.Method == "" {
			*problems = append(*problems, "server handler mapping with empty method")
			continue
		}
		if _, ok := byMethod[mapping.Method]; ok {
			*problems = append(*problems, fmt.Sprintf("duplicate server handler mapping for %q", mapping.Method))
			continue
		}
		byMethod[mapping.Method] = mapping
	}
	return byMethod
}

func mapClientRequests(entries []ClientRequestEntry) map[string]ClientRequestEntry {
	byMethod := make(map[string]ClientRequestEntry, len(entries))
	for _, entry := range entries {
		byMethod[entry.Method] = entry
	}
	return byMethod
}

func mapServerRequests(entries []ServerRequestEntry) map[string]ServerRequestEntry {
	byMethod := make(map[string]ServerRequestEntry, len(entries))
	for _, entry := range entries {
		byMethod[entry.Method] = entry
	}
	return byMethod
}

func validateResourceMapping(entry ClientRequestEntry, mapping ResourceAPIMapping, problems *[]string) {
	if entry.SDKVisibility != "public" {
		if mapping.GeneratedOnlyException == "" {
			*problems = append(*problems, fmt.Sprintf("%s non-public mapping has no exception", entry.Method))
		}
		if mapping.ReviewNote == "" {
			*problems = append(*problems, fmt.Sprintf("%s non-public mapping has no review note", entry.Method))
		}
		return
	}

	required := []struct {
		name  string
		value string
	}{
		{"resource owner", mapping.ResourceOwner},
		{"wrapper file", mapping.WrapperFile},
		{"wrapper name", mapping.WrapperName},
		{"signature convention", mapping.SignatureConventionID},
		{"compile callsite", mapping.CompileCallsite},
		{"unit test owner", mapping.UnitTestOwner},
		{"docs/example owner", mapping.DocsExampleOwner},
		{"review note", mapping.ReviewNote},
	}
	for _, field := range required {
		if field.value == "" {
			*problems = append(*problems, fmt.Sprintf("%s SDK-public mapping has no %s", entry.Method, field.name))
		}
	}
	if mapping.SafeIntegrationOwner == "" && mapping.SafeIntegrationReason == "" {
		*problems = append(*problems, fmt.Sprintf("%s SDK-public mapping has no safe integration owner or reason", entry.Method))
	}
	if !approvedResourceOwner(mapping.ResourceOwner) {
		*problems = append(*problems, fmt.Sprintf("%s has unapproved resource owner %q", entry.Method, mapping.ResourceOwner))
	}
	if mapping.SignatureConventionID == "high-level" && strings.Contains(mapping.CompileCallsite, "protocol.") {
		*problems = append(*problems, fmt.Sprintf("%s high-level compile callsite must not expose protocol.* params: %s", entry.Method, mapping.CompileCallsite))
	}
	if mapping.SignatureConventionID == "handle-start" {
		validateHandleStartMapping(entry, mapping, problems)
	}
	if !compileCallsiteNamesWrapper(mapping.WrapperName, mapping.CompileCallsite) {
		*problems = append(*problems, fmt.Sprintf("%s compile callsite %q does not name declared wrapper %q", entry.Method, mapping.CompileCallsite, mapping.WrapperName))
	}
}

func validateHandleStartMapping(entry ClientRequestEntry, mapping ResourceAPIMapping, problems *[]string) {
	if strings.Contains(mapping.CompileCallsite, "protocol.") || strings.Contains(mapping.PublicSignature, "protocol.") {
		*problems = append(*problems, fmt.Sprintf("%s handle-start public shape must not expose protocol.* params", entry.Method))
	}
	if !entryHasIdentityExtractor(entry) {
		return
	}
	if !strings.Contains(mapping.CompileCallsite, "codex.") || !strings.Contains(mapping.CompileCallsite, "Options") {
		*problems = append(*problems, fmt.Sprintf("%s identity-bearing handle-start callsite must use root SDK options: %s", entry.Method, mapping.CompileCallsite))
	}
	if mapping.PublicSignature != "" && (!strings.Contains(mapping.PublicSignature, "codex.") || !strings.Contains(mapping.PublicSignature, "Options")) {
		*problems = append(*problems, fmt.Sprintf("%s identity-bearing handle-start public signature must use root SDK options: %s", entry.Method, mapping.PublicSignature))
	}
}

func entryHasIdentityExtractor(entry ClientRequestEntry) bool {
	for _, scope := range entry.RequestSerializationScopes {
		if len(scope.IdentityExtractors) > 0 {
			return true
		}
	}
	return false
}

func validateServerHandlerMapping(entry ServerRequestEntry, mapping ServerHandlerMapping, problems *[]string) {
	if mapping.Visibility == "" {
		*problems = append(*problems, fmt.Sprintf("%s server handler mapping has no visibility", entry.Method))
	}
	if mapping.Capability == "" {
		*problems = append(*problems, fmt.Sprintf("%s server handler mapping has no capability", entry.Method))
	}
	if mapping.UnitTestOwner == "" {
		*problems = append(*problems, fmt.Sprintf("%s server handler mapping has no unit test owner", entry.Method))
	}
	if mapping.ReviewNote == "" {
		*problems = append(*problems, fmt.Sprintf("%s server handler mapping has no review note", entry.Method))
	}
	if entry.SDKVisibility == "public" {
		if mapping.HandlerOwner == "" {
			*problems = append(*problems, fmt.Sprintf("%s public server handler mapping has no handler owner", entry.Method))
		}
		if mapping.DocsExampleOwner == "" {
			*problems = append(*problems, fmt.Sprintf("%s public server handler mapping has no docs/example owner", entry.Method))
		}
	}
	if entry.SDKVisibility != "public" && mapping.GeneratedOnlyException == "" {
		*problems = append(*problems, fmt.Sprintf("%s non-public server handler mapping has no exception", entry.Method))
	}
}

func validateServerNotificationResourceMappings(entries []NotificationEntry, problems *[]string) {
	notifications := map[string]NotificationEntry{}
	for _, entry := range entries {
		notifications[entry.Method] = entry
	}
	mapped := map[string]ServerNotificationResourceMapping{}
	for _, mapping := range serverNotificationResourceMappings {
		_, ok := notifications[mapping.Method]
		if !ok {
			*problems = append(*problems, fmt.Sprintf("server notification resource mapping for absent notification %q", mapping.Method))
			continue
		}
		if mapping.ReviewNote == "" {
			*problems = append(*problems, fmt.Sprintf("%s server notification resource mapping has no review note", mapping.Method))
		}
		if len(mapping.ResourceOwners) == 0 {
			*problems = append(*problems, fmt.Sprintf("%s server notification resource mapping has no resource owners", mapping.Method))
		}
		for _, owner := range mapping.ResourceOwners {
			if !approvedResourceOwner(owner) {
				*problems = append(*problems, fmt.Sprintf("%s server notification resource mapping has unapproved owner %q", mapping.Method, owner))
			}
		}
		mapped[mapping.Method] = mapping
	}
	for _, entry := range entries {
		if entry.SDKVisibility != "public" || len(routedOwners(entry)) > 0 {
			continue
		}
		if _, ok := mapped[entry.Method]; !ok {
			*problems = append(*problems, fmt.Sprintf("%s global server notification has no resource-owner mapping", entry.Method))
		}
	}
}

func approvedResourceOwner(owner string) bool {
	switch owner {
	case "Accounts",
		"Apps",
		"CollaborationModes",
		"Commands",
		"Config",
		"Environments",
		"ExperimentalFeatures",
		"ExternalAgents",
		"Feedback",
		"FileSystem",
		"FuzzyFileSearch",
		"Hooks",
		"MCP",
		"Marketplace",
		"Memory",
		"Models",
		"PermissionProfiles",
		"Plugins",
		"Processes",
		"Realtime",
		"RemoteControl",
		"Reviews",
		"Skills",
		"Threads",
		"Turns",
		"WindowsSandbox",
		"compatibility",
		"handshake",
		"internal test only":
		return true
	default:
		return false
	}
}

func compileCallsiteNamesWrapper(wrapperName, compileCallsite string) bool {
	if wrapperName == "" || compileCallsite == "" {
		return false
	}
	for _, candidate := range strings.Split(wrapperName, " / ") {
		candidate = strings.TrimSpace(candidate)
		if candidate == "" {
			continue
		}
		if strings.Contains(candidate, " ") {
			candidate = strings.Fields(candidate)[0]
		}
		if strings.Contains(compileCallsite, candidate) {
			return true
		}
		if dot := strings.LastIndex(candidate, "."); dot >= 0 && dot+1 < len(candidate) {
			if strings.Contains(compileCallsite, candidate[dot+1:]) {
				return true
			}
		}
	}
	return false
}
