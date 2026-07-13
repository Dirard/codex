package protocodex

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

type protocolSchemaVariant struct {
	Method          string
	PayloadTypeName string
}

type protocolSchemaInventory struct {
	ClientRequests      map[string]protocolSchemaVariant
	ServerRequests      map[string]protocolSchemaVariant
	ServerNotifications map[string]protocolSchemaVariant
	ClientNotifications map[string]protocolSchemaVariant
}

type manifestProtocolPayload struct {
	TypeName string
	Excepted bool
}

func extractProtocolSchemaInventory(bundle *SchemaBundle) (protocolSchemaInventory, error) {
	clientRequests, err := extractProtocolSchemaUnion(bundle.V2Definitions, "flat v2", "ClientRequest", "params")
	if err != nil {
		return protocolSchemaInventory{}, err
	}
	serverRequests, err := extractProtocolSchemaUnion(bundle.Definitions, "complete", "ServerRequest", "params")
	if err != nil {
		return protocolSchemaInventory{}, err
	}
	serverNotifications, err := extractProtocolSchemaUnion(bundle.V2Definitions, "flat v2", "ServerNotification", "params")
	if err != nil {
		return protocolSchemaInventory{}, err
	}
	clientNotifications, err := extractProtocolSchemaUnion(bundle.Definitions, "complete", "ClientNotification", "params")
	if err != nil {
		return protocolSchemaInventory{}, err
	}
	return protocolSchemaInventory{
		ClientRequests:      clientRequests,
		ServerRequests:      serverRequests,
		ServerNotifications: serverNotifications,
		ClientNotifications: clientNotifications,
	}, nil
}

func extractProtocolSchemaUnion(definitions map[string]Schema, sourceLabel, definitionName, payloadField string) (map[string]protocolSchemaVariant, error) {
	schema, ok := definitions[definitionName]
	if !ok {
		return nil, fmt.Errorf("%s schema definition %s is missing", sourceLabel, definitionName)
	}
	variants := map[string]protocolSchemaVariant{}
	for _, variant := range schema.OneOf {
		method, err := schemaVariantMethod(definitionName, variant)
		if err != nil {
			return nil, err
		}
		if _, ok := variants[method]; ok {
			return nil, fmt.Errorf("%s schema has duplicate method %q", definitionName, method)
		}
		variants[method] = protocolSchemaVariant{
			Method:          method,
			PayloadTypeName: schemaPayloadTypeName(variant.Properties[payloadField]),
		}
	}
	return variants, nil
}

func schemaVariantMethod(definitionName string, variant Schema) (string, error) {
	methodSchema, ok := variant.Properties["method"]
	if !ok {
		return "", fmt.Errorf("%s schema variant has no method property", definitionName)
	}
	values, ok := stringEnumValues(methodSchema)
	if !ok || len(values) != 1 {
		return "", fmt.Errorf("%s schema variant has non-singleton method enum", definitionName)
	}
	return values[0], nil
}

func schemaPayloadTypeName(schema Schema) string {
	if ref, ok := schema.SingleRef(); ok {
		return typeNameForDefinition(schemaRefName(ref))
	}
	if ref, ok := schema.NullableRef(); ok {
		return typeNameForDefinition(schemaRefName(ref))
	}
	return ""
}

func validateProtocolSchemaManifestMode(modeName string, mode ManifestMode, bundle *SchemaBundle) error {
	inventory, err := extractProtocolSchemaInventory(bundle)
	if err != nil {
		return err
	}
	var problems []string
	compareSchemaManifestDirection(modeName, "ClientRequest", inventory.ClientRequests, clientRequestPayloadTypes(mode.ClientRequests), &problems)
	compareSchemaManifestDirection(modeName, "ServerRequest", inventory.ServerRequests, serverRequestPayloadTypes(mode.ServerRequests), &problems)
	compareSchemaManifestDirection(modeName, "ServerNotification", inventory.ServerNotifications, notificationPayloadTypes(mode.ServerNotifications), &problems)
	compareSchemaManifestDirection(modeName, "ClientNotification", inventory.ClientNotifications, notificationPayloadTypes(mode.ClientNotifications), &problems)
	if len(problems) > 0 {
		sort.Strings(problems)
		return fmt.Errorf("%s schema/manifest drift:\n- %s", modeName, strings.Join(problems, "\n- "))
	}
	return nil
}

func validateProtocolSchemaCanonicalManifestCoverage(label string, inventory protocolSchemaInventory, mode ManifestMode) error {
	var problems []string
	compareSchemaManifestCoverage(label, "ClientRequest", inventory.ClientRequests, clientRequestPayloadTypes(mode.ClientRequests), &problems)
	compareSchemaManifestCoverage(label, "ServerRequest", inventory.ServerRequests, serverRequestPayloadTypes(mode.ServerRequests), &problems)
	compareSchemaManifestCoverage(label, "ServerNotification", inventory.ServerNotifications, notificationPayloadTypes(mode.ServerNotifications), &problems)
	compareSchemaManifestCoverage(label, "ClientNotification", inventory.ClientNotifications, notificationPayloadTypes(mode.ClientNotifications), &problems)
	if len(problems) > 0 {
		sort.Strings(problems)
		return fmt.Errorf("%s schema/canonical manifest coverage drift:\n- %s", label, strings.Join(problems, "\n- "))
	}
	return nil
}

func validateStableProtocolSchemaManifestCoverage(label string, inventory protocolSchemaInventory, mode ManifestMode) error {
	var problems []string
	compareSchemaManifestCoverage(label, "ClientRequest", inventory.ClientRequests, clientRequestPayloadTypes(mode.ClientRequests), &problems)
	compareSchemaManifestCoverage(label, "ServerRequest", inventory.ServerRequests, serverRequestPayloadTypes(mode.ServerRequests), &problems)
	compareSchemaManifestCoverage(label, "ClientNotification", inventory.ClientNotifications, notificationPayloadTypes(mode.ClientNotifications), &problems)
	if len(problems) > 0 {
		sort.Strings(problems)
		return fmt.Errorf("%s stable schema/manifest coverage drift:\n- %s", label, strings.Join(problems, "\n- "))
	}
	return nil
}

func validateProtocolManifestSchemaSubset(label string, mode ManifestMode, inventory protocolSchemaInventory) error {
	var problems []string
	compareManifestSchemaSubset(label, "ClientRequest", clientRequestPayloadTypes(mode.ClientRequests), inventory.ClientRequests, &problems)
	compareManifestSchemaSubset(label, "ServerRequest", serverRequestPayloadTypes(mode.ServerRequests), inventory.ServerRequests, &problems)
	compareManifestSchemaSubset(label, "ServerNotification", notificationPayloadTypes(mode.ServerNotifications), inventory.ServerNotifications, &problems)
	compareManifestSchemaSubset(label, "ClientNotification", notificationPayloadTypes(mode.ClientNotifications), inventory.ClientNotifications, &problems)
	if len(problems) > 0 {
		sort.Strings(problems)
		return fmt.Errorf("%s manifest/schema subset drift:\n- %s", label, strings.Join(problems, "\n- "))
	}
	return nil
}

func validateReachableSerdeShapeCoverage(modeName string, mode ManifestMode, bundle *SchemaBundle) error {
	reachable, err := reachableSchemaDefinitions(mode, bundle)
	if err != nil {
		return err
	}
	keys := make([]string, 0, len(reachable))
	for key := range reachable {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	names := definitionNameMap(sortedDefinitionKeys(bundle))
	shapes := mapSerdeShapes(mode.SerdeShapes)
	var problems []string
	for _, key := range keys {
		name := names[key]
		if name == "" {
			name = typeNameForDefinition(key)
		}
		shape := serdeShapeForType(name, key, shapes)
		if shape.RustType == "" {
			problems = append(problems, fmt.Sprintf("reachable type %s (%s) has no serde shape metadata", name, key))
			continue
		}
		switch shape.MetadataStatus {
		case "manifestRequired":
		case "schemaSufficient":
			if shape.SchemaSufficientProof == nil || !shape.SchemaSufficientProof.Complete() {
				problems = append(problems, fmt.Sprintf("reachable type %s (%s) has incomplete schema sufficient proof", name, key))
			}
		default:
			problems = append(problems, fmt.Sprintf("reachable type %s (%s) has unsupported serde metadata status %q", name, key, shape.MetadataStatus))
		}
	}
	if len(problems) > 0 {
		sort.Strings(problems)
		return fmt.Errorf("%s reachable serde shape coverage drift:\n- %s", modeName, strings.Join(problems, "\n- "))
	}
	return nil
}

func reachableSchemaDefinitions(mode ManifestMode, bundle *SchemaBundle) (map[string]bool, error) {
	names := definitionNameMap(sortedDefinitionKeys(bundle))
	reachable := map[string]bool{}
	var problems []string
	var visitDefinition func(key string)
	var visitSchema func(schema Schema)
	visitRef := func(ref string) {
		key, ok := schemaDefinitionKeyForRef(ref, bundle, names)
		if !ok {
			problems = append(problems, fmt.Sprintf("schema ref %q is missing from schema bundle", ref))
			return
		}
		visitDefinition(key)
	}
	visitSchema = func(schema Schema) {
		if ref, ok := schema.SingleRef(); ok {
			visitRef(ref)
			return
		}
		if ref, ok := schema.NullableRef(); ok {
			visitRef(ref)
			return
		}
		for _, property := range schema.Properties {
			visitSchema(property)
		}
		if schema.Items != nil {
			visitSchema(*schema.Items)
		}
		if schema.AdditionalProperties != nil {
			visitSchema(*schema.AdditionalProperties)
		}
		for _, branch := range schema.AllOf {
			visitSchema(branch)
		}
		for _, branch := range schema.AnyOf {
			visitSchema(branch)
		}
		for _, branch := range schema.OneOf {
			visitSchema(branch)
		}
	}
	visitDefinition = func(key string) {
		if reachable[key] {
			return
		}
		schema, ok := bundle.Definition(key)
		if !ok {
			problems = append(problems, fmt.Sprintf("schema definition %q is missing from schema bundle", key))
			return
		}
		reachable[key] = true
		visitSchema(schema)
	}
	for _, typeName := range manifestReachableRootTypeNames(mode) {
		key, ok := schemaDefinitionKeyForManifestType(typeName, bundle, names)
		if !ok {
			problems = append(problems, fmt.Sprintf("manifest type %q is missing from schema bundle", typeName))
			continue
		}
		visitDefinition(key)
	}
	if len(problems) > 0 {
		sort.Strings(problems)
		return nil, fmt.Errorf("reachable schema definition drift:\n- %s", strings.Join(problems, "\n- "))
	}
	return reachable, nil
}

func manifestReachableRootTypeNames(mode ManifestMode) []string {
	var roots []string
	add := func(typeName string) {
		if typeName == "" || typeName == "Option<()>" {
			return
		}
		roots = append(roots, typeName)
	}
	addIfSchemaRefPresent := func(typeName, schemaRef string) {
		if schemaRef == "" {
			return
		}
		add(typeName)
	}
	for _, entry := range mode.ClientRequests {
		addIfSchemaRefPresent(entry.ParamsType, entry.ParamsSchemaRef)
		addIfSchemaRefPresent(entry.ResponseType, entry.ResponseSchemaRef)
	}
	for _, entry := range mode.ServerRequests {
		addIfSchemaRefPresent(entry.PayloadType, entry.ParamsSchemaRef)
		addIfSchemaRefPresent(entry.ResponseType, entry.ResponseSchemaRef)
	}
	for _, entry := range mode.ServerNotifications {
		addIfSchemaRefPresent(entry.PayloadType, entry.PayloadSchemaRef)
	}
	for _, entry := range mode.ClientNotifications {
		addIfSchemaRefPresent(entry.PayloadType, entry.PayloadSchemaRef)
	}
	return roots
}

func schemaDefinitionKeyForRef(ref string, bundle *SchemaBundle, names map[string]string) (string, bool) {
	return schemaDefinitionKeyForManifestType(schemaRefName(ref), bundle, names)
}

func schemaDefinitionKeyForManifestType(typeName string, bundle *SchemaBundle, names map[string]string) (string, bool) {
	typeName = strings.TrimPrefix(typeName, "#/definitions/")
	if typeName == "" || typeName == "Option<()>" {
		return "", false
	}
	candidates := []string{typeName}
	if !strings.HasPrefix(typeName, "v2/") {
		candidates = append(candidates, "v2/"+typeName)
	}
	if strings.HasPrefix(typeName, "Nullable") {
		base := strings.TrimPrefix(typeName, "Nullable")
		candidates = append(candidates, base, "v2/"+base)
	}
	for _, candidate := range candidates {
		if _, ok := bundle.Definition(candidate); ok {
			return candidate, true
		}
	}
	want := typeNameForDefinition(typeName)
	for key, name := range names {
		if name == want || typeNameForDefinition(key) == want {
			return key, true
		}
	}
	return "", false
}

func compareSchemaManifestDirection(modeName, schemaName string, schemaVariants map[string]protocolSchemaVariant, manifestTypes map[string]manifestProtocolPayload, problems *[]string) {
	compareSchemaManifestCoverage(modeName, schemaName, schemaVariants, manifestTypes, problems)
	compareManifestSchemaSubset(modeName, schemaName, manifestTypes, schemaVariants, problems)
}

func compareSchemaManifestCoverage(modeName, schemaName string, schemaVariants map[string]protocolSchemaVariant, manifestTypes map[string]manifestProtocolPayload, problems *[]string) {
	for method, variant := range schemaVariants {
		manifestPayload, ok := manifestTypes[method]
		if !ok {
			*problems = append(*problems, fmt.Sprintf("schema %s method %q missing from manifest", schemaName, method))
			continue
		}
		if variant.PayloadTypeName != "" && manifestPayload.TypeName != "" && !payloadTypeNamesEquivalent(variant.PayloadTypeName, manifestPayload.TypeName) {
			*problems = append(*problems, fmt.Sprintf("%s %s method %q schema payload %s != manifest payload %s", modeName, schemaName, method, variant.PayloadTypeName, manifestPayload.TypeName))
		}
	}
}

func compareManifestSchemaSubset(modeName, schemaName string, manifestTypes map[string]manifestProtocolPayload, schemaVariants map[string]protocolSchemaVariant, problems *[]string) {
	for method, payload := range manifestTypes {
		if _, ok := schemaVariants[method]; !ok {
			if payload.Excepted {
				continue
			}
			*problems = append(*problems, fmt.Sprintf("manifest %s method %q missing from schema %s", modeName, method, schemaName))
		}
	}
}

func validateProtocolSchemaInventorySubset(label string, subset, superset protocolSchemaInventory) error {
	var problems []string
	compareInventorySubset(label, "ClientRequest", subset.ClientRequests, superset.ClientRequests, &problems)
	compareInventorySubset(label, "ServerRequest", subset.ServerRequests, superset.ServerRequests, &problems)
	compareInventorySubset(label, "ServerNotification", subset.ServerNotifications, superset.ServerNotifications, &problems)
	compareInventorySubset(label, "ClientNotification", subset.ClientNotifications, superset.ClientNotifications, &problems)
	if len(problems) > 0 {
		sort.Strings(problems)
		return fmt.Errorf("%s schema inventory is not an experimental subset:\n- %s", label, strings.Join(problems, "\n- "))
	}
	return nil
}

func compareInventorySubset(label, schemaName string, subset, superset map[string]protocolSchemaVariant, problems *[]string) {
	for method, variant := range subset {
		superVariant, ok := superset[method]
		if !ok {
			*problems = append(*problems, fmt.Sprintf("%s %s method %q missing from experimental schema", label, schemaName, method))
			continue
		}
		if variant.PayloadTypeName != "" && superVariant.PayloadTypeName != "" && variant.PayloadTypeName != superVariant.PayloadTypeName {
			*problems = append(*problems, fmt.Sprintf("%s %s method %q payload %s != experimental payload %s", label, schemaName, method, variant.PayloadTypeName, superVariant.PayloadTypeName))
		}
	}
}

func clientRequestPayloadTypes(entries []ClientRequestEntry) map[string]manifestProtocolPayload {
	out := make(map[string]manifestProtocolPayload, len(entries))
	for _, entry := range entries {
		out[entry.Method] = manifestProtocolPayload{
			TypeName: manifestPayloadTypeName(entry.ParamsType),
			Excepted: manifestEntryExcepted(entry.Exception),
		}
	}
	return out
}

func serverRequestPayloadTypes(entries []ServerRequestEntry) map[string]manifestProtocolPayload {
	out := make(map[string]manifestProtocolPayload, len(entries))
	for _, entry := range entries {
		out[entry.Method] = manifestProtocolPayload{
			TypeName: manifestPayloadTypeName(entry.PayloadType),
			Excepted: manifestEntryExcepted(entry.Exception),
		}
	}
	return out
}

func notificationPayloadTypes(entries []NotificationEntry) map[string]manifestProtocolPayload {
	out := make(map[string]manifestProtocolPayload, len(entries))
	for _, entry := range entries {
		out[entry.Method] = manifestProtocolPayload{
			TypeName: manifestPayloadTypeName(entry.PayloadType),
			Excepted: manifestEntryExcepted(entry.Exception),
		}
	}
	return out
}

func manifestPayloadTypeName(typeName string) string {
	if typeName == "" || typeName == "Option<()>" {
		return ""
	}
	return typeNameForDefinition(typeName)
}

func payloadTypeNamesEquivalent(schemaType, manifestType string) bool {
	return schemaType == manifestType || "Nullable"+schemaType == manifestType || schemaType == "Nullable"+manifestType
}

func manifestEntryExcepted(raw json.RawMessage) bool {
	present, err := validateExceptionReviewJSON(raw, "manifest exception")
	return present && err == nil
}

func schemaRefName(ref string) string {
	return strings.TrimPrefix(ref, "#/definitions/")
}
