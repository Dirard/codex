package protocodex

import (
	"fmt"
	"sort"
	"strings"
)

func renderHandlersGenerated(manifest *Manifest) string {
	var b strings.Builder
	b.WriteString("package codex\n\n")
	b.WriteString("import (\n\t\"context\"\n\t\"encoding/json\"\n\t\"fmt\"\n\n\t\"github.com/openai/codex/sdk/go/protocol\"\n)\n\n")
	serverRequests := mapServerRequests(manifest.Experimental.ServerRequests)
	publicByField := map[string][]ServerHandlerMapping{}
	var fieldNames []string
	for _, mapping := range serverHandlerMappings {
		if !isPublicServerHandler(mapping) {
			continue
		}
		field := strings.TrimPrefix(mapping.HandlerOwner, "ServerHandlers.")
		if _, ok := publicByField[field]; !ok {
			fieldNames = append(fieldNames, field)
		}
		publicByField[field] = append(publicByField[field], mapping)
	}
	sort.Strings(fieldNames)
	b.WriteString("type ServerHandlers struct {\n")
	for _, field := range fieldNames {
		b.WriteString(fmt.Sprintf("\t%s %s\n", field, field+"Handler"))
	}
	b.WriteString("\tUnknown UnknownServerRequestHandler\n")
	b.WriteString("}\n\n")
	for _, field := range fieldNames {
		b.WriteString(fmt.Sprintf("type %s interface {\n", field+"Handler"))
		for _, mapping := range publicByField[field] {
			entry := serverRequests[mapping.Method]
			params := typeNameForDefinition(entry.PayloadType)
			response := typeNameForDefinition(entry.ResponseType)
			if params == "" {
				params = "json.RawMessage"
			}
			if response == "" {
				response = "json.RawMessage"
			}
			b.WriteString(fmt.Sprintf("\tHandle%s(ctx context.Context, params %s) (%s, error)\n", RawMethodName(mapping.Method), qualifiedProtocolType(params), qualifiedProtocolType(response)))
		}
		b.WriteString("}\n\n")
		if len(publicByField[field]) > 1 {
			b.WriteString(fmt.Sprintf("type %s struct {\n", field+"HandlerFuncs"))
			for _, mapping := range publicByField[field] {
				entry := serverRequests[mapping.Method]
				params := typeNameForDefinition(entry.PayloadType)
				response := typeNameForDefinition(entry.ResponseType)
				if params == "" {
					params = "json.RawMessage"
				}
				if response == "" {
					response = "json.RawMessage"
				}
				funcField := RawMethodName(mapping.Method)
				b.WriteString(fmt.Sprintf("\t%s func(ctx context.Context, params %s) (%s, error)\n", funcField, qualifiedProtocolType(params), qualifiedProtocolType(response)))
			}
			b.WriteString("}\n\n")
			for _, mapping := range publicByField[field] {
				entry := serverRequests[mapping.Method]
				params := typeNameForDefinition(entry.PayloadType)
				response := typeNameForDefinition(entry.ResponseType)
				if params == "" {
					params = "json.RawMessage"
				}
				if response == "" {
					response = "json.RawMessage"
				}
				methodName := "Handle" + RawMethodName(mapping.Method)
				funcField := RawMethodName(mapping.Method)
				b.WriteString(fmt.Sprintf("func (f %s) %s(ctx context.Context, params %s) (%s, error) {\n", field+"HandlerFuncs", methodName, qualifiedProtocolType(params), qualifiedProtocolType(response)))
				b.WriteString(fmt.Sprintf("\tvar zero %s\n", qualifiedProtocolType(response)))
				b.WriteString(fmt.Sprintf("\tif f.%s == nil { return zero, fmt.Errorf(\"server handler %%q is not configured\", %q) }\n", funcField, mapping.Method))
				b.WriteString(fmt.Sprintf("\treturn f.%s(ctx, params)\n", funcField))
				b.WriteString("}\n\n")
			}
		}
		for _, mapping := range publicByField[field] {
			entry := serverRequests[mapping.Method]
			params := typeNameForDefinition(entry.PayloadType)
			response := typeNameForDefinition(entry.ResponseType)
			if params == "" {
				params = "json.RawMessage"
			}
			if response == "" {
				response = "json.RawMessage"
			}
			methodName := "Handle" + RawMethodName(mapping.Method)
			funcName := field + RawMethodName(mapping.Method) + "Func"
			b.WriteString(fmt.Sprintf("type %s func(ctx context.Context, params %s) (%s, error)\n\n", funcName, qualifiedProtocolType(params), qualifiedProtocolType(response)))
			b.WriteString(fmt.Sprintf("func (f %s) %s(ctx context.Context, params %s) (%s, error) { return f(ctx, params) }\n\n", funcName, methodName, qualifiedProtocolType(params), qualifiedProtocolType(response)))
		}
	}
	b.WriteString("func (h ServerHandlers) DispatchServerRequest(ctx context.Context, method string, params json.RawMessage) (any, error) {\n")
	b.WriteString("\tswitch method {\n")
	for _, mapping := range serverHandlerMappings {
		entry := serverRequests[mapping.Method]
		params := typeNameForDefinition(entry.PayloadType)
		if params == "" {
			params = "json.RawMessage"
		}
		b.WriteString(fmt.Sprintf("\tcase %q:\n", mapping.Method))
		if !isPublicServerHandler(mapping) {
			b.WriteString(fmt.Sprintf("\t\tif _, err := decode%sServerRequest(params); err != nil { return nil, err }\n", RawMethodName(mapping.Method)))
			b.WriteString(fmt.Sprintf("\t\treturn nil, fmt.Errorf(\"server request %%q has no public handler: %s\", method)\n", mapping.Method))
			continue
		}
		b.WriteString(fmt.Sprintf("\t\tdecoded, err := decode%sServerRequest(params)\n", RawMethodName(mapping.Method)))
		b.WriteString("\t\tif err != nil { return nil, err }\n")
		field := strings.TrimPrefix(mapping.HandlerOwner, "ServerHandlers.")
		b.WriteString(fmt.Sprintf("\t\tif h.%s == nil { return nil, &UnsupportedError{Reason: fmt.Sprintf(\"server handler %%q is not configured\", method)} }\n", field))
		b.WriteString(fmt.Sprintf("\t\treturn h.%s.Handle%s(ctx, decoded)\n", field, RawMethodName(mapping.Method)))
	}
	b.WriteString("\tdefault:\n\t\tif h.Unknown != nil { return h.Unknown.HandleUnknownServerRequest(ctx, UnknownServerRequest{Method: method, Params: append(json.RawMessage(nil), params...)}) }\n\t\treturn nil, &UnsupportedError{Reason: fmt.Sprintf(\"unsupported server request method %q\", method)}\n\t}\n}\n\n")
	for _, mapping := range serverHandlerMappings {
		entry := serverRequests[mapping.Method]
		params := typeNameForDefinition(entry.PayloadType)
		if params == "" {
			params = "json.RawMessage"
		}
		if !isPublicServerHandler(mapping) {
			params = "json.RawMessage"
		}
		b.WriteString(fmt.Sprintf("func decode%sServerRequest(params json.RawMessage) (%s, error) {\n", RawMethodName(mapping.Method), qualifiedProtocolType(params)))
		if params == "json.RawMessage" {
			b.WriteString("\tif !json.Valid(params) { return nil, fmt.Errorf(\"invalid JSON params\") }\n")
			b.WriteString("\treturn params, nil\n")
		} else {
			b.WriteString(fmt.Sprintf("\tvar decoded %s\n", qualifiedProtocolType(params)))
			b.WriteString("\tif err := json.Unmarshal(params, &decoded); err != nil { return decoded, err }\n")
			b.WriteString("\treturn decoded, nil\n")
		}
		b.WriteString("}\n\n")
	}
	b.WriteString("type generatedServerHandlerMetadataRow struct { Method string; Visibility string; Capability string; HandlerOwner string }\n\n")
	b.WriteString("var generatedServerHandlerMetadata = []generatedServerHandlerMetadataRow{\n")
	for _, mapping := range serverHandlerMappings {
		b.WriteString(fmt.Sprintf("\t{Method: %q, Visibility: %q, Capability: %q, HandlerOwner: %q},\n", mapping.Method, mapping.Visibility, mapping.Capability, mapping.HandlerOwner))
	}
	b.WriteString("}\n")
	return b.String()
}

func isPublicServerHandler(mapping ServerHandlerMapping) bool {
	return mapping.Visibility == "sdk-public" || mapping.Visibility == "experimental-public"
}

func qualifiedProtocolType(name string) string {
	if strings.HasPrefix(name, "json.") {
		return name
	}
	return "protocol." + name
}

func renderResourceCoverageGenerated(manifest *Manifest) string {
	var b strings.Builder
	b.WriteString("package codex\n\n")
	b.WriteString("type generatedResourceCoverageRow struct { Method string; SDKVisibility string; ImplementationStatus string; ResourceOwner string; RawMethodName string; WrapperName string; WrapperFile string; PublicSignature string; SignatureConventionID string; CompileCallsite string; UnitTestOwner string; SafeIntegrationOwner string; SafeIntegrationReason string; DocsExampleOwner string; ServerNotificationMethods []string; ServerHandlerCapabilities []string; GeneratedOnlyException string; ReviewNote string }\n\n")
	b.WriteString("var generatedResourceCoverage = []generatedResourceCoverageRow{\n")
	visibility := map[string]string{}
	for _, entry := range manifest.Experimental.ClientRequests {
		visibility[entry.Method] = entry.SDKVisibility
	}
	notificationsByOwner := serverNotificationMethodsByOwner(manifest)
	handlersByOwner := serverHandlerCapabilitiesByOwner()
	for _, mapping := range resourceAPIMappings {
		b.WriteString(fmt.Sprintf("\t{Method: %q, SDKVisibility: %q, ImplementationStatus: %q, ResourceOwner: %q, RawMethodName: %q, WrapperName: %q, WrapperFile: %q, PublicSignature: %q, SignatureConventionID: %q, CompileCallsite: %q, UnitTestOwner: %q, SafeIntegrationOwner: %q, SafeIntegrationReason: %q, DocsExampleOwner: %q, ServerNotificationMethods: %#v, ServerHandlerCapabilities: %#v, GeneratedOnlyException: %q, ReviewNote: %q},\n", mapping.Method, visibility[mapping.Method], resourceImplementationStatus(mapping), mapping.ResourceOwner, RawMethodName(mapping.Method), mapping.WrapperName, mapping.WrapperFile, mapping.PublicSignature, mapping.SignatureConventionID, mapping.CompileCallsite, mapping.UnitTestOwner, mapping.SafeIntegrationOwner, mapping.SafeIntegrationReason, mapping.DocsExampleOwner, notificationsByOwner[mapping.ResourceOwner], handlersByOwner[mapping.ResourceOwner], mapping.GeneratedOnlyException, mapping.ReviewNote))
	}
	b.WriteString("}\n")
	return b.String()
}

func resourceImplementationStatus(mapping ResourceAPIMapping) string {
	if mapping.GeneratedOnlyException != "" {
		return "generated-only"
	}
	if stage4ImplementedResourceMethods[mapping.Method] {
		return "implemented-stage4"
	}
	if stage5BImplementedResourceMethods[mapping.Method] {
		return "implemented-stage5b"
	}
	if stage5CImplementedResourceMethods[mapping.Method] {
		return "implemented-stage5c"
	}
	if stage5DImplementedResourceMethods[mapping.Method] {
		return "implemented-stage5d"
	}
	if stage5EImplementedResourceMethods[mapping.Method] {
		return "implemented-stage5e"
	}
	if stage5FImplementedResourceMethods[mapping.Method] {
		return "implemented-stage5f"
	}
	return "planned-stage5"
}

var stage4ImplementedResourceMethods = map[string]bool{
	"account/login/start":     true,
	"account/login/cancel":    true,
	"account/logout":          true,
	"account/rateLimits/read": true,
	"account/usage/read":      true,
	"account/read":            true,
	"mcpServer/oauth/login":   true,
	"review/start":            true,
	"thread/start":            true,
	"turn/start":              true,
	"turn/steer":              true,
	"turn/interrupt":          true,
}

var stage5BImplementedResourceMethods = map[string]bool{
	"hooks/list":                           true,
	"skills/list":                          true,
	"skills/extraRoots/set":                true,
	"skills/config/write":                  true,
	"thread/resume":                        true,
	"thread/fork":                          true,
	"thread/archive":                       true,
	"thread/delete":                        true,
	"thread/unsubscribe":                   true,
	"thread/name/set":                      true,
	"thread/goal/set":                      true,
	"thread/goal/get":                      true,
	"thread/goal/clear":                    true,
	"thread/metadata/update":               true,
	"thread/unarchive":                     true,
	"thread/compact/start":                 true,
	"thread/shellCommand":                  true,
	"thread/approveGuardianDeniedAction":   true,
	"thread/rollback":                      true,
	"thread/list":                          true,
	"thread/loaded/list":                   true,
	"thread/read":                          true,
	"thread/inject_items":                  true,
	"thread/realtime/start":                true,
	"thread/realtime/appendAudio":          true,
	"thread/realtime/appendText":           true,
	"thread/realtime/appendSpeech":         true,
	"thread/realtime/stop":                 true,
	"thread/realtime/listVoices":           true,
	"thread/increment_elicitation":         true,
	"thread/decrement_elicitation":         true,
	"thread/settings/update":               true,
	"thread/memoryMode/set":                true,
	"thread/backgroundTerminals/clean":     true,
	"thread/backgroundTerminals/list":      true,
	"thread/backgroundTerminals/terminate": true,
	"thread/search":                        true,
	"thread/searchOccurrences":             true,
	"thread/turns/list":                    true,
	"thread/items/list":                    true,
}

var stage5CImplementedResourceMethods = map[string]bool{
	"command/exec":            true,
	"command/exec/write":      true,
	"command/exec/terminate":  true,
	"command/exec/resize":     true,
	"config/mcpServer/reload": true,
	"config/read":             true,
	"config/value/write":      true,
	"config/batchWrite":       true,
	"configRequirements/read": true,
	"fs/readFile":             true,
	"fs/writeFile":            true,
	"fs/createDirectory":      true,
	"fs/getMetadata":          true,
	"fs/readDirectory":        true,
	"fs/remove":               true,
	"fs/copy":                 true,
	"fs/watch":                true,
	"fs/unwatch":              true,
	"process/spawn":           true,
	"process/writeStdin":      true,
	"process/kill":            true,
	"process/resizePty":       true,
}

var stage5DImplementedResourceMethods = map[string]bool{
	"app/list":                   true,
	"app/read":                   true,
	"app/installed":              true,
	"marketplace/add":            true,
	"marketplace/remove":         true,
	"marketplace/upgrade":        true,
	"mcpServer/resource/read":    true,
	"mcpServer/tool/call":        true,
	"mcpServerStatus/list":       true,
	"plugin/list":                true,
	"plugin/installed":           true,
	"plugin/read":                true,
	"plugin/skill/read":          true,
	"plugin/share/save":          true,
	"plugin/share/updateTargets": true,
	"plugin/share/list":          true,
	"plugin/share/checkout":      true,
	"plugin/share/delete":        true,
	"plugin/install":             true,
	"plugin/uninstall":           true,
}

var stage5EImplementedResourceMethods = map[string]bool{
	"account/rateLimitResetCredit/consume":     true,
	"account/sendAddCreditsNudgeEmail":         true,
	"account/workspaceMessages/read":           true,
	"collaborationMode/list":                   true,
	"environment/add":                          true,
	"environment/info":                         true,
	"environment/status":                       true,
	"externalAgentConfig/detect":               true,
	"externalAgentConfig/import":               true,
	"externalAgentConfig/import/readHistories": true,
	"model/list":                               true,
	"modelProvider/capabilities/read":          true,
	"remoteControl/client/list":                true,
	"remoteControl/client/revoke":              true,
	"remoteControl/disable":                    true,
	"remoteControl/enable":                     true,
	"remoteControl/pairing/start":              true,
	"remoteControl/pairing/status":             true,
	"remoteControl/status/read":                true,
}

var stage5FImplementedResourceMethods = map[string]bool{
	"experimentalFeature/list":           true,
	"experimentalFeature/enablement/set": true,
	"feedback/upload":                    true,
	"fuzzyFileSearch":                    true,
	"fuzzyFileSearch/sessionStart":       true,
	"fuzzyFileSearch/sessionUpdate":      true,
	"fuzzyFileSearch/sessionStop":        true,
	"memory/reset":                       true,
	"permissionProfile/list":             true,
	"windowsSandbox/readiness":           true,
	"windowsSandbox/setupStart":          true,
}

func renderInventory(manifest *Manifest) string {
	var b strings.Builder
	b.WriteString("# Current Protocol Inventory\n\n")
	b.WriteString("## Client Methods By Resource Owner\n\n")
	mappingsByOwner := map[string][]ResourceAPIMapping{}
	var owners []string
	for _, mapping := range resourceAPIMappings {
		if _, ok := mappingsByOwner[mapping.ResourceOwner]; !ok {
			owners = append(owners, mapping.ResourceOwner)
		}
		mappingsByOwner[mapping.ResourceOwner] = append(mappingsByOwner[mapping.ResourceOwner], mapping)
	}
	sort.Strings(owners)
	for _, owner := range owners {
		b.WriteString(fmt.Sprintf("### %s\n\n", owner))
		ownerNotifications := strings.Join(serverNotificationMethodsByOwner(manifest)[owner], ",")
		ownerHandlers := strings.Join(serverHandlerCapabilitiesByOwner()[owner], ",")
		b.WriteString(fmt.Sprintf("serverNotifications=%s\n", ownerNotifications))
		b.WriteString(fmt.Sprintf("serverHandlers=%s\n\n", ownerHandlers))
		for _, mapping := range mappingsByOwner[owner] {
			b.WriteString(fmt.Sprintf("- `%s` status=%s raw=%s wrapper=%s file=%s signature=%s convention=%s callsite=%s unitTest=%s safeIntegration=%s%s docs=%s exception=%s review=%s\n", mapping.Method, resourceImplementationStatus(mapping), RawMethodName(mapping.Method), mapping.WrapperName, mapping.WrapperFile, mapping.PublicSignature, mapping.SignatureConventionID, mapping.CompileCallsite, mapping.UnitTestOwner, mapping.SafeIntegrationOwner, mapping.SafeIntegrationReason, mapping.DocsExampleOwner, mapping.GeneratedOnlyException, mapping.ReviewNote))
		}
		b.WriteByte('\n')
	}
	b.WriteString("\n## Server Requests\n\n")
	for _, mapping := range serverHandlerMappings {
		b.WriteString(fmt.Sprintf("- `%s` handler=%s visibility=%s capability=%s unitTest=%s docs=%s exception=%s review=%s\n", mapping.Method, mapping.HandlerOwner, mapping.Visibility, mapping.Capability, mapping.UnitTestOwner, mapping.DocsExampleOwner, mapping.GeneratedOnlyException, mapping.ReviewNote))
	}
	b.WriteString("\n## Server Notifications\n\n")
	for _, entry := range manifest.Experimental.ServerNotifications {
		var domains []string
		for _, route := range entry.RoutingStrategy.Routes {
			domains = append(domains, route.ResourceDomain)
		}
		b.WriteString(fmt.Sprintf("- `%s` payload=%s visibility=%s routing=%s routeDomains=%s\n", entry.Method, entry.PayloadType, entry.SDKVisibility, entry.RoutingStrategy.Kind, strings.Join(domains, ",")))
	}
	b.WriteString("\n## Client Notifications\n\n")
	for _, entry := range manifest.Experimental.ClientNotifications {
		b.WriteString(fmt.Sprintf("- `%s` payload=%s visibility=%s\n", entry.Method, entry.PayloadType, entry.SDKVisibility))
	}
	return b.String()
}

func serverNotificationMethodsByOwner(manifest *Manifest) map[string][]string {
	byOwner := map[string][]string{}
	for _, entry := range manifest.Experimental.ServerNotifications {
		owners := routeOwners(entry)
		for _, owner := range owners {
			byOwner[owner] = appendUnique(byOwner[owner], entry.Method)
		}
	}
	for owner := range byOwner {
		sort.Strings(byOwner[owner])
	}
	return byOwner
}

func routeOwners(entry NotificationEntry) []string {
	owners := routedOwners(entry)
	for _, mapping := range serverNotificationResourceMappings {
		if mapping.Method != entry.Method {
			continue
		}
		for _, owner := range mapping.ResourceOwners {
			owners = appendUnique(owners, owner)
		}
	}
	return owners
}

func routedOwners(entry NotificationEntry) []string {
	var owners []string
	for _, route := range entry.RoutingStrategy.Routes {
		for _, owner := range routingDomainResourceOwners(route.ResourceDomain) {
			owners = appendUnique(owners, owner)
		}
	}
	return owners
}

func routingDomainResourceOwners(domain string) []string {
	switch domain {
	case "account":
		return []string{"Accounts"}
	case "command":
		return []string{"Commands"}
	case "externalAgentConfig":
		return []string{"ExternalAgents"}
	case "fs":
		return []string{"FileSystem"}
	case "fuzzyFileSearch":
		return []string{"FuzzyFileSearch"}
	case "hook":
		return []string{"Hooks"}
	case "mcpServer":
		return []string{"MCP"}
	case "model":
		return []string{"Models"}
	case "process":
		return []string{"Processes"}
	case "remoteControl":
		return []string{"RemoteControl"}
	case "thread":
		return []string{"Threads"}
	case "turn", "item", "rawResponseItem":
		return []string{"Turns"}
	case "error", "guardianWarning", "serverRequest", "warning":
		return []string{"Threads", "Turns"}
	default:
		return nil
	}
}

func serverHandlerCapabilitiesByOwner() map[string][]string {
	byOwner := map[string][]string{}
	for _, mapping := range serverHandlerMappings {
		for _, owner := range handlerOwners(mapping.HandlerOwner) {
			value := mapping.Method + "(" + mapping.Capability + ")"
			byOwner[owner] = appendUnique(byOwner[owner], value)
		}
	}
	for owner := range byOwner {
		sort.Strings(byOwner[owner])
	}
	return byOwner
}

func handlerOwners(handlerOwner string) []string {
	switch handlerOwner {
	case "ServerHandlers.Approvals", "ServerHandlers.Permissions", "ServerHandlers.UserInput", "ServerHandlers.DynamicTools":
		return []string{"Turns"}
	case "ServerHandlers.MCPElicitation":
		return []string{"MCP"}
	case "ServerHandlers.ChatGPTTokenRefresh", "ServerHandlers.Attestation":
		return []string{"Accounts"}
	case "ServerHandlers.CurrentTime":
		return []string{"Threads"}
	default:
		return nil
	}
}

func appendUnique(values []string, value string) []string {
	if value == "" {
		return values
	}
	for _, existing := range values {
		if existing == value {
			return values
		}
	}
	return append(values, value)
}
