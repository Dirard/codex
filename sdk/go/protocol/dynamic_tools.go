package protocol

import (
	"encoding/json"
	"fmt"
)

type legacyDynamicToolSpec struct {
	Namespace       *string         `json:"namespace"`
	Name            string          `json:"name"`
	Description     string          `json:"description"`
	InputSchema     json.RawMessage `json:"inputSchema"`
	DeferLoading    *bool           `json:"deferLoading"`
	ExposeToContext *bool           `json:"exposeToContext"`
}

func normalizeDynamicToolSpecsJSON(raw json.RawMessage) ([]DynamicToolSpec, error) {
	var rawValues []json.RawMessage
	if err := json.Unmarshal(raw, &rawValues); err != nil {
		return nil, err
	}
	hasLegacyFormat, hasCanonicalFormat, err := detectDynamicToolSpecFormat(rawValues)
	if err != nil {
		return nil, err
	}
	if hasLegacyFormat && hasCanonicalFormat {
		return nil, fmt.Errorf("dynamic tools must use either canonical or legacy format consistently")
	}
	if !hasLegacyFormat {
		var tools []DynamicToolSpec
		if err := json.Unmarshal(raw, &tools); err != nil {
			return nil, err
		}
		return tools, nil
	}
	return normalizeLegacyDynamicToolSpecs(rawValues)
}

func detectDynamicToolSpecFormat(rawValues []json.RawMessage) (bool, bool, error) {
	hasLegacyFormat := false
	hasCanonicalFormat := false
	for _, rawValue := range rawValues {
		var fields map[string]json.RawMessage
		if err := json.Unmarshal(rawValue, &fields); err != nil {
			return false, false, err
		}
		if dynamicToolHasLegacyFields(fields) || dynamicToolNestedToolsHaveLegacyFields(fields) {
			hasLegacyFormat = true
		}
		if _, ok := fields["type"]; ok {
			hasCanonicalFormat = true
		}
	}
	return hasLegacyFormat, hasCanonicalFormat, nil
}

func dynamicToolHasLegacyFields(fields map[string]json.RawMessage) bool {
	if _, ok := fields["namespace"]; ok {
		return true
	}
	if _, ok := fields["exposeToContext"]; ok {
		return true
	}
	_, hasType := fields["type"]
	return !hasType
}

func dynamicToolNestedToolsHaveLegacyFields(fields map[string]json.RawMessage) bool {
	rawTools, ok := fields["tools"]
	if !ok {
		return false
	}
	var tools []map[string]json.RawMessage
	if err := json.Unmarshal(rawTools, &tools); err != nil {
		return false
	}
	for _, tool := range tools {
		if dynamicToolHasLegacyFields(tool) {
			return true
		}
	}
	return false
}

func normalizeLegacyDynamicToolSpecs(rawValues []json.RawMessage) ([]DynamicToolSpec, error) {
	var out []DynamicToolSpec
	namespaceIndices := map[string]int{}
	for _, rawValue := range rawValues {
		tool, err := decodeLegacyDynamicToolSpec(rawValue)
		if err != nil {
			return nil, err
		}
		namespaceTool := legacyDynamicToolToNamespaceTool(tool)
		if tool.Namespace == nil {
			out = append(out, DynamicToolSpec{
				DeferLoading: namespaceTool.DeferLoading,
				Description:  namespaceTool.Description,
				InputSchema:  namespaceTool.InputSchema,
				Name:         namespaceTool.Name,
				TypeValue:    "function",
			})
			continue
		}
		namespace := *tool.Namespace
		if index, ok := namespaceIndices[namespace]; ok {
			tools, _ := out[index].Tools.Value()
			tools = append(tools, namespaceTool)
			out[index].Tools = SomeNonNull(tools)
			continue
		}
		namespaceIndices[namespace] = len(out)
		out = append(out, DynamicToolSpec{
			Description: SomeNonNull(""),
			Name:        SomeNonNull(namespace),
			Tools:       SomeNonNull([]DynamicToolNamespaceTool{namespaceTool}),
			TypeValue:   "namespace",
		})
	}
	return out, nil
}

func decodeLegacyDynamicToolSpec(raw json.RawMessage) (legacyDynamicToolSpec, error) {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return legacyDynamicToolSpec{}, err
	}
	for _, required := range []string{"name", "description", "inputSchema"} {
		if _, ok := fields[required]; !ok {
			return legacyDynamicToolSpec{}, DecodeError{Field: required, Reason: "missing required field"}
		}
	}
	var tool legacyDynamicToolSpec
	if err := json.Unmarshal(raw, &tool); err != nil {
		return legacyDynamicToolSpec{}, err
	}
	return tool, nil
}

func legacyDynamicToolToNamespaceTool(tool legacyDynamicToolSpec) DynamicToolNamespaceTool {
	deferLoading := false
	if tool.DeferLoading != nil {
		deferLoading = *tool.DeferLoading
	} else if tool.ExposeToContext != nil {
		deferLoading = !*tool.ExposeToContext
	}
	return DynamicToolNamespaceTool{
		DeferLoading: SomeNonNull(deferLoading),
		Description:  SomeNonNull(tool.Description),
		InputSchema:  append(json.RawMessage(nil), tool.InputSchema...),
		Name:         SomeNonNull(tool.Name),
		TypeValue:    "function",
	}
}
