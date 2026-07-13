package protocodex

import (
	"bytes"
	"encoding/json"
	"fmt"
)

type protocolEntrySemantics struct {
	label                     string
	method                    string
	sdkVisibility             string
	serdeShapeRequirement     string
	exception                 json.RawMessage
	experimental              json.RawMessage
	experimentalFields        []ExperimentalField
	boundedModelContextFields []BoundedModelContextField
	manualPayloadConversion   *string
}

type experimentalMarker struct {
	Reason     string    `json:"reason"`
	FieldPaths *[]string `json:"fieldPaths"`
}

type experimentalDiscriminator struct {
	FieldPath string `json:"fieldPath"`
	WireValue string `json:"wireValue"`
}

type exceptionReview struct {
	Reason     string `json:"reason"`
	Owner      string `json:"owner"`
	ReviewNote string `json:"reviewNote"`
}

type serdeDefaultFunction struct {
	Function string `json:"function"`
}

func validateManifestV1Semantics(modeName, expectedProtocolMode string, mode ManifestMode) error {
	if mode.ProtocolMode != expectedProtocolMode {
		return fmt.Errorf("%s manifest protocolMode is %q, want %q", modeName, mode.ProtocolMode, expectedProtocolMode)
	}
	for _, entry := range mode.ClientRequests {
		if err := validateRequestEntrySemantics(modeName, protocolEntrySemantics{
			label:                     "client request",
			method:                    entry.Method,
			sdkVisibility:             entry.SDKVisibility,
			serdeShapeRequirement:     entry.SerdeShapeRequirement,
			exception:                 entry.Exception,
			experimental:              entry.Experimental,
			experimentalFields:        entry.ExperimentalFields,
			boundedModelContextFields: entry.BoundedModelContextFields,
			manualPayloadConversion:   entry.ManualPayloadConversion,
		}, entry.Retry); err != nil {
			return err
		}
		for index, scope := range entry.RequestSerializationScopes {
			label := fmt.Sprintf("%s client request %q requestSerializationScopes[%d]", modeName, entry.Method, index)
			if err := validateOptionalNonEmptyJSONString(scope.QueueKey, label+" queueKey"); err != nil {
				return err
			}
			if err := validateIdentityExtractors(label, scope.IdentityExtractors); err != nil {
				return err
			}
		}
	}
	for _, entry := range mode.ServerRequests {
		if err := validateRequestEntrySemantics(modeName, protocolEntrySemantics{
			label:                     "server request",
			method:                    entry.Method,
			sdkVisibility:             entry.SDKVisibility,
			serdeShapeRequirement:     entry.SerdeShapeRequirement,
			exception:                 entry.Exception,
			experimental:              entry.Experimental,
			experimentalFields:        entry.ExperimentalFields,
			boundedModelContextFields: entry.BoundedModelContextFields,
			manualPayloadConversion:   entry.ManualPayloadConversion,
		}, entry.Retry); err != nil {
			return err
		}
	}
	for _, entries := range []struct {
		label   string
		entries []NotificationEntry
	}{
		{label: "server notification", entries: mode.ServerNotifications},
		{label: "client notification", entries: mode.ClientNotifications},
	} {
		for _, entry := range entries.entries {
			if err := validateProtocolEntrySemantics(modeName, protocolEntrySemantics{
				label:                   entries.label,
				method:                  entry.Method,
				sdkVisibility:           entry.SDKVisibility,
				serdeShapeRequirement:   entry.SerdeShapeRequirement,
				exception:               entry.Exception,
				experimental:            entry.Experimental,
				experimentalFields:      entry.ExperimentalFields,
				manualPayloadConversion: entry.ManualPayloadConversion,
			}); err != nil {
				return err
			}
			for index, route := range entry.RoutingStrategy.Routes {
				label := fmt.Sprintf("%s %s %q routingStrategy.routes[%d]", modeName, entries.label, entry.Method, index)
				if route.ResourceDomain == "" || route.WireIdentitySource == "" {
					return fmt.Errorf("%s has incomplete resourceDomain or wireIdentitySource", label)
				}
				if err := validateIdentityExtractors(label, route.IdentityExtractors); err != nil {
					return err
				}
			}
		}
	}
	return validateSerdeShapeSemantics(modeName, mode.SerdeShapes)
}

func validateRequestEntrySemantics(modeName string, entry protocolEntrySemantics, retry string) error {
	if retry != "neverRetryAfterWrite" {
		return fmt.Errorf("%s %s %q has unknown retry policy %q", modeName, entry.label, entry.method, retry)
	}
	return validateProtocolEntrySemantics(modeName, entry)
}

func validateProtocolEntrySemantics(modeName string, entry protocolEntrySemantics) error {
	label := fmt.Sprintf("%s %s %q", modeName, entry.label, entry.method)
	if entry.method == "" {
		return fmt.Errorf("%s has empty method", label)
	}
	if !knownSDKVisibility(entry.sdkVisibility) {
		return fmt.Errorf("%s has unknown sdkVisibility %q", label, entry.sdkVisibility)
	}
	if !knownSerdeShapeRequirement(entry.serdeShapeRequirement) {
		return fmt.Errorf("%s has unknown serdeShapeRequirement %q", label, entry.serdeShapeRequirement)
	}
	excepted, err := validateExceptionReviewJSON(entry.exception, label+" exception")
	if err != nil {
		return err
	}
	if entry.sdkVisibility == "public" && excepted {
		return fmt.Errorf("%s public entry must not declare an exception", label)
	}
	if entry.sdkVisibility != "public" && !excepted {
		return fmt.Errorf("%s non-public entry must declare a complete exception", label)
	}
	if err := validateExperimentalMarkerJSON(entry.experimental, label+" experimental"); err != nil {
		return err
	}
	if err := validateExperimentalFields(label, entry.experimentalFields); err != nil {
		return err
	}
	for index, field := range entry.boundedModelContextFields {
		if field.Method == "" || field.FieldPath == "" || field.LimitProfile == "" {
			return fmt.Errorf("%s boundedModelContextFields[%d] is incomplete", label, index)
		}
		if field.Method != entry.method {
			return fmt.Errorf("%s boundedModelContextFields[%d] method is %q", label, index, field.Method)
		}
	}
	if err := validateManualPayloadConversion(modeName, entry.label, entry.method, entry.manualPayloadConversion); err != nil {
		return err
	}
	manualRequirement := entry.serdeShapeRequirement == "manualPayloadConversion"
	if manualRequirement != (entry.manualPayloadConversion != nil) {
		return fmt.Errorf("%s serdeShapeRequirement and manualPayloadConversion are inconsistent", label)
	}
	return nil
}

func knownSDKVisibility(value string) bool {
	switch value {
	case "public", "generatedOnly", "compatibilityOnly", "internalTestOnly", "handshakeOnly", "excluded":
		return true
	default:
		return false
	}
}

func knownSerdeShapeRequirement(value string) bool {
	switch value {
	case "schemaSufficient", "manifestRequired", "manualPayloadConversion":
		return true
	default:
		return false
	}
}

func validateExceptionReviewJSON(raw json.RawMessage, label string) (bool, error) {
	var review exceptionReview
	present, err := decodeOptionalStrictMetadata(raw, label, &review)
	if err != nil || !present {
		return present, err
	}
	if review.Reason == "" || review.Owner == "" || review.ReviewNote == "" {
		return true, fmt.Errorf("%s must contain non-empty reason, owner, and reviewNote", label)
	}
	return true, nil
}

func validateExperimentalMarkerJSON(raw json.RawMessage, label string) error {
	var marker experimentalMarker
	present, err := decodeOptionalStrictMetadata(raw, label, &marker)
	if err != nil || !present {
		return err
	}
	if marker.Reason == "" || marker.FieldPaths == nil {
		return fmt.Errorf("%s must contain a non-empty reason and fieldPaths", label)
	}
	for index, fieldPath := range *marker.FieldPaths {
		if fieldPath == "" {
			return fmt.Errorf("%s fieldPaths[%d] is empty", label, index)
		}
	}
	return nil
}

func validateExperimentalFields(label string, fields []ExperimentalField) error {
	for index, field := range fields {
		fieldLabel := fmt.Sprintf("%s experimentalFields[%d]", label, index)
		if field.ContainingType == "" || field.FieldPath == "" || field.Reason == "" {
			return fmt.Errorf("%s is incomplete", fieldLabel)
		}
		var discriminator experimentalDiscriminator
		present, err := decodeOptionalStrictMetadata(field.Discriminator, fieldLabel+" discriminator", &discriminator)
		if err != nil {
			return err
		}
		if present && (discriminator.FieldPath == "" || discriminator.WireValue == "") {
			return fmt.Errorf("%s discriminator must contain non-empty fieldPath and wireValue", fieldLabel)
		}
	}
	return nil
}

func validateIdentityExtractors(label string, extractors []IdentityExtractor) error {
	for index, extractor := range extractors {
		extractorLabel := fmt.Sprintf("%s identityExtractors[%d]", label, index)
		if extractor.FieldPath == "" || extractor.IdentityName == "" {
			return fmt.Errorf("%s is incomplete", extractorLabel)
		}
		if err := validateOptionalNonEmptyJSONString(extractor.TerminalPredicate, extractorLabel+" terminalPredicate"); err != nil {
			return err
		}
	}
	return nil
}

func validateSerdeShapeSemantics(modeName string, shapes []SerdeShape) error {
	for _, shape := range shapes {
		label := fmt.Sprintf("%s serde shape %q", modeName, shape.RustType)
		if shape.RustType == "" {
			return fmt.Errorf("%s has empty rustType", label)
		}
		if shape.MetadataStatus == "manifestRequired" && shape.SchemaSufficientProof != nil {
			return fmt.Errorf("%s manifestRequired shape must not declare schemaSufficientProof", label)
		}
		if err := validateManualPayloadConversion(modeName, "serde shape", shape.RustType, shape.ManualPayloadConversion); err != nil {
			return err
		}
		if shape.ReviewNote != nil && *shape.ReviewNote == "" {
			return fmt.Errorf("%s reviewNote must not be empty", label)
		}
		for fieldIndex, field := range shape.Fields {
			fieldLabel := fmt.Sprintf("%s fields[%d]", label, fieldIndex)
			if field.RustField == "" || field.WireName == "" {
				return fmt.Errorf("%s has empty rustField or wireName", fieldLabel)
			}
			if !knownSerdePresence(field.Shape.Presence) {
				return fmt.Errorf("%s has unknown presence %q", fieldLabel, field.Shape.Presence)
			}
			for aliasIndex, alias := range field.Aliases {
				if alias == "" {
					return fmt.Errorf("%s aliases[%d] is empty", fieldLabel, aliasIndex)
				}
			}
			if field.Shape.Default != nil {
				if err := validateSerdeDefault(fieldLabel, *field.Shape.Default); err != nil {
					return err
				}
			}
		}
		for aliasIndex, alias := range shape.VariantAliases {
			aliasLabel := fmt.Sprintf("%s variantAliases[%d]", label, aliasIndex)
			if alias.RustVariant == "" || alias.CanonicalWireValue == "" {
				return fmt.Errorf("%s is incomplete", aliasLabel)
			}
			for valueIndex, value := range alias.Aliases {
				if value == "" {
					return fmt.Errorf("%s aliases[%d] is empty", aliasLabel, valueIndex)
				}
			}
		}
	}
	return nil
}

func knownSerdePresence(value string) bool {
	switch value {
	case "required", "optionalNonNull", "optionalNullable", "doubleOption":
		return true
	default:
		return false
	}
}

func validateSerdeDefault(label string, value SerdeDefault) error {
	if value.WireValueJSON == "" || !json.Valid([]byte(value.WireValueJSON)) {
		return fmt.Errorf("%s default wireValueJson must contain valid JSON", label)
	}
	var provider string
	if err := json.Unmarshal(value.Provider, &provider); err == nil {
		if provider != "serdeDefault" {
			return fmt.Errorf("%s default provider %q is unknown", label, provider)
		}
		return nil
	}
	var function serdeDefaultFunction
	present, err := decodeOptionalStrictMetadata(value.Provider, label+" default provider", &function)
	if err != nil {
		return err
	}
	if !present || function.Function == "" {
		return fmt.Errorf("%s default provider must be serdeDefault or a non-empty function", label)
	}
	return nil
}

func validateOptionalNonEmptyJSONString(raw json.RawMessage, label string) error {
	if rawJSONEmptyOrNull(raw) {
		return nil
	}
	var value string
	if err := json.Unmarshal(raw, &value); err != nil {
		return fmt.Errorf("%s must be a string or null: %w", label, err)
	}
	if value == "" {
		return fmt.Errorf("%s must not be empty", label)
	}
	return nil
}

func decodeOptionalStrictMetadata(raw json.RawMessage, label string, target any) (bool, error) {
	if rawJSONEmptyOrNull(raw) {
		return false, nil
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return true, fmt.Errorf("%s is invalid: %w", label, err)
	}
	if err := ensureJSONEOF(decoder); err != nil {
		return true, fmt.Errorf("%s is invalid: %w", label, err)
	}
	return true, nil
}

func validateModelContextLimits(limits ModelContextLimits) error {
	values := []struct {
		name  string
		value int
	}{
		{name: "maxAdditionalContextEntries", value: limits.MaxAdditionalContextEntries},
		{name: "maxAdditionalContextKeyBytes", value: limits.MaxAdditionalContextKeyBytes},
		{name: "maxAdditionalContextValueBytes", value: limits.MaxAdditionalContextValueBytes},
		{name: "maxAdditionalContextTotalBytes", value: limits.MaxAdditionalContextTotalBytes},
	}
	for _, value := range values {
		if value.value <= 0 {
			return fmt.Errorf("modelContextLimits.%s must be positive", value.name)
		}
	}
	return nil
}
