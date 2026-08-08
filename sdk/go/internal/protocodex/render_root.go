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
