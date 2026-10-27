package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
)

type DecodeError struct {
	Field  string
	Reason string
}

func (e DecodeError) Error() string { return fmt.Sprintf("field %s: %s", e.Field, e.Reason) }

type ActiveProtocolMode string

const (
	ActiveProtocolModeStable       ActiveProtocolMode = "stable"
	ActiveProtocolModeExperimental ActiveProtocolMode = "experimental"
)

type AdditionalPermissionProfile struct {
	FileSystem Optional[AdditionalFileSystemPermissions] `json:"fileSystem,omitzero"`
	Network    Optional[AdditionalNetworkPermissions]    `json:"network,omitzero"`
}

var additionalPermissionProfileSpec = objectSpec{
	fields: []objectField{
		{name: "fileSystem"},
		{name: "network"},
	},
}

func (v AdditionalPermissionProfile) MarshalJSON() ([]byte, error) {
	type wire AdditionalPermissionProfile
	return json.Marshal(wire(v))
}

func (v *AdditionalPermissionProfile) UnmarshalJSON(data []byte) error {
	type wire AdditionalPermissionProfile
	return decodeObject(data, &additionalPermissionProfileSpec, (*wire)(v))
}

type AttestationGenerateParams map[string]json.RawMessage

func (v AttestationGenerateParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *AttestationGenerateParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = AttestationGenerateParams(raw)
	return nil
}

type AttestationGenerateResponse struct {
	Token string `json:"token"`
}

var attestationGenerateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "token", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AttestationGenerateResponse) MarshalJSON() ([]byte, error) {
	type wire AttestationGenerateResponse
	return json.Marshal(wire(v))
}

func (v *AttestationGenerateResponse) UnmarshalJSON(data []byte) error {
	type wire AttestationGenerateResponse
	return decodeObject(data, &attestationGenerateResponseSpec, (*wire)(v))
}

type ChatgptAuthTokensRefreshParams struct {
	PreviousAccountID Optional[string]               `json:"previousAccountId,omitzero"`
	Reason            ChatgptAuthTokensRefreshReason `json:"reason"`
}

var chatgptAuthTokensRefreshParamsSpec = objectSpec{
	fields: []objectField{
		{name: "previousAccountId"},
		{name: "reason", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ChatgptAuthTokensRefreshParams) MarshalJSON() ([]byte, error) {
	type wire ChatgptAuthTokensRefreshParams
	return json.Marshal(wire(v))
}

func (v *ChatgptAuthTokensRefreshParams) UnmarshalJSON(data []byte) error {
	type wire ChatgptAuthTokensRefreshParams
	return decodeObject(data, &chatgptAuthTokensRefreshParamsSpec, (*wire)(v))
}

type ChatgptAuthTokensRefreshReason string

const (
	ChatgptAuthTokensRefreshReasonUnauthorized ChatgptAuthTokensRefreshReason = "unauthorized"
)

type ChatgptAuthTokensRefreshResponse struct {
	AccessToken      string           `json:"accessToken"`
	ChatgptAccountID string           `json:"chatgptAccountId"`
	ChatgptPlanType  Optional[string] `json:"chatgptPlanType,omitzero"`
}

var chatgptAuthTokensRefreshResponseSpec = objectSpec{
	fields: []objectField{
		{name: "accessToken", flags: fieldRequired | fieldRejectNull},
		{name: "chatgptAccountId", flags: fieldRequired | fieldRejectNull},
		{name: "chatgptPlanType"},
	},
}

func (v ChatgptAuthTokensRefreshResponse) MarshalJSON() ([]byte, error) {
	type wire ChatgptAuthTokensRefreshResponse
	return json.Marshal(wire(v))
}

func (v *ChatgptAuthTokensRefreshResponse) UnmarshalJSON(data []byte) error {
	type wire ChatgptAuthTokensRefreshResponse
	return decodeObject(data, &chatgptAuthTokensRefreshResponseSpec, (*wire)(v))
}

type ClientInfo struct {
	Name    string           `json:"name"`
	Title   Optional[string] `json:"title,omitzero"`
	Version string           `json:"version"`
}

var clientInfoSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
		{name: "version", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ClientInfo) MarshalJSON() ([]byte, error) {
	type wire ClientInfo
	return json.Marshal(wire(v))
}

func (v *ClientInfo) UnmarshalJSON(data []byte) error {
	type wire ClientInfo
	return decodeObject(data, &clientInfoSpec, (*wire)(v))
}

type ClientRequest struct {
	ID     RequestID       `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params,omitempty"`
}

func (v ClientRequest) MarshalJSON() ([]byte, error) {
	type wire struct {
		ID     RequestID       `json:"id"`
		Method string          `json:"method"`
		Params json.RawMessage `json:"params,omitempty"`
	}
	return json.Marshal(wire{ID: v.ID, Method: v.Method, Params: v.Params})
}

func (v *ClientRequest) UnmarshalJSON(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	rawID, ok := raw["id"]
	if !ok {
		return DecodeError{Field: "id", Reason: "missing required field"}
	}
	if err := json.Unmarshal(rawID, &v.ID); err != nil {
		return fmt.Errorf("field id: %w", err)
	}
	rawMethod, ok := raw["method"]
	if !ok {
		return DecodeError{Field: "method", Reason: "missing required field"}
	}
	if bytes.Equal(rawMethod, []byte("null")) {
		return DecodeError{Field: "method", Reason: "cannot be null"}
	}
	if err := json.Unmarshal(rawMethod, &v.Method); err != nil {
		return fmt.Errorf("field method: %w", err)
	}
	v.Params = nil
	if rawParams, ok := raw["params"]; ok {
		v.Params = append(v.Params[:0], rawParams...)
	}
	return nil
}

func (v ClientRequest) InitializeParams() (InitializeParams, bool, error) {
	if v.Method != "initialize" {
		return InitializeParams{}, false, nil
	}
	var params InitializeParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ServerDiagnosticsParams() (ServerDiagnosticsParams, bool, error) {
	if v.Method != "server/diagnostics" {
		return ServerDiagnosticsParams{}, false, nil
	}
	var params ServerDiagnosticsParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) UserVerificationStatusParams() (UserVerificationStatusParams, bool, error) {
	if v.Method != "userVerification/status" {
		return UserVerificationStatusParams{}, false, nil
	}
	var params UserVerificationStatusParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) UserVerificationEnrollParams() (UserVerificationEnrollParams, bool, error) {
	if v.Method != "userVerification/enroll" {
		return UserVerificationEnrollParams{}, false, nil
	}
	var params UserVerificationEnrollParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) UserVerificationDeleteParams() (UserVerificationDeleteParams, bool, error) {
	if v.Method != "userVerification/delete" {
		return UserVerificationDeleteParams{}, false, nil
	}
	var params UserVerificationDeleteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) UserVerificationVerifyParams() (UserVerificationVerifyParams, bool, error) {
	if v.Method != "userVerification/verify" {
		return UserVerificationVerifyParams{}, false, nil
	}
	var params UserVerificationVerifyParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) UserVerificationCancelParams() (UserVerificationCancelParams, bool, error) {
	if v.Method != "userVerification/cancel" {
		return UserVerificationCancelParams{}, false, nil
	}
	var params UserVerificationCancelParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadStartParams() (ThreadStartParams, bool, error) {
	if v.Method != "thread/start" {
		return ThreadStartParams{}, false, nil
	}
	var params ThreadStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadResumeParams() (ThreadResumeParams, bool, error) {
	if v.Method != "thread/resume" {
		return ThreadResumeParams{}, false, nil
	}
	var params ThreadResumeParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadForkParams() (ThreadForkParams, bool, error) {
	if v.Method != "thread/fork" {
		return ThreadForkParams{}, false, nil
	}
	var params ThreadForkParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSampleParams() (ThreadSampleParams, bool, error) {
	if v.Method != "thread/sample" {
		return ThreadSampleParams{}, false, nil
	}
	var params ThreadSampleParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSampleCancelParams() (ThreadSampleCancelParams, bool, error) {
	if v.Method != "thread/sample/cancel" {
		return ThreadSampleCancelParams{}, false, nil
	}
	var params ThreadSampleCancelParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadArchiveParams() (ThreadArchiveParams, bool, error) {
	if v.Method != "thread/archive" {
		return ThreadArchiveParams{}, false, nil
	}
	var params ThreadArchiveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadDeleteParams() (ThreadDeleteParams, bool, error) {
	if v.Method != "thread/delete" {
		return ThreadDeleteParams{}, false, nil
	}
	var params ThreadDeleteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadUnsubscribeParams() (ThreadUnsubscribeParams, bool, error) {
	if v.Method != "thread/unsubscribe" {
		return ThreadUnsubscribeParams{}, false, nil
	}
	var params ThreadUnsubscribeParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadIncrementElicitationParams() (ThreadIncrementElicitationParams, bool, error) {
	if v.Method != "thread/increment_elicitation" {
		return ThreadIncrementElicitationParams{}, false, nil
	}
	var params ThreadIncrementElicitationParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadDecrementElicitationParams() (ThreadDecrementElicitationParams, bool, error) {
	if v.Method != "thread/decrement_elicitation" {
		return ThreadDecrementElicitationParams{}, false, nil
	}
	var params ThreadDecrementElicitationParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadNameSetParams() (ThreadSetNameParams, bool, error) {
	if v.Method != "thread/name/set" {
		return ThreadSetNameParams{}, false, nil
	}
	var params ThreadSetNameParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadPredictionRequestParams() (ThreadPredictionRequestParams, bool, error) {
	if v.Method != "thread/prediction/request" {
		return ThreadPredictionRequestParams{}, false, nil
	}
	var params ThreadPredictionRequestParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadGoalSetParams() (ThreadGoalSetParams, bool, error) {
	if v.Method != "thread/goal/set" {
		return ThreadGoalSetParams{}, false, nil
	}
	var params ThreadGoalSetParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadGoalGetParams() (ThreadGoalGetParams, bool, error) {
	if v.Method != "thread/goal/get" {
		return ThreadGoalGetParams{}, false, nil
	}
	var params ThreadGoalGetParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadGoalClearParams() (ThreadGoalClearParams, bool, error) {
	if v.Method != "thread/goal/clear" {
		return ThreadGoalClearParams{}, false, nil
	}
	var params ThreadGoalClearParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadQueueAddParams() (ThreadQueueAddParams, bool, error) {
	if v.Method != "thread/queue/add" {
		return ThreadQueueAddParams{}, false, nil
	}
	var params ThreadQueueAddParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadQueueListParams() (ThreadQueueListParams, bool, error) {
	if v.Method != "thread/queue/list" {
		return ThreadQueueListParams{}, false, nil
	}
	var params ThreadQueueListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadQueueUpdateParams() (ThreadQueueUpdateParams, bool, error) {
	if v.Method != "thread/queue/update" {
		return ThreadQueueUpdateParams{}, false, nil
	}
	var params ThreadQueueUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadQueueDeleteParams() (ThreadQueueDeleteParams, bool, error) {
	if v.Method != "thread/queue/delete" {
		return ThreadQueueDeleteParams{}, false, nil
	}
	var params ThreadQueueDeleteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadQueueReorderParams() (ThreadQueueReorderParams, bool, error) {
	if v.Method != "thread/queue/reorder" {
		return ThreadQueueReorderParams{}, false, nil
	}
	var params ThreadQueueReorderParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadQueueStartParams() (ThreadQueueStartParams, bool, error) {
	if v.Method != "thread/queue/start" {
		return ThreadQueueStartParams{}, false, nil
	}
	var params ThreadQueueStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadWorkflowListParams() (ThreadWorkflowListParams, bool, error) {
	if v.Method != "thread/workflow/list" {
		return ThreadWorkflowListParams{}, false, nil
	}
	var params ThreadWorkflowListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadWorkflowReadParams() (ThreadWorkflowReadParams, bool, error) {
	if v.Method != "thread/workflow/read" {
		return ThreadWorkflowReadParams{}, false, nil
	}
	var params ThreadWorkflowReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadWorkflowCancelParams() (ThreadWorkflowCancelParams, bool, error) {
	if v.Method != "thread/workflow/cancel" {
		return ThreadWorkflowCancelParams{}, false, nil
	}
	var params ThreadWorkflowCancelParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadBackgroundTerminalsPendingParams() (ThreadBackgroundTerminalsPendingParams, bool, error) {
	if v.Method != "thread/backgroundTerminals/pending" {
		return ThreadBackgroundTerminalsPendingParams{}, false, nil
	}
	var params ThreadBackgroundTerminalsPendingParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadMetadataUpdateParams() (ThreadMetadataUpdateParams, bool, error) {
	if v.Method != "thread/metadata/update" {
		return ThreadMetadataUpdateParams{}, false, nil
	}
	var params ThreadMetadataUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadAttachmentAddParams() (ThreadAttachmentAddParams, bool, error) {
	if v.Method != "thread/attachment/add" {
		return ThreadAttachmentAddParams{}, false, nil
	}
	var params ThreadAttachmentAddParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadAttachmentListParams() (ThreadAttachmentListParams, bool, error) {
	if v.Method != "thread/attachment/list" {
		return ThreadAttachmentListParams{}, false, nil
	}
	var params ThreadAttachmentListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadAttachmentOwnerListParams() (ThreadAttachmentOwnerListParams, bool, error) {
	if v.Method != "thread/attachmentOwner/list" {
		return ThreadAttachmentOwnerListParams{}, false, nil
	}
	var params ThreadAttachmentOwnerListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadAttachmentRemoveParams() (ThreadAttachmentRemoveParams, bool, error) {
	if v.Method != "thread/attachment/remove" {
		return ThreadAttachmentRemoveParams{}, false, nil
	}
	var params ThreadAttachmentRemoveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSectionMoveParams() (ThreadSectionMoveParams, bool, error) {
	if v.Method != "thread/section/move" {
		return ThreadSectionMoveParams{}, false, nil
	}
	var params ThreadSectionMoveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSettingsUpdateParams() (ThreadSettingsUpdateParams, bool, error) {
	if v.Method != "thread/settings/update" {
		return ThreadSettingsUpdateParams{}, false, nil
	}
	var params ThreadSettingsUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadMemoryModeSetParams() (ThreadMemoryModeSetParams, bool, error) {
	if v.Method != "thread/memoryMode/set" {
		return ThreadMemoryModeSetParams{}, false, nil
	}
	var params ThreadMemoryModeSetParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadMemoryReadParams() (ThreadMemoryReadParams, bool, error) {
	if v.Method != "thread/memory/read" {
		return ThreadMemoryReadParams{}, false, nil
	}
	var params ThreadMemoryReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) MemoryStatusParams() (MemoryStatusParams, bool, error) {
	if v.Method != "memory/status" {
		return MemoryStatusParams{}, false, nil
	}
	var params MemoryStatusParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadUnarchiveParams() (ThreadUnarchiveParams, bool, error) {
	if v.Method != "thread/unarchive" {
		return ThreadUnarchiveParams{}, false, nil
	}
	var params ThreadUnarchiveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadCompactStartParams() (ThreadCompactStartParams, bool, error) {
	if v.Method != "thread/compact/start" {
		return ThreadCompactStartParams{}, false, nil
	}
	var params ThreadCompactStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadShellCommandParams() (ThreadShellCommandParams, bool, error) {
	if v.Method != "thread/shellCommand" {
		return ThreadShellCommandParams{}, false, nil
	}
	var params ThreadShellCommandParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadApproveGuardianDeniedActionParams() (ThreadApproveGuardianDeniedActionParams, bool, error) {
	if v.Method != "thread/approveGuardianDeniedAction" {
		return ThreadApproveGuardianDeniedActionParams{}, false, nil
	}
	var params ThreadApproveGuardianDeniedActionParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadBackgroundTerminalsCleanParams() (ThreadBackgroundTerminalsCleanParams, bool, error) {
	if v.Method != "thread/backgroundTerminals/clean" {
		return ThreadBackgroundTerminalsCleanParams{}, false, nil
	}
	var params ThreadBackgroundTerminalsCleanParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadBackgroundTerminalsListParams() (ThreadBackgroundTerminalsListParams, bool, error) {
	if v.Method != "thread/backgroundTerminals/list" {
		return ThreadBackgroundTerminalsListParams{}, false, nil
	}
	var params ThreadBackgroundTerminalsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadBackgroundTerminalsTerminateParams() (ThreadBackgroundTerminalsTerminateParams, bool, error) {
	if v.Method != "thread/backgroundTerminals/terminate" {
		return ThreadBackgroundTerminalsTerminateParams{}, false, nil
	}
	var params ThreadBackgroundTerminalsTerminateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRevertParams() (ThreadRevertParams, bool, error) {
	if v.Method != "thread/revert" {
		return ThreadRevertParams{}, false, nil
	}
	var params ThreadRevertParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadListParams() (ThreadListParams, bool, error) {
	if v.Method != "thread/list" {
		return ThreadListParams{}, false, nil
	}
	var params ThreadListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectListParams() (ProjectListParams, bool, error) {
	if v.Method != "project/list" {
		return ProjectListParams{}, false, nil
	}
	var params ProjectListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectReadParams() (ProjectReadParams, bool, error) {
	if v.Method != "project/read" {
		return ProjectReadParams{}, false, nil
	}
	var params ProjectReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectCreateParams() (ProjectCreateParams, bool, error) {
	if v.Method != "project/create" {
		return ProjectCreateParams{}, false, nil
	}
	var params ProjectCreateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectImportParams() (ProjectImportParams, bool, error) {
	if v.Method != "project/import" {
		return ProjectImportParams{}, false, nil
	}
	var params ProjectImportParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectUpdateParams() (ProjectUpdateParams, bool, error) {
	if v.Method != "project/update" {
		return ProjectUpdateParams{}, false, nil
	}
	var params ProjectUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectMoveParams() (ProjectMoveParams, bool, error) {
	if v.Method != "project/move" {
		return ProjectMoveParams{}, false, nil
	}
	var params ProjectMoveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProjectDeleteParams() (ProjectDeleteParams, bool, error) {
	if v.Method != "project/delete" {
		return ProjectDeleteParams{}, false, nil
	}
	var params ProjectDeleteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSectionListParams() (ThreadSectionListParams, bool, error) {
	if v.Method != "threadSection/list" {
		return ThreadSectionListParams{}, false, nil
	}
	var params ThreadSectionListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSectionCreateParams() (ThreadSectionCreateParams, bool, error) {
	if v.Method != "threadSection/create" {
		return ThreadSectionCreateParams{}, false, nil
	}
	var params ThreadSectionCreateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSectionUpdateParams() (ThreadSectionUpdateParams, bool, error) {
	if v.Method != "threadSection/update" {
		return ThreadSectionUpdateParams{}, false, nil
	}
	var params ThreadSectionUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSectionDeleteParams() (ThreadSectionDeleteParams, bool, error) {
	if v.Method != "threadSection/delete" {
		return ThreadSectionDeleteParams{}, false, nil
	}
	var params ThreadSectionDeleteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSearchParams() (ThreadSearchParams, bool, error) {
	if v.Method != "thread/search" {
		return ThreadSearchParams{}, false, nil
	}
	var params ThreadSearchParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadSearchOccurrencesParams() (ThreadSearchOccurrencesParams, bool, error) {
	if v.Method != "thread/searchOccurrences" {
		return ThreadSearchOccurrencesParams{}, false, nil
	}
	var params ThreadSearchOccurrencesParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadLoadedListParams() (ThreadLoadedListParams, bool, error) {
	if v.Method != "thread/loaded/list" {
		return ThreadLoadedListParams{}, false, nil
	}
	var params ThreadLoadedListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadReadParams() (ThreadReadParams, bool, error) {
	if v.Method != "thread/read" {
		return ThreadReadParams{}, false, nil
	}
	var params ThreadReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadReadStateUpdateParams() (ThreadReadStateUpdateParams, bool, error) {
	if v.Method != "thread/readState/update" {
		return ThreadReadStateUpdateParams{}, false, nil
	}
	var params ThreadReadStateUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadTurnsListParams() (ThreadTurnsListParams, bool, error) {
	if v.Method != "thread/turns/list" {
		return ThreadTurnsListParams{}, false, nil
	}
	var params ThreadTurnsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadItemsListParams() (ThreadItemsListParams, bool, error) {
	if v.Method != "thread/items/list" {
		return ThreadItemsListParams{}, false, nil
	}
	var params ThreadItemsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadItemsReadParams() (ThreadItemsReadParams, bool, error) {
	if v.Method != "thread/items/read" {
		return ThreadItemsReadParams{}, false, nil
	}
	var params ThreadItemsReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadInjectItemsParams() (ThreadInjectItemsParams, bool, error) {
	if v.Method != "thread/inject_items" {
		return ThreadInjectItemsParams{}, false, nil
	}
	var params ThreadInjectItemsParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) SkillsListParams() (SkillsListParams, bool, error) {
	if v.Method != "skills/list" {
		return SkillsListParams{}, false, nil
	}
	var params SkillsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) SkillsExtraRootsSetParams() (SkillsExtraRootsSetParams, bool, error) {
	if v.Method != "skills/extraRoots/set" {
		return SkillsExtraRootsSetParams{}, false, nil
	}
	var params SkillsExtraRootsSetParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) HooksListParams() (HooksListParams, bool, error) {
	if v.Method != "hooks/list" {
		return HooksListParams{}, false, nil
	}
	var params HooksListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) MarketplaceAddParams() (MarketplaceAddParams, bool, error) {
	if v.Method != "marketplace/add" {
		return MarketplaceAddParams{}, false, nil
	}
	var params MarketplaceAddParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) MarketplaceRemoveParams() (MarketplaceRemoveParams, bool, error) {
	if v.Method != "marketplace/remove" {
		return MarketplaceRemoveParams{}, false, nil
	}
	var params MarketplaceRemoveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) MarketplaceUpgradeParams() (MarketplaceUpgradeParams, bool, error) {
	if v.Method != "marketplace/upgrade" {
		return MarketplaceUpgradeParams{}, false, nil
	}
	var params MarketplaceUpgradeParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginListParams() (PluginListParams, bool, error) {
	if v.Method != "plugin/list" {
		return PluginListParams{}, false, nil
	}
	var params PluginListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginSearchParams() (PluginSearchParams, bool, error) {
	if v.Method != "plugin/search" {
		return PluginSearchParams{}, false, nil
	}
	var params PluginSearchParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginInstalledParams() (PluginInstalledParams, bool, error) {
	if v.Method != "plugin/installed" {
		return PluginInstalledParams{}, false, nil
	}
	var params PluginInstalledParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginReconcileParams() (PluginReconcileParams, bool, error) {
	if v.Method != "plugin/reconcile" {
		return PluginReconcileParams{}, false, nil
	}
	var params PluginReconcileParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginReadParams() (PluginReadParams, bool, error) {
	if v.Method != "plugin/read" {
		return PluginReadParams{}, false, nil
	}
	var params PluginReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginSkillReadParams() (PluginSkillReadParams, bool, error) {
	if v.Method != "plugin/skill/read" {
		return PluginSkillReadParams{}, false, nil
	}
	var params PluginSkillReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginShareSaveParams() (PluginShareSaveParams, bool, error) {
	if v.Method != "plugin/share/save" {
		return PluginShareSaveParams{}, false, nil
	}
	var params PluginShareSaveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginShareUpdateTargetsParams() (PluginShareUpdateTargetsParams, bool, error) {
	if v.Method != "plugin/share/updateTargets" {
		return PluginShareUpdateTargetsParams{}, false, nil
	}
	var params PluginShareUpdateTargetsParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginShareListParams() (PluginShareListParams, bool, error) {
	if v.Method != "plugin/share/list" {
		return PluginShareListParams{}, false, nil
	}
	var params PluginShareListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginShareCheckoutParams() (PluginShareCheckoutParams, bool, error) {
	if v.Method != "plugin/share/checkout" {
		return PluginShareCheckoutParams{}, false, nil
	}
	var params PluginShareCheckoutParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginShareDeleteParams() (PluginShareDeleteParams, bool, error) {
	if v.Method != "plugin/share/delete" {
		return PluginShareDeleteParams{}, false, nil
	}
	var params PluginShareDeleteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AppReadParams() (AppsReadParams, bool, error) {
	if v.Method != "app/read" {
		return AppsReadParams{}, false, nil
	}
	var params AppsReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AppListParams() (AppsListParams, bool, error) {
	if v.Method != "app/list" {
		return AppsListParams{}, false, nil
	}
	var params AppsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AppInstalledParams() (AppsInstalledParams, bool, error) {
	if v.Method != "app/installed" {
		return AppsInstalledParams{}, false, nil
	}
	var params AppsInstalledParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsReadFileParams() (FsReadFileParams, bool, error) {
	if v.Method != "fs/readFile" {
		return FsReadFileParams{}, false, nil
	}
	var params FsReadFileParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsWriteFileParams() (FsWriteFileParams, bool, error) {
	if v.Method != "fs/writeFile" {
		return FsWriteFileParams{}, false, nil
	}
	var params FsWriteFileParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsCreateDirectoryParams() (FsCreateDirectoryParams, bool, error) {
	if v.Method != "fs/createDirectory" {
		return FsCreateDirectoryParams{}, false, nil
	}
	var params FsCreateDirectoryParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsGetMetadataParams() (FsGetMetadataParams, bool, error) {
	if v.Method != "fs/getMetadata" {
		return FsGetMetadataParams{}, false, nil
	}
	var params FsGetMetadataParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsReadDirectoryParams() (FsReadDirectoryParams, bool, error) {
	if v.Method != "fs/readDirectory" {
		return FsReadDirectoryParams{}, false, nil
	}
	var params FsReadDirectoryParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsRemoveParams() (FsRemoveParams, bool, error) {
	if v.Method != "fs/remove" {
		return FsRemoveParams{}, false, nil
	}
	var params FsRemoveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsCopyParams() (FsCopyParams, bool, error) {
	if v.Method != "fs/copy" {
		return FsCopyParams{}, false, nil
	}
	var params FsCopyParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsWatchParams() (FsWatchParams, bool, error) {
	if v.Method != "fs/watch" {
		return FsWatchParams{}, false, nil
	}
	var params FsWatchParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FsUnwatchParams() (FsUnwatchParams, bool, error) {
	if v.Method != "fs/unwatch" {
		return FsUnwatchParams{}, false, nil
	}
	var params FsUnwatchParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) SkillsConfigWriteParams() (SkillsConfigWriteParams, bool, error) {
	if v.Method != "skills/config/write" {
		return SkillsConfigWriteParams{}, false, nil
	}
	var params SkillsConfigWriteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginInstallParams() (PluginInstallParams, bool, error) {
	if v.Method != "plugin/install" {
		return PluginInstallParams{}, false, nil
	}
	var params PluginInstallParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PluginUninstallParams() (PluginUninstallParams, bool, error) {
	if v.Method != "plugin/uninstall" {
		return PluginUninstallParams{}, false, nil
	}
	var params PluginUninstallParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) TurnStartParams() (TurnStartParams, bool, error) {
	if v.Method != "turn/start" {
		return TurnStartParams{}, false, nil
	}
	var params TurnStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) TurnSettingsUpdateParams() (TurnSettingsUpdateParams, bool, error) {
	if v.Method != "turn/settings/update" {
		return TurnSettingsUpdateParams{}, false, nil
	}
	var params TurnSettingsUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) TurnSteerParams() (TurnSteerParams, bool, error) {
	if v.Method != "turn/steer" {
		return TurnSteerParams{}, false, nil
	}
	var params TurnSteerParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) TurnInterruptParams() (TurnInterruptParams, bool, error) {
	if v.Method != "turn/interrupt" {
		return TurnInterruptParams{}, false, nil
	}
	var params TurnInterruptParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRealtimeStartParams() (ThreadRealtimeStartParams, bool, error) {
	if v.Method != "thread/realtime/start" {
		return ThreadRealtimeStartParams{}, false, nil
	}
	var params ThreadRealtimeStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRealtimeAppendAudioParams() (ThreadRealtimeAppendAudioParams, bool, error) {
	if v.Method != "thread/realtime/appendAudio" {
		return ThreadRealtimeAppendAudioParams{}, false, nil
	}
	var params ThreadRealtimeAppendAudioParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRealtimeAppendTextParams() (ThreadRealtimeAppendTextParams, bool, error) {
	if v.Method != "thread/realtime/appendText" {
		return ThreadRealtimeAppendTextParams{}, false, nil
	}
	var params ThreadRealtimeAppendTextParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRealtimeAppendSpeechParams() (ThreadRealtimeAppendSpeechParams, bool, error) {
	if v.Method != "thread/realtime/appendSpeech" {
		return ThreadRealtimeAppendSpeechParams{}, false, nil
	}
	var params ThreadRealtimeAppendSpeechParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRealtimeStopParams() (ThreadRealtimeStopParams, bool, error) {
	if v.Method != "thread/realtime/stop" {
		return ThreadRealtimeStopParams{}, false, nil
	}
	var params ThreadRealtimeStopParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadTimelineListParams() (ThreadTimelineListParams, bool, error) {
	if v.Method != "thread/timeline/list" {
		return ThreadTimelineListParams{}, false, nil
	}
	var params ThreadTimelineListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ThreadRealtimeListVoicesParams() (ThreadRealtimeListVoicesParams, bool, error) {
	if v.Method != "thread/realtime/listVoices" {
		return ThreadRealtimeListVoicesParams{}, false, nil
	}
	var params ThreadRealtimeListVoicesParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ReviewStartParams() (ReviewStartParams, bool, error) {
	if v.Method != "review/start" {
		return ReviewStartParams{}, false, nil
	}
	var params ReviewStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ModelListParams() (ModelListParams, bool, error) {
	if v.Method != "model/list" {
		return ModelListParams{}, false, nil
	}
	var params ModelListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ModelProviderCapabilitiesReadParams() (ModelProviderCapabilitiesReadParams, bool, error) {
	if v.Method != "modelProvider/capabilities/read" {
		return ModelProviderCapabilitiesReadParams{}, false, nil
	}
	var params ModelProviderCapabilitiesReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ExperimentalFeatureListParams() (ExperimentalFeatureListParams, bool, error) {
	if v.Method != "experimentalFeature/list" {
		return ExperimentalFeatureListParams{}, false, nil
	}
	var params ExperimentalFeatureListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) PermissionProfileListParams() (PermissionProfileListParams, bool, error) {
	if v.Method != "permissionProfile/list" {
		return PermissionProfileListParams{}, false, nil
	}
	var params PermissionProfileListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ExperimentalFeatureEnablementSetParams() (ExperimentalFeatureEnablementSetParams, bool, error) {
	if v.Method != "experimentalFeature/enablement/set" {
		return ExperimentalFeatureEnablementSetParams{}, false, nil
	}
	var params ExperimentalFeatureEnablementSetParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) RemoteControlEnableParams() (NullableRemoteControlEnableParams, bool, error) {
	if v.Method != "remoteControl/enable" {
		return NullableRemoteControlEnableParams{}, false, nil
	}
	var params NullableRemoteControlEnableParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, nil
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) RemoteControlDisableParams() (NullableRemoteControlDisableParams, bool, error) {
	if v.Method != "remoteControl/disable" {
		return NullableRemoteControlDisableParams{}, false, nil
	}
	var params NullableRemoteControlDisableParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, nil
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) RemoteControlPairingStartParams() (RemoteControlPairingStartParams, bool, error) {
	if v.Method != "remoteControl/pairing/start" {
		return RemoteControlPairingStartParams{}, false, nil
	}
	var params RemoteControlPairingStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) RemoteControlPairingStatusParams() (RemoteControlPairingStatusParams, bool, error) {
	if v.Method != "remoteControl/pairing/status" {
		return RemoteControlPairingStatusParams{}, false, nil
	}
	var params RemoteControlPairingStatusParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) RemoteControlClientListParams() (RemoteControlClientsListParams, bool, error) {
	if v.Method != "remoteControl/client/list" {
		return RemoteControlClientsListParams{}, false, nil
	}
	var params RemoteControlClientsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) RemoteControlClientRevokeParams() (RemoteControlClientsRevokeParams, bool, error) {
	if v.Method != "remoteControl/client/revoke" {
		return RemoteControlClientsRevokeParams{}, false, nil
	}
	var params RemoteControlClientsRevokeParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) CollaborationModeListParams() (CollaborationModeListParams, bool, error) {
	if v.Method != "collaborationMode/list" {
		return CollaborationModeListParams{}, false, nil
	}
	var params CollaborationModeListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) EnvironmentAddParams() (EnvironmentAddParams, bool, error) {
	if v.Method != "environment/add" {
		return EnvironmentAddParams{}, false, nil
	}
	var params EnvironmentAddParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) EnvironmentInfoParams() (EnvironmentInfoParams, bool, error) {
	if v.Method != "environment/info" {
		return EnvironmentInfoParams{}, false, nil
	}
	var params EnvironmentInfoParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) EnvironmentStatusParams() (EnvironmentStatusParams, bool, error) {
	if v.Method != "environment/status" {
		return EnvironmentStatusParams{}, false, nil
	}
	var params EnvironmentStatusParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) McpServerOauthLoginParams() (McpServerOauthLoginParams, bool, error) {
	if v.Method != "mcpServer/oauth/login" {
		return McpServerOauthLoginParams{}, false, nil
	}
	var params McpServerOauthLoginParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) McpServerStatusListParams() (ListMcpServerStatusParams, bool, error) {
	if v.Method != "mcpServerStatus/list" {
		return ListMcpServerStatusParams{}, false, nil
	}
	var params ListMcpServerStatusParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) McpServerResourceReadParams() (McpResourceReadParams, bool, error) {
	if v.Method != "mcpServer/resource/read" {
		return McpResourceReadParams{}, false, nil
	}
	var params McpResourceReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) McpServerEventStreamStartParams() (McpServerEventStreamStartParams, bool, error) {
	if v.Method != "mcpServer/event/stream/start" {
		return McpServerEventStreamStartParams{}, false, nil
	}
	var params McpServerEventStreamStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) McpServerEventStreamStopParams() (McpServerEventStreamStopParams, bool, error) {
	if v.Method != "mcpServer/event/stream/stop" {
		return McpServerEventStreamStopParams{}, false, nil
	}
	var params McpServerEventStreamStopParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) McpServerToolCallParams() (McpServerToolCallParams, bool, error) {
	if v.Method != "mcpServer/tool/call" {
		return McpServerToolCallParams{}, false, nil
	}
	var params McpServerToolCallParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) WindowsSandboxSetupStartParams() (WindowsSandboxSetupStartParams, bool, error) {
	if v.Method != "windowsSandbox/setupStart" {
		return WindowsSandboxSetupStartParams{}, false, nil
	}
	var params WindowsSandboxSetupStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountLoginStartParams() (LoginAccountParams, bool, error) {
	if v.Method != "account/login/start" {
		return LoginAccountParams{}, false, nil
	}
	var params LoginAccountParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountBedrockDiscoverParams() (BedrockDiscoverParams, bool, error) {
	if v.Method != "account/bedrock/discover" {
		return BedrockDiscoverParams{}, false, nil
	}
	var params BedrockDiscoverParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountBedrockSetupParams() (BedrockSetupParams, bool, error) {
	if v.Method != "account/bedrock/setup" {
		return BedrockSetupParams{}, false, nil
	}
	var params BedrockSetupParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountGcpSetupParams() (GcpSetupParams, bool, error) {
	if v.Method != "account/gcp/setup" {
		return GcpSetupParams{}, false, nil
	}
	var params GcpSetupParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountBedrockCheckGovCloudRequirementsParams() (BedrockCheckGovCloudRequirementsParams, bool, error) {
	if v.Method != "account/bedrock/checkGovCloudRequirements" {
		return BedrockCheckGovCloudRequirementsParams{}, false, nil
	}
	var params BedrockCheckGovCloudRequirementsParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountLoginCancelParams() (CancelLoginAccountParams, bool, error) {
	if v.Method != "account/login/cancel" {
		return CancelLoginAccountParams{}, false, nil
	}
	var params CancelLoginAccountParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountSessionListParams() (AccountSessionsListParams, bool, error) {
	if v.Method != "accountSession/list" {
		return AccountSessionsListParams{}, false, nil
	}
	var params AccountSessionsListParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountSessionSwitchParams() (AccountSessionsSwitchParams, bool, error) {
	if v.Method != "accountSession/switch" {
		return AccountSessionsSwitchParams{}, false, nil
	}
	var params AccountSessionsSwitchParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountRateLimitsReadParams() (NullableGetAccountRateLimitsParams, bool, error) {
	if v.Method != "account/rateLimits/read" {
		return NullableGetAccountRateLimitsParams{}, false, nil
	}
	var params NullableGetAccountRateLimitsParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, nil
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountRateLimitResetCreditConsumeParams() (ConsumeAccountRateLimitResetCreditParams, bool, error) {
	if v.Method != "account/rateLimitResetCredit/consume" {
		return ConsumeAccountRateLimitResetCreditParams{}, false, nil
	}
	var params ConsumeAccountRateLimitResetCreditParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountUsageReadParams() (NullableGetAccountTokenUsageParams, bool, error) {
	if v.Method != "account/usage/read" {
		return NullableGetAccountTokenUsageParams{}, false, nil
	}
	var params NullableGetAccountTokenUsageParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, nil
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountSendAddCreditsNudgeEmailParams() (SendAddCreditsNudgeEmailParams, bool, error) {
	if v.Method != "account/sendAddCreditsNudgeEmail" {
		return SendAddCreditsNudgeEmailParams{}, false, nil
	}
	var params SendAddCreditsNudgeEmailParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FeedbackUploadParams() (FeedbackUploadParams, bool, error) {
	if v.Method != "feedback/upload" {
		return FeedbackUploadParams{}, false, nil
	}
	var params FeedbackUploadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FeedbackSaveParams() (FeedbackSaveParams, bool, error) {
	if v.Method != "feedback/save" {
		return FeedbackSaveParams{}, false, nil
	}
	var params FeedbackSaveParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) CommandExecParams() (CommandExecParams, bool, error) {
	if v.Method != "command/exec" {
		return CommandExecParams{}, false, nil
	}
	var params CommandExecParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) CommandExecWriteParams() (CommandExecWriteParams, bool, error) {
	if v.Method != "command/exec/write" {
		return CommandExecWriteParams{}, false, nil
	}
	var params CommandExecWriteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) CommandExecTerminateParams() (CommandExecTerminateParams, bool, error) {
	if v.Method != "command/exec/terminate" {
		return CommandExecTerminateParams{}, false, nil
	}
	var params CommandExecTerminateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) CommandExecResizeParams() (CommandExecResizeParams, bool, error) {
	if v.Method != "command/exec/resize" {
		return CommandExecResizeParams{}, false, nil
	}
	var params CommandExecResizeParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProcessSpawnParams() (ProcessSpawnParams, bool, error) {
	if v.Method != "process/spawn" {
		return ProcessSpawnParams{}, false, nil
	}
	var params ProcessSpawnParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProcessWriteStdinParams() (ProcessWriteStdinParams, bool, error) {
	if v.Method != "process/writeStdin" {
		return ProcessWriteStdinParams{}, false, nil
	}
	var params ProcessWriteStdinParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProcessKillParams() (ProcessKillParams, bool, error) {
	if v.Method != "process/kill" {
		return ProcessKillParams{}, false, nil
	}
	var params ProcessKillParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ProcessResizePtyParams() (ProcessResizePtyParams, bool, error) {
	if v.Method != "process/resizePty" {
		return ProcessResizePtyParams{}, false, nil
	}
	var params ProcessResizePtyParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ConfigReadParams() (ConfigReadParams, bool, error) {
	if v.Method != "config/read" {
		return ConfigReadParams{}, false, nil
	}
	var params ConfigReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ExternalAgentConfigDetectParams() (ExternalAgentConfigDetectParams, bool, error) {
	if v.Method != "externalAgentConfig/detect" {
		return ExternalAgentConfigDetectParams{}, false, nil
	}
	var params ExternalAgentConfigDetectParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ExternalAgentConfigImportParams() (ExternalAgentConfigImportParams, bool, error) {
	if v.Method != "externalAgentConfig/import" {
		return ExternalAgentConfigImportParams{}, false, nil
	}
	var params ExternalAgentConfigImportParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ExternalAgentConfigImportRecordHistoryParams() (ExternalAgentConfigImportHistoryRecordParams, bool, error) {
	if v.Method != "externalAgentConfig/import/recordHistory" {
		return ExternalAgentConfigImportHistoryRecordParams{}, false, nil
	}
	var params ExternalAgentConfigImportHistoryRecordParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ConfigValueWriteParams() (ConfigValueWriteParams, bool, error) {
	if v.Method != "config/value/write" {
		return ConfigValueWriteParams{}, false, nil
	}
	var params ConfigValueWriteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) ConfigBatchWriteParams() (ConfigBatchWriteParams, bool, error) {
	if v.Method != "config/batchWrite" {
		return ConfigBatchWriteParams{}, false, nil
	}
	var params ConfigBatchWriteParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) AccountReadParams() (GetAccountParams, bool, error) {
	if v.Method != "account/read" {
		return GetAccountParams{}, false, nil
	}
	var params GetAccountParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FuzzyFileSearchParams() (FuzzyFileSearchParams, bool, error) {
	if v.Method != "fuzzyFileSearch" {
		return FuzzyFileSearchParams{}, false, nil
	}
	var params FuzzyFileSearchParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FuzzyFileSearchSessionStartParams() (FuzzyFileSearchSessionStartParams, bool, error) {
	if v.Method != "fuzzyFileSearch/sessionStart" {
		return FuzzyFileSearchSessionStartParams{}, false, nil
	}
	var params FuzzyFileSearchSessionStartParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FuzzyFileSearchSessionUpdateParams() (FuzzyFileSearchSessionUpdateParams, bool, error) {
	if v.Method != "fuzzyFileSearch/sessionUpdate" {
		return FuzzyFileSearchSessionUpdateParams{}, false, nil
	}
	var params FuzzyFileSearchSessionUpdateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ClientRequest) FuzzyFileSearchSessionStopParams() (FuzzyFileSearchSessionStopParams, bool, error) {
	if v.Method != "fuzzyFileSearch/sessionStop" {
		return FuzzyFileSearchSessionStopParams{}, false, nil
	}
	var params FuzzyFileSearchSessionStopParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

type CommandExecutionApprovalDecision struct {
	StringValue                   string                                      `json:"-"`
	AcceptWithExecpolicyAmendment OptionalNonNull[map[string]json.RawMessage] `json:"acceptWithExecpolicyAmendment,omitzero"`
	ApplyNetworkPolicyAmendment   OptionalNonNull[map[string]json.RawMessage] `json:"applyNetworkPolicyAmendment,omitzero"`
	RawJSON                       json.RawMessage                             `json:"-"`
}

var (
	CommandExecutionApprovalDecisionAccept           = CommandExecutionApprovalDecision{StringValue: "accept"}
	CommandExecutionApprovalDecisionAcceptForSession = CommandExecutionApprovalDecision{StringValue: "acceptForSession"}
	CommandExecutionApprovalDecisionDecline          = CommandExecutionApprovalDecision{StringValue: "decline"}
	CommandExecutionApprovalDecisionCancel           = CommandExecutionApprovalDecision{StringValue: "cancel"}
)

func (v CommandExecutionApprovalDecision) IsSet() bool {
	return v.StringValue != "" || len(bytes.TrimSpace(v.RawJSON)) > 0 || v.AcceptWithExecpolicyAmendment.IsSet() || v.ApplyNetworkPolicyAmendment.IsSet()
}

var commandExecutionApprovalDecisionSpec = objectSpec{
	fields: []objectField{
		{name: "acceptWithExecpolicyAmendment", flags: fieldNonNullOptional},
		{name: "applyNetworkPolicyAmendment", flags: fieldNonNullOptional},
	},
	fieldOffset: 1,
}

func (v CommandExecutionApprovalDecision) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.AcceptWithExecpolicyAmendment.IsSet() {
		out["acceptWithExecpolicyAmendment"] = v.AcceptWithExecpolicyAmendment
	}
	if v.ApplyNetworkPolicyAmendment.IsSet() {
		out["applyNetworkPolicyAmendment"] = v.ApplyNetworkPolicyAmendment
	}
	if v.StringValue != "" {
		if len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 {
			return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
		}
		switch v.StringValue {
		case "accept", "acceptForSession", "decline", "cancel":
			return json.Marshal(v.StringValue)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported CommandExecutionApprovalDecision string value %q", v.StringValue)}
		}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	if _, ok := out["acceptWithExecpolicyAmendment"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	if _, ok := out["applyNetworkPolicyAmendment"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid CommandExecutionApprovalDecision raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *CommandExecutionApprovalDecision) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var stringValue string
	if err := json.Unmarshal(trimmed, &stringValue); err == nil {
		switch stringValue {
		case "accept", "acceptForSession", "decline", "cancel":
			*v = CommandExecutionApprovalDecision{StringValue: stringValue}
		default:
			*v = CommandExecutionApprovalDecision{RawJSON: append(json.RawMessage(nil), data...)}
		}
		return nil
	}
	v.StringValue = ""
	type wire CommandExecutionApprovalDecision
	var s objectScan
	if _, err := s.begin(data, &commandExecutionApprovalDecisionSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.nonNull("acceptWithExecpolicyAmendment") {
		untaggedOneOfMatches++
	}
	if s.nonNull("applyNetworkPolicyAmendment") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type CommandExecutionApprovalKind string

const (
	CommandExecutionApprovalKindCommand    CommandExecutionApprovalKind = "command"
	CommandExecutionApprovalKindWriteStdin CommandExecutionApprovalKind = "writeStdin"
)

type CommandExecutionRequestApprovalParams struct {
	AdditionalPermissions           Optional[AdditionalPermissionProfile]         `json:"additionalPermissions,omitzero"`
	ApprovalID                      Optional[string]                              `json:"approvalId,omitzero"`
	AvailableDecisions              Optional[[]CommandExecutionApprovalDecision]  `json:"availableDecisions,omitzero"`
	Command                         Optional[string]                              `json:"command,omitzero"`
	CommandActions                  Optional[[]CommandAction]                     `json:"commandActions,omitzero"`
	Cwd                             Optional[LegacyAppPathString]                 `json:"cwd,omitzero"`
	EnvironmentID                   Optional[string]                              `json:"environmentId,omitzero"`
	ItemID                          string                                        `json:"itemId"`
	Kind                            OptionalNonNull[CommandExecutionApprovalKind] `json:"kind,omitzero"`
	NetworkApprovalContext          Optional[NetworkApprovalContext]              `json:"networkApprovalContext,omitzero"`
	ProposedExecpolicyAmendment     Optional[[]string]                            `json:"proposedExecpolicyAmendment,omitzero"`
	ProposedNetworkPolicyAmendments Optional[[]NetworkPolicyAmendment]            `json:"proposedNetworkPolicyAmendments,omitzero"`
	Reason                          Optional[string]                              `json:"reason,omitzero"`
	StartedAtMs                     int64                                         `json:"startedAtMs"`
	ThreadID                        string                                        `json:"threadId"`
	TurnID                          string                                        `json:"turnId"`
}

var commandExecutionRequestApprovalParamsSpec = objectSpec{
	fields: []objectField{
		{name: "additionalPermissions", defaultJSON: "null"},
		{name: "approvalId"},
		{name: "availableDecisions", defaultJSON: "null"},
		{name: "command", defaultJSON: "null"},
		{name: "commandActions", defaultJSON: "null"},
		{name: "cwd", defaultJSON: "null"},
		{name: "environmentId"},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "kind", flags: fieldNonNullOptional},
		{name: "networkApprovalContext"},
		{name: "proposedExecpolicyAmendment"},
		{name: "proposedNetworkPolicyAmendments"},
		{name: "reason"},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecutionRequestApprovalParams) MarshalJSON() ([]byte, error) {
	type wire CommandExecutionRequestApprovalParams
	w := wire(v)
	w.AdditionalPermissions = w.AdditionalPermissions.withoutNull()
	w.AvailableDecisions = w.AvailableDecisions.withoutNull()
	w.Command = w.Command.withoutNull()
	w.CommandActions = w.CommandActions.withoutNull()
	w.Cwd = w.Cwd.withoutNull()
	return json.Marshal(w)
}

func (v *CommandExecutionRequestApprovalParams) UnmarshalJSON(data []byte) error {
	type wire CommandExecutionRequestApprovalParams
	return decodeObject(data, &commandExecutionRequestApprovalParamsSpec, (*wire)(v))
}

type CommandExecutionRequestApprovalResponse struct {
	Decision CommandExecutionApprovalDecision `json:"decision"`
}

var commandExecutionRequestApprovalResponseSpec = objectSpec{
	fields: []objectField{
		{name: "decision", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecutionRequestApprovalResponse) MarshalJSON() ([]byte, error) {
	type wire CommandExecutionRequestApprovalResponse
	return json.Marshal(wire(v))
}

func (v *CommandExecutionRequestApprovalResponse) UnmarshalJSON(data []byte) error {
	type wire CommandExecutionRequestApprovalResponse
	return decodeObject(data, &commandExecutionRequestApprovalResponseSpec, (*wire)(v))
}

type CurrentTimeReadParams struct {
	ThreadID string `json:"threadId"`
}

var currentTimeReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CurrentTimeReadParams) MarshalJSON() ([]byte, error) {
	type wire CurrentTimeReadParams
	return json.Marshal(wire(v))
}

func (v *CurrentTimeReadParams) UnmarshalJSON(data []byte) error {
	type wire CurrentTimeReadParams
	return decodeObject(data, &currentTimeReadParamsSpec, (*wire)(v))
}

type CurrentTimeReadResponse struct {
	CurrentTimeAt int64 `json:"currentTimeAt"`
}

var currentTimeReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "currentTimeAt", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CurrentTimeReadResponse) MarshalJSON() ([]byte, error) {
	type wire CurrentTimeReadResponse
	return json.Marshal(wire(v))
}

func (v *CurrentTimeReadResponse) UnmarshalJSON(data []byte) error {
	type wire CurrentTimeReadResponse
	return decodeObject(data, &currentTimeReadResponseSpec, (*wire)(v))
}

type DynamicToolCallParams struct {
	Arguments json.RawMessage  `json:"arguments"`
	CallID    string           `json:"callId"`
	Namespace Optional[string] `json:"namespace,omitzero"`
	ThreadID  string           `json:"threadId"`
	Tool      string           `json:"tool"`
	TurnID    string           `json:"turnId"`
}

var dynamicToolCallParamsSpec = objectSpec{
	fields: []objectField{
		{name: "arguments", flags: fieldRequired},
		{name: "callId", flags: fieldRequired | fieldRejectNull},
		{name: "namespace"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "tool", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v DynamicToolCallParams) MarshalJSON() ([]byte, error) {
	type wire DynamicToolCallParams
	return json.Marshal(wire(v))
}

func (v *DynamicToolCallParams) UnmarshalJSON(data []byte) error {
	type wire DynamicToolCallParams
	return decodeObject(data, &dynamicToolCallParamsSpec, (*wire)(v))
}

type DynamicToolCallResponse struct {
	ContentItems []DynamicToolCallOutputContentItem `json:"contentItems"`
	Success      bool                               `json:"success"`
}

var dynamicToolCallResponseSpec = objectSpec{
	fields: []objectField{
		{name: "contentItems", flags: fieldRequired | fieldRejectNull},
		{name: "success", flags: fieldRequired | fieldRejectNull},
	},
}

func (v DynamicToolCallResponse) MarshalJSON() ([]byte, error) {
	type wire DynamicToolCallResponse
	return json.Marshal(wire(v))
}

func (v *DynamicToolCallResponse) UnmarshalJSON(data []byte) error {
	type wire DynamicToolCallResponse
	return decodeObject(data, &dynamicToolCallResponseSpec, (*wire)(v))
}

type FileChange struct {
	Content     OptionalNonNull[string] `json:"content,omitzero"`
	MovePath    Optional[string]        `json:"move_path,omitzero"`
	TypeValue   string                  `json:"type"`
	UnifiedDiff OptionalNonNull[string] `json:"unified_diff,omitzero"`
	RawJSON     json.RawMessage         `json:"-"`
}

var fileChangeSpec = objectSpec{
	fields: []objectField{
		{name: "content", flags: fieldNonNullOptional},
		{name: "move_path"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "unified_diff", flags: fieldNonNullOptional},
	},
	tag:  2,
	tags: []string{"add", "delete", "update"},
}

func (v FileChange) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "add":
		if !v.Content.IsSet() {
			return nil, DecodeError{Field: "content", Reason: "missing required field for type add"}
		}
	case "delete":
		if !v.Content.IsSet() {
			return nil, DecodeError{Field: "content", Reason: "missing required field for type delete"}
		}
	case "update":
		if !v.UnifiedDiff.IsSet() {
			return nil, DecodeError{Field: "unified_diff", Reason: "missing required field for type update"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire FileChange
	return json.Marshal(wire(v))
}

func (v *FileChange) UnmarshalJSON(data []byte) error {
	type wire FileChange
	var s objectScan
	known, err := s.begin(data, &fileChangeSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "add":
		if !s.has("content") {
			return DecodeError{Field: "content", Reason: "missing required field for type add"}
		}
		if s.isNull("content") {
			return DecodeError{Field: "content", Reason: "cannot be null"}
		}
	case "delete":
		if !s.has("content") {
			return DecodeError{Field: "content", Reason: "missing required field for type delete"}
		}
		if s.isNull("content") {
			return DecodeError{Field: "content", Reason: "cannot be null"}
		}
	case "update":
		if !s.has("unified_diff") {
			return DecodeError{Field: "unified_diff", Reason: "missing required field for type update"}
		}
		if s.isNull("unified_diff") {
			return DecodeError{Field: "unified_diff", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type FileChangeApprovalDecision string

const (
	FileChangeApprovalDecisionAccept           FileChangeApprovalDecision = "accept"
	FileChangeApprovalDecisionAcceptForSession FileChangeApprovalDecision = "acceptForSession"
	FileChangeApprovalDecisionDecline          FileChangeApprovalDecision = "decline"
	FileChangeApprovalDecisionCancel           FileChangeApprovalDecision = "cancel"
)

type FileChangeRequestApprovalParams struct {
	GrantRoot   Optional[string] `json:"grantRoot,omitzero"`
	ItemID      string           `json:"itemId"`
	Reason      Optional[string] `json:"reason,omitzero"`
	StartedAtMs int64            `json:"startedAtMs"`
	ThreadID    string           `json:"threadId"`
	TurnID      string           `json:"turnId"`
}

var fileChangeRequestApprovalParamsSpec = objectSpec{
	fields: []objectField{
		{name: "grantRoot"},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "reason"},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FileChangeRequestApprovalParams) MarshalJSON() ([]byte, error) {
	type wire FileChangeRequestApprovalParams
	return json.Marshal(wire(v))
}

func (v *FileChangeRequestApprovalParams) UnmarshalJSON(data []byte) error {
	type wire FileChangeRequestApprovalParams
	return decodeObject(data, &fileChangeRequestApprovalParamsSpec, (*wire)(v))
}

type FileChangeRequestApprovalResponse struct {
	Decision FileChangeApprovalDecision `json:"decision"`
}

var fileChangeRequestApprovalResponseSpec = objectSpec{
	fields: []objectField{
		{name: "decision", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FileChangeRequestApprovalResponse) MarshalJSON() ([]byte, error) {
	type wire FileChangeRequestApprovalResponse
	return json.Marshal(wire(v))
}

func (v *FileChangeRequestApprovalResponse) UnmarshalJSON(data []byte) error {
	type wire FileChangeRequestApprovalResponse
	return decodeObject(data, &fileChangeRequestApprovalResponseSpec, (*wire)(v))
}

type FuzzyFileSearchMatchType string

const (
	FuzzyFileSearchMatchTypeFile      FuzzyFileSearchMatchType = "file"
	FuzzyFileSearchMatchTypeDirectory FuzzyFileSearchMatchType = "directory"
)

type FuzzyFileSearchParams struct {
	CancellationToken Optional[string] `json:"cancellationToken,omitzero"`
	Query             string           `json:"query"`
	Roots             []string         `json:"roots"`
}

var fuzzyFileSearchParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cancellationToken"},
		{name: "query", flags: fieldRequired | fieldRejectNull},
		{name: "roots", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchParams) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchParams
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchParams) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchParams
	return decodeObject(data, &fuzzyFileSearchParamsSpec, (*wire)(v))
}

type FuzzyFileSearchResponse struct {
	Files []FuzzyFileSearchResult `json:"files"`
}

var fuzzyFileSearchResponseSpec = objectSpec{
	fields: []objectField{
		{name: "files", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchResponse) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchResponse
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchResponse) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchResponse
	return decodeObject(data, &fuzzyFileSearchResponseSpec, (*wire)(v))
}

type FuzzyFileSearchResult struct {
	FileName  string                   `json:"file_name"`
	Indices   Optional[[]uint32]       `json:"indices,omitzero"`
	MatchType FuzzyFileSearchMatchType `json:"match_type"`
	Path      string                   `json:"path"`
	Root      string                   `json:"root"`
	Score     uint32                   `json:"score"`
}

var fuzzyFileSearchResultSpec = objectSpec{
	fields: []objectField{
		{name: "file_name", flags: fieldRequired | fieldRejectNull},
		{name: "indices"},
		{name: "match_type", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "root", flags: fieldRequired | fieldRejectNull},
		{name: "score", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchResult) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchResult
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchResult) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchResult
	var s objectScan
	if _, err := s.begin(data, &fuzzyFileSearchResultSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Score < 0 {
		return DecodeError{Field: "score", Reason: "below minimum 0"}
	}
	return nil
}

type FuzzyFileSearchSessionCompletedNotification struct {
	SessionID string `json:"sessionId"`
}

var fuzzyFileSearchSessionCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchSessionCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchSessionCompletedNotification
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchSessionCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchSessionCompletedNotification
	return decodeObject(data, &fuzzyFileSearchSessionCompletedNotificationSpec, (*wire)(v))
}

type FuzzyFileSearchSessionStartParams struct {
	Roots     []string `json:"roots"`
	SessionID string   `json:"sessionId"`
}

var fuzzyFileSearchSessionStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "roots", flags: fieldRequired | fieldRejectNull},
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchSessionStartParams) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchSessionStartParams
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchSessionStartParams) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchSessionStartParams
	return decodeObject(data, &fuzzyFileSearchSessionStartParamsSpec, (*wire)(v))
}

type FuzzyFileSearchSessionStartResponse map[string]json.RawMessage

func (v FuzzyFileSearchSessionStartResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FuzzyFileSearchSessionStartResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FuzzyFileSearchSessionStartResponse(raw)
	return nil
}

type FuzzyFileSearchSessionStopParams struct {
	SessionID string `json:"sessionId"`
}

var fuzzyFileSearchSessionStopParamsSpec = objectSpec{
	fields: []objectField{
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchSessionStopParams) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchSessionStopParams
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchSessionStopParams) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchSessionStopParams
	return decodeObject(data, &fuzzyFileSearchSessionStopParamsSpec, (*wire)(v))
}

type FuzzyFileSearchSessionStopResponse map[string]json.RawMessage

func (v FuzzyFileSearchSessionStopResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FuzzyFileSearchSessionStopResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FuzzyFileSearchSessionStopResponse(raw)
	return nil
}

type FuzzyFileSearchSessionUpdateParams struct {
	Query     string `json:"query"`
	SessionID string `json:"sessionId"`
}

var fuzzyFileSearchSessionUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "query", flags: fieldRequired | fieldRejectNull},
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchSessionUpdateParams) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchSessionUpdateParams
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchSessionUpdateParams) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchSessionUpdateParams
	return decodeObject(data, &fuzzyFileSearchSessionUpdateParamsSpec, (*wire)(v))
}

type FuzzyFileSearchSessionUpdateResponse map[string]json.RawMessage

func (v FuzzyFileSearchSessionUpdateResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FuzzyFileSearchSessionUpdateResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FuzzyFileSearchSessionUpdateResponse(raw)
	return nil
}

type FuzzyFileSearchSessionUpdatedNotification struct {
	Files     []FuzzyFileSearchResult `json:"files"`
	Query     string                  `json:"query"`
	SessionID string                  `json:"sessionId"`
}

var fuzzyFileSearchSessionUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "files", flags: fieldRequired | fieldRejectNull},
		{name: "query", flags: fieldRequired | fieldRejectNull},
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FuzzyFileSearchSessionUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire FuzzyFileSearchSessionUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *FuzzyFileSearchSessionUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire FuzzyFileSearchSessionUpdatedNotification
	return decodeObject(data, &fuzzyFileSearchSessionUpdatedNotificationSpec, (*wire)(v))
}

type GrantedPermissionProfile struct {
	FileSystem Optional[AdditionalFileSystemPermissions] `json:"fileSystem,omitzero"`
	Network    Optional[AdditionalNetworkPermissions]    `json:"network,omitzero"`
}

var grantedPermissionProfileSpec = objectSpec{
	fields: []objectField{
		{name: "fileSystem"},
		{name: "network"},
	},
}

func (v GrantedPermissionProfile) MarshalJSON() ([]byte, error) {
	type wire GrantedPermissionProfile
	return json.Marshal(wire(v))
}

func (v *GrantedPermissionProfile) UnmarshalJSON(data []byte) error {
	type wire GrantedPermissionProfile
	return decodeObject(data, &grantedPermissionProfileSpec, (*wire)(v))
}

type InitializeCapabilities struct {
	ExperimentalAPI                OptionalNonNull[bool]                `json:"experimentalApi,omitzero"`
	ExplicitGatewayOauth           OptionalNonNull[bool]                `json:"explicitGatewayOauth,omitzero"`
	Extensions                     Optional[map[string]json.RawMessage] `json:"extensions,omitzero"`
	McpServerOpenaiFormElicitation OptionalNonNull[bool]                `json:"mcpServerOpenaiFormElicitation,omitzero"`
	OptOutNotificationMethods      Optional[[]string]                   `json:"optOutNotificationMethods,omitzero"`
	RequestAttestation             OptionalNonNull[bool]                `json:"requestAttestation,omitzero"`
}

var initializeCapabilitiesSpec = objectSpec{
	fields: []objectField{
		{name: "experimentalApi", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "explicitGatewayOauth", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "extensions"},
		{name: "mcpServerOpenaiFormElicitation", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "optOutNotificationMethods"},
		{name: "requestAttestation", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v InitializeCapabilities) MarshalJSON() ([]byte, error) {
	type wire InitializeCapabilities
	w := wire(v)
	w.ExplicitGatewayOauth = nonNullWithoutFalse(w.ExplicitGatewayOauth)
	w.McpServerOpenaiFormElicitation = nonNullWithoutFalse(w.McpServerOpenaiFormElicitation)
	return json.Marshal(w)
}

func (v *InitializeCapabilities) UnmarshalJSON(data []byte) error {
	type wire InitializeCapabilities
	return decodeObject(data, &initializeCapabilitiesSpec, (*wire)(v))
}

type InitializeParams struct {
	Capabilities Optional[InitializeCapabilities] `json:"capabilities,omitzero"`
	ClientInfo   ClientInfo                       `json:"clientInfo"`
}

var initializeParamsSpec = objectSpec{
	fields: []objectField{
		{name: "capabilities"},
		{name: "clientInfo", flags: fieldRequired | fieldRejectNull},
	},
}

func (v InitializeParams) MarshalJSON() ([]byte, error) {
	type wire InitializeParams
	w := wire(v)
	w.Capabilities = w.Capabilities.withoutNull()
	return json.Marshal(w)
}

func (v *InitializeParams) UnmarshalJSON(data []byte) error {
	type wire InitializeParams
	return decodeObject(data, &initializeParamsSpec, (*wire)(v))
}

type InitializeResponse struct {
	ActiveProtocolMode         ActiveProtocolMode `json:"activeProtocolMode"`
	CodexHome                  AbsolutePathBuf    `json:"codexHome"`
	ExperimentalManifestDigest string             `json:"experimentalManifestDigest"`
	ExperimentalProtocolDigest string             `json:"experimentalProtocolDigest"`
	ExperimentalSchemaDigest   string             `json:"experimentalSchemaDigest"`
	PlatformFamily             string             `json:"platformFamily"`
	PlatformOs                 string             `json:"platformOs"`
	StableManifestDigest       string             `json:"stableManifestDigest"`
	StableProtocolDigest       string             `json:"stableProtocolDigest"`
	StableSchemaDigest         string             `json:"stableSchemaDigest"`
	UserAgent                  string             `json:"userAgent"`
}

var initializeResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activeProtocolMode", flags: fieldRequired | fieldRejectNull},
		{name: "codexHome", flags: fieldRequired | fieldRejectNull},
		{name: "experimentalManifestDigest", flags: fieldRequired | fieldRejectNull},
		{name: "experimentalProtocolDigest", flags: fieldRequired | fieldRejectNull},
		{name: "experimentalSchemaDigest", flags: fieldRequired | fieldRejectNull},
		{name: "platformFamily", flags: fieldRequired | fieldRejectNull},
		{name: "platformOs", flags: fieldRequired | fieldRejectNull},
		{name: "stableManifestDigest", flags: fieldRequired | fieldRejectNull},
		{name: "stableProtocolDigest", flags: fieldRequired | fieldRejectNull},
		{name: "stableSchemaDigest", flags: fieldRequired | fieldRejectNull},
		{name: "userAgent", flags: fieldRequired | fieldRejectNull},
	},
}

func (v InitializeResponse) MarshalJSON() ([]byte, error) {
	type wire InitializeResponse
	return json.Marshal(wire(v))
}

func (v *InitializeResponse) UnmarshalJSON(data []byte) error {
	type wire InitializeResponse
	return decodeObject(data, &initializeResponseSpec, (*wire)(v))
}

type JSONRPCError struct {
	Error JSONRPCErrorError `json:"error"`
	ID    RequestID         `json:"id"`
}

var jsonrpcErrorSpec = objectSpec{
	fields: []objectField{
		{name: "error", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
	},
}

func (v JSONRPCError) MarshalJSON() ([]byte, error) {
	type wire JSONRPCError
	return json.Marshal(wire(v))
}

func (v *JSONRPCError) UnmarshalJSON(data []byte) error {
	type wire JSONRPCError
	return decodeObject(data, &jsonrpcErrorSpec, (*wire)(v))
}

type JSONRPCErrorError struct {
	Code    int64           `json:"code"`
	Data    json.RawMessage `json:"data,omitempty"`
	Message string          `json:"message"`
}

var jsonrpcErrorErrorSpec = objectSpec{
	fields: []objectField{
		{name: "code", flags: fieldRequired | fieldRejectNull},
		{name: "data"},
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v JSONRPCErrorError) MarshalJSON() ([]byte, error) {
	type wire JSONRPCErrorError
	return json.Marshal(wire(v))
}

func (v *JSONRPCErrorError) UnmarshalJSON(data []byte) error {
	type wire JSONRPCErrorError
	return decodeObject(data, &jsonrpcErrorErrorSpec, (*wire)(v))
}

type JSONRPCMessage struct {
	Request      *JSONRPCRequest
	Notification *JSONRPCNotification
	Response     *JSONRPCResponse
	Error        *JSONRPCError
	RawJSON      json.RawMessage
}

func NewJSONRPCRequestMessage(value JSONRPCRequest) JSONRPCMessage {
	return JSONRPCMessage{Request: &value}
}

func (v JSONRPCMessage) JSONRPCRequest() (JSONRPCRequest, bool) {
	if v.Request == nil {
		return JSONRPCRequest{}, false
	}
	return *v.Request, true
}

func NewJSONRPCNotificationMessage(value JSONRPCNotification) JSONRPCMessage {
	return JSONRPCMessage{Notification: &value}
}

func (v JSONRPCMessage) JSONRPCNotification() (JSONRPCNotification, bool) {
	if v.Notification == nil {
		return JSONRPCNotification{}, false
	}
	return *v.Notification, true
}

func NewJSONRPCResponseMessage(value JSONRPCResponse) JSONRPCMessage {
	return JSONRPCMessage{Response: &value}
}

func (v JSONRPCMessage) JSONRPCResponse() (JSONRPCResponse, bool) {
	if v.Response == nil {
		return JSONRPCResponse{}, false
	}
	return *v.Response, true
}

func NewJSONRPCErrorMessage(value JSONRPCError) JSONRPCMessage {
	return JSONRPCMessage{Error: &value}
}

func (v JSONRPCMessage) JSONRPCError() (JSONRPCError, bool) {
	if v.Error == nil {
		return JSONRPCError{}, false
	}
	return *v.Error, true
}

func (v JSONRPCMessage) MarshalJSON() ([]byte, error) {
	switch {
	case v.Request != nil:
		return json.Marshal(v.Request)
	case v.Notification != nil:
		return json.Marshal(v.Notification)
	case v.Response != nil:
		return json.Marshal(v.Response)
	case v.Error != nil:
		return json.Marshal(v.Error)
	case len(bytes.TrimSpace(v.RawJSON)) > 0:
		if !json.Valid(v.RawJSON) {
			return nil, fmt.Errorf("invalid JSON-RPC message raw fallback")
		}
		return append([]byte(nil), v.RawJSON...), nil
	default:
		return nil, DecodeError{Field: "", Reason: "empty JSON-RPC message"}
	}
}

func (v *JSONRPCMessage) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if _, ok := raw["error"]; ok {
		var value JSONRPCError
		if err := json.Unmarshal(data, &value); err != nil {
			return err
		}
		*v = JSONRPCMessage{Error: &value}
		return nil
	}
	if _, ok := raw["result"]; ok {
		var value JSONRPCResponse
		if err := json.Unmarshal(data, &value); err != nil {
			return err
		}
		*v = JSONRPCMessage{Response: &value}
		return nil
	}
	if _, hasID := raw["id"]; hasID {
		if _, hasMethod := raw["method"]; hasMethod {
			var value JSONRPCRequest
			if err := json.Unmarshal(data, &value); err != nil {
				return err
			}
			*v = JSONRPCMessage{Request: &value}
			return nil
		}
	}
	if _, ok := raw["method"]; ok {
		var value JSONRPCNotification
		if err := json.Unmarshal(data, &value); err != nil {
			return err
		}
		*v = JSONRPCMessage{Notification: &value}
		return nil
	}
	*v = JSONRPCMessage{RawJSON: append([]byte(nil), trimmed...)}
	return nil
}

type JSONRPCNotification struct {
	Method string          `json:"method"`
	Params json.RawMessage `json:"params,omitempty"`
}

var jsonrpcNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "method", flags: fieldRequired | fieldRejectNull},
		{name: "params"},
	},
}

func (v JSONRPCNotification) MarshalJSON() ([]byte, error) {
	type wire JSONRPCNotification
	return json.Marshal(wire(v))
}

func (v *JSONRPCNotification) UnmarshalJSON(data []byte) error {
	type wire JSONRPCNotification
	return decodeObject(data, &jsonrpcNotificationSpec, (*wire)(v))
}

type JSONRPCRequest struct {
	ID     RequestID                 `json:"id"`
	Method string                    `json:"method"`
	Params json.RawMessage           `json:"params,omitempty"`
	Trace  Optional[W3cTraceContext] `json:"trace,omitzero"`
}

var jsonrpcRequestSpec = objectSpec{
	fields: []objectField{
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "method", flags: fieldRequired | fieldRejectNull},
		{name: "params"},
		{name: "trace"},
	},
}

func (v JSONRPCRequest) MarshalJSON() ([]byte, error) {
	type wire JSONRPCRequest
	return json.Marshal(wire(v))
}

func (v *JSONRPCRequest) UnmarshalJSON(data []byte) error {
	type wire JSONRPCRequest
	return decodeObject(data, &jsonrpcRequestSpec, (*wire)(v))
}

type JSONRPCResponse struct {
	ID     RequestID       `json:"id"`
	Result json.RawMessage `json:"result"`
}

var jsonrpcResponseSpec = objectSpec{
	fields: []objectField{
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "result", flags: fieldRequired},
	},
}

func (v JSONRPCResponse) MarshalJSON() ([]byte, error) {
	type wire JSONRPCResponse
	return json.Marshal(wire(v))
}

func (v *JSONRPCResponse) UnmarshalJSON(data []byte) error {
	type wire JSONRPCResponse
	return decodeObject(data, &jsonrpcResponseSpec, (*wire)(v))
}

type McpElicitationArrayType string

const (
	McpElicitationArrayTypeArray McpElicitationArrayType = "array"
)

type McpElicitationBooleanSchema struct {
	DefaultValue Optional[bool]            `json:"default,omitzero"`
	Description  Optional[string]          `json:"description,omitzero"`
	Title        Optional[string]          `json:"title,omitzero"`
	TypeValue    McpElicitationBooleanType `json:"type"`
}

var mcpElicitationBooleanSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationBooleanSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationBooleanSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationBooleanSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationBooleanSchema
	return decodeObject(data, &mcpElicitationBooleanSchemaSpec, (*wire)(v))
}

type McpElicitationBooleanType string

const (
	McpElicitationBooleanTypeBoolean McpElicitationBooleanType = "boolean"
)

type McpElicitationConstOption struct {
	ConstValue string `json:"const"`
	Title      string `json:"title"`
}

var mcpElicitationConstOptionSpec = objectSpec{
	fields: []objectField{
		{name: "const", flags: fieldRequired | fieldRejectNull},
		{name: "title", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationConstOption) MarshalJSON() ([]byte, error) {
	type wire McpElicitationConstOption
	return json.Marshal(wire(v))
}

func (v *McpElicitationConstOption) UnmarshalJSON(data []byte) error {
	type wire McpElicitationConstOption
	return decodeObject(data, &mcpElicitationConstOptionSpec, (*wire)(v))
}

type McpElicitationEnumSchema struct {
	mcpElicitationSingleSelectEnumSchema *McpElicitationSingleSelectEnumSchema
	mcpElicitationMultiSelectEnumSchema  *McpElicitationMultiSelectEnumSchema
	mcpElicitationLegacyTitledEnumSchema *McpElicitationLegacyTitledEnumSchema
	RawJSON                              json.RawMessage
}

func NewMcpElicitationEnumSchemaMcpElicitationSingleSelectEnumSchema(value McpElicitationSingleSelectEnumSchema) McpElicitationEnumSchema {
	return McpElicitationEnumSchema{mcpElicitationSingleSelectEnumSchema: &value}
}

func (v McpElicitationEnumSchema) McpElicitationSingleSelectEnumSchema() (McpElicitationSingleSelectEnumSchema, bool) {
	if v.mcpElicitationSingleSelectEnumSchema == nil {
		return McpElicitationSingleSelectEnumSchema{}, false
	}
	return *v.mcpElicitationSingleSelectEnumSchema, true
}

func NewMcpElicitationEnumSchemaMcpElicitationMultiSelectEnumSchema(value McpElicitationMultiSelectEnumSchema) McpElicitationEnumSchema {
	return McpElicitationEnumSchema{mcpElicitationMultiSelectEnumSchema: &value}
}

func (v McpElicitationEnumSchema) McpElicitationMultiSelectEnumSchema() (McpElicitationMultiSelectEnumSchema, bool) {
	if v.mcpElicitationMultiSelectEnumSchema == nil {
		return McpElicitationMultiSelectEnumSchema{}, false
	}
	return *v.mcpElicitationMultiSelectEnumSchema, true
}

func NewMcpElicitationEnumSchemaMcpElicitationLegacyTitledEnumSchema(value McpElicitationLegacyTitledEnumSchema) McpElicitationEnumSchema {
	return McpElicitationEnumSchema{mcpElicitationLegacyTitledEnumSchema: &value}
}

func (v McpElicitationEnumSchema) McpElicitationLegacyTitledEnumSchema() (McpElicitationLegacyTitledEnumSchema, bool) {
	if v.mcpElicitationLegacyTitledEnumSchema == nil {
		return McpElicitationLegacyTitledEnumSchema{}, false
	}
	return *v.mcpElicitationLegacyTitledEnumSchema, true
}

func (v McpElicitationEnumSchema) MarshalJSON() ([]byte, error) {
	switch {
	case v.mcpElicitationSingleSelectEnumSchema != nil:
		return json.Marshal(v.mcpElicitationSingleSelectEnumSchema)
	case v.mcpElicitationMultiSelectEnumSchema != nil:
		return json.Marshal(v.mcpElicitationMultiSelectEnumSchema)
	case v.mcpElicitationLegacyTitledEnumSchema != nil:
		return json.Marshal(v.mcpElicitationLegacyTitledEnumSchema)
	case len(bytes.TrimSpace(v.RawJSON)) > 0:
		if !json.Valid(v.RawJSON) {
			return nil, fmt.Errorf("invalid McpElicitationEnumSchema raw fallback")
		}
		return append([]byte(nil), v.RawJSON...), nil
	default:
		return nil, DecodeError{Field: "", Reason: "empty McpElicitationEnumSchema"}
	}
}

func (v *McpElicitationEnumSchema) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if rawDiscriminator, ok := raw["type"]; ok {
		var discriminator string
		if err := json.Unmarshal(rawDiscriminator, &discriminator); err != nil {
			return fmt.Errorf("field type: %w", err)
		}
		matchedDiscriminator := false
		if discriminator == "string" {
			matchedDiscriminator = true
			var value McpElicitationSingleSelectEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationEnumSchema{mcpElicitationSingleSelectEnumSchema: &value}
				return nil
			}
		}
		if discriminator == "array" {
			matchedDiscriminator = true
			var value McpElicitationMultiSelectEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationEnumSchema{mcpElicitationMultiSelectEnumSchema: &value}
				return nil
			}
		}
		if discriminator == "string" {
			matchedDiscriminator = true
			var value McpElicitationLegacyTitledEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationEnumSchema{mcpElicitationLegacyTitledEnumSchema: &value}
				return nil
			}
		}
		if matchedDiscriminator {
			return DecodeError{Field: "type", Reason: "no matching union variant"}
		}
	}
	*v = McpElicitationEnumSchema{RawJSON: append([]byte(nil), trimmed...)}
	return nil
}

type McpElicitationLegacyTitledEnumSchema struct {
	DefaultValue Optional[string]         `json:"default,omitzero"`
	Description  Optional[string]         `json:"description,omitzero"`
	Enum         []string                 `json:"enum"`
	EnumNames    Optional[[]string]       `json:"enumNames,omitzero"`
	Title        Optional[string]         `json:"title,omitzero"`
	TypeValue    McpElicitationStringType `json:"type"`
}

var mcpElicitationLegacyTitledEnumSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "enum", flags: fieldRequired | fieldRejectNull},
		{name: "enumNames"},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationLegacyTitledEnumSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationLegacyTitledEnumSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationLegacyTitledEnumSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationLegacyTitledEnumSchema
	return decodeObject(data, &mcpElicitationLegacyTitledEnumSchemaSpec, (*wire)(v))
}

type McpElicitationMultiSelectEnumSchema struct {
	mcpElicitationUntitledMultiSelectEnumSchema *McpElicitationUntitledMultiSelectEnumSchema
	mcpElicitationTitledMultiSelectEnumSchema   *McpElicitationTitledMultiSelectEnumSchema
	RawJSON                                     json.RawMessage
}

func NewMcpElicitationMultiSelectEnumSchemaMcpElicitationUntitledMultiSelectEnumSchema(value McpElicitationUntitledMultiSelectEnumSchema) McpElicitationMultiSelectEnumSchema {
	return McpElicitationMultiSelectEnumSchema{mcpElicitationUntitledMultiSelectEnumSchema: &value}
}

func (v McpElicitationMultiSelectEnumSchema) McpElicitationUntitledMultiSelectEnumSchema() (McpElicitationUntitledMultiSelectEnumSchema, bool) {
	if v.mcpElicitationUntitledMultiSelectEnumSchema == nil {
		return McpElicitationUntitledMultiSelectEnumSchema{}, false
	}
	return *v.mcpElicitationUntitledMultiSelectEnumSchema, true
}

func NewMcpElicitationMultiSelectEnumSchemaMcpElicitationTitledMultiSelectEnumSchema(value McpElicitationTitledMultiSelectEnumSchema) McpElicitationMultiSelectEnumSchema {
	return McpElicitationMultiSelectEnumSchema{mcpElicitationTitledMultiSelectEnumSchema: &value}
}

func (v McpElicitationMultiSelectEnumSchema) McpElicitationTitledMultiSelectEnumSchema() (McpElicitationTitledMultiSelectEnumSchema, bool) {
	if v.mcpElicitationTitledMultiSelectEnumSchema == nil {
		return McpElicitationTitledMultiSelectEnumSchema{}, false
	}
	return *v.mcpElicitationTitledMultiSelectEnumSchema, true
}

func (v McpElicitationMultiSelectEnumSchema) MarshalJSON() ([]byte, error) {
	switch {
	case v.mcpElicitationUntitledMultiSelectEnumSchema != nil:
		return json.Marshal(v.mcpElicitationUntitledMultiSelectEnumSchema)
	case v.mcpElicitationTitledMultiSelectEnumSchema != nil:
		return json.Marshal(v.mcpElicitationTitledMultiSelectEnumSchema)
	case len(bytes.TrimSpace(v.RawJSON)) > 0:
		if !json.Valid(v.RawJSON) {
			return nil, fmt.Errorf("invalid McpElicitationMultiSelectEnumSchema raw fallback")
		}
		return append([]byte(nil), v.RawJSON...), nil
	default:
		return nil, DecodeError{Field: "", Reason: "empty McpElicitationMultiSelectEnumSchema"}
	}
}

func (v *McpElicitationMultiSelectEnumSchema) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if rawDiscriminator, ok := raw["type"]; ok {
		var discriminator string
		if err := json.Unmarshal(rawDiscriminator, &discriminator); err != nil {
			return fmt.Errorf("field type: %w", err)
		}
		matchedDiscriminator := false
		if discriminator == "array" {
			matchedDiscriminator = true
			var value McpElicitationUntitledMultiSelectEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationMultiSelectEnumSchema{mcpElicitationUntitledMultiSelectEnumSchema: &value}
				return nil
			}
		}
		if discriminator == "array" {
			matchedDiscriminator = true
			var value McpElicitationTitledMultiSelectEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationMultiSelectEnumSchema{mcpElicitationTitledMultiSelectEnumSchema: &value}
				return nil
			}
		}
		if matchedDiscriminator {
			return DecodeError{Field: "type", Reason: "no matching union variant"}
		}
	}
	*v = McpElicitationMultiSelectEnumSchema{RawJSON: append([]byte(nil), trimmed...)}
	return nil
}

type McpElicitationNumberSchema struct {
	DefaultValue Optional[float64]        `json:"default,omitzero"`
	Description  Optional[string]         `json:"description,omitzero"`
	Maximum      Optional[float64]        `json:"maximum,omitzero"`
	Minimum      Optional[float64]        `json:"minimum,omitzero"`
	Title        Optional[string]         `json:"title,omitzero"`
	TypeValue    McpElicitationNumberType `json:"type"`
}

var mcpElicitationNumberSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "maximum"},
		{name: "minimum"},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationNumberSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationNumberSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationNumberSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationNumberSchema
	return decodeObject(data, &mcpElicitationNumberSchemaSpec, (*wire)(v))
}

type McpElicitationNumberType string

const (
	McpElicitationNumberTypeNumber  McpElicitationNumberType = "number"
	McpElicitationNumberTypeInteger McpElicitationNumberType = "integer"
)

type McpElicitationObjectType string

const (
	McpElicitationObjectTypeObject McpElicitationObjectType = "object"
)

type McpElicitationPrimitiveSchema struct {
	mcpElicitationEnumSchema    *McpElicitationEnumSchema
	mcpElicitationStringSchema  *McpElicitationStringSchema
	mcpElicitationNumberSchema  *McpElicitationNumberSchema
	mcpElicitationBooleanSchema *McpElicitationBooleanSchema
	RawJSON                     json.RawMessage
}

func NewMcpElicitationPrimitiveSchemaMcpElicitationEnumSchema(value McpElicitationEnumSchema) McpElicitationPrimitiveSchema {
	return McpElicitationPrimitiveSchema{mcpElicitationEnumSchema: &value}
}

func (v McpElicitationPrimitiveSchema) McpElicitationEnumSchema() (McpElicitationEnumSchema, bool) {
	if v.mcpElicitationEnumSchema == nil {
		return McpElicitationEnumSchema{}, false
	}
	return *v.mcpElicitationEnumSchema, true
}

func NewMcpElicitationPrimitiveSchemaMcpElicitationStringSchema(value McpElicitationStringSchema) McpElicitationPrimitiveSchema {
	return McpElicitationPrimitiveSchema{mcpElicitationStringSchema: &value}
}

func (v McpElicitationPrimitiveSchema) McpElicitationStringSchema() (McpElicitationStringSchema, bool) {
	if v.mcpElicitationStringSchema == nil {
		return McpElicitationStringSchema{}, false
	}
	return *v.mcpElicitationStringSchema, true
}

func NewMcpElicitationPrimitiveSchemaMcpElicitationNumberSchema(value McpElicitationNumberSchema) McpElicitationPrimitiveSchema {
	return McpElicitationPrimitiveSchema{mcpElicitationNumberSchema: &value}
}

func (v McpElicitationPrimitiveSchema) McpElicitationNumberSchema() (McpElicitationNumberSchema, bool) {
	if v.mcpElicitationNumberSchema == nil {
		return McpElicitationNumberSchema{}, false
	}
	return *v.mcpElicitationNumberSchema, true
}

func NewMcpElicitationPrimitiveSchemaMcpElicitationBooleanSchema(value McpElicitationBooleanSchema) McpElicitationPrimitiveSchema {
	return McpElicitationPrimitiveSchema{mcpElicitationBooleanSchema: &value}
}

func (v McpElicitationPrimitiveSchema) McpElicitationBooleanSchema() (McpElicitationBooleanSchema, bool) {
	if v.mcpElicitationBooleanSchema == nil {
		return McpElicitationBooleanSchema{}, false
	}
	return *v.mcpElicitationBooleanSchema, true
}

func (v McpElicitationPrimitiveSchema) MarshalJSON() ([]byte, error) {
	switch {
	case v.mcpElicitationEnumSchema != nil:
		return json.Marshal(v.mcpElicitationEnumSchema)
	case v.mcpElicitationStringSchema != nil:
		return json.Marshal(v.mcpElicitationStringSchema)
	case v.mcpElicitationNumberSchema != nil:
		return json.Marshal(v.mcpElicitationNumberSchema)
	case v.mcpElicitationBooleanSchema != nil:
		return json.Marshal(v.mcpElicitationBooleanSchema)
	case len(bytes.TrimSpace(v.RawJSON)) > 0:
		if !json.Valid(v.RawJSON) {
			return nil, fmt.Errorf("invalid McpElicitationPrimitiveSchema raw fallback")
		}
		return append([]byte(nil), v.RawJSON...), nil
	default:
		return nil, DecodeError{Field: "", Reason: "empty McpElicitationPrimitiveSchema"}
	}
}

func (v *McpElicitationPrimitiveSchema) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if rawDiscriminator, ok := raw["type"]; ok {
		var discriminator string
		if err := json.Unmarshal(rawDiscriminator, &discriminator); err != nil {
			return fmt.Errorf("field type: %w", err)
		}
		matchedDiscriminator := false
		if discriminator == "array" || discriminator == "string" {
			matchedDiscriminator = true
			var value McpElicitationEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationPrimitiveSchema{mcpElicitationEnumSchema: &value}
				return nil
			}
		}
		if discriminator == "string" {
			matchedDiscriminator = true
			var value McpElicitationStringSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationPrimitiveSchema{mcpElicitationStringSchema: &value}
				return nil
			}
		}
		if discriminator == "integer" || discriminator == "number" {
			matchedDiscriminator = true
			var value McpElicitationNumberSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationPrimitiveSchema{mcpElicitationNumberSchema: &value}
				return nil
			}
		}
		if discriminator == "boolean" {
			matchedDiscriminator = true
			var value McpElicitationBooleanSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationPrimitiveSchema{mcpElicitationBooleanSchema: &value}
				return nil
			}
		}
		if matchedDiscriminator {
			return DecodeError{Field: "type", Reason: "no matching union variant"}
		}
	}
	*v = McpElicitationPrimitiveSchema{RawJSON: append([]byte(nil), trimmed...)}
	return nil
}

type McpElicitationSchema struct {
	Schema     Optional[string]                         `json:"$schema,omitzero"`
	Properties map[string]McpElicitationPrimitiveSchema `json:"properties"`
	Required   Optional[[]string]                       `json:"required,omitzero"`
	TypeValue  McpElicitationObjectType                 `json:"type"`
}

var mcpElicitationSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "$schema"},
		{name: "properties", flags: fieldRequired | fieldRejectNull},
		{name: "required"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationSchema
	return decodeObject(data, &mcpElicitationSchemaSpec, (*wire)(v))
}

type McpElicitationSingleSelectEnumSchema struct {
	mcpElicitationUntitledSingleSelectEnumSchema *McpElicitationUntitledSingleSelectEnumSchema
	mcpElicitationTitledSingleSelectEnumSchema   *McpElicitationTitledSingleSelectEnumSchema
	RawJSON                                      json.RawMessage
}

func NewMcpElicitationSingleSelectEnumSchemaMcpElicitationUntitledSingleSelectEnumSchema(value McpElicitationUntitledSingleSelectEnumSchema) McpElicitationSingleSelectEnumSchema {
	return McpElicitationSingleSelectEnumSchema{mcpElicitationUntitledSingleSelectEnumSchema: &value}
}

func (v McpElicitationSingleSelectEnumSchema) McpElicitationUntitledSingleSelectEnumSchema() (McpElicitationUntitledSingleSelectEnumSchema, bool) {
	if v.mcpElicitationUntitledSingleSelectEnumSchema == nil {
		return McpElicitationUntitledSingleSelectEnumSchema{}, false
	}
	return *v.mcpElicitationUntitledSingleSelectEnumSchema, true
}

func NewMcpElicitationSingleSelectEnumSchemaMcpElicitationTitledSingleSelectEnumSchema(value McpElicitationTitledSingleSelectEnumSchema) McpElicitationSingleSelectEnumSchema {
	return McpElicitationSingleSelectEnumSchema{mcpElicitationTitledSingleSelectEnumSchema: &value}
}

func (v McpElicitationSingleSelectEnumSchema) McpElicitationTitledSingleSelectEnumSchema() (McpElicitationTitledSingleSelectEnumSchema, bool) {
	if v.mcpElicitationTitledSingleSelectEnumSchema == nil {
		return McpElicitationTitledSingleSelectEnumSchema{}, false
	}
	return *v.mcpElicitationTitledSingleSelectEnumSchema, true
}

func (v McpElicitationSingleSelectEnumSchema) MarshalJSON() ([]byte, error) {
	switch {
	case v.mcpElicitationUntitledSingleSelectEnumSchema != nil:
		return json.Marshal(v.mcpElicitationUntitledSingleSelectEnumSchema)
	case v.mcpElicitationTitledSingleSelectEnumSchema != nil:
		return json.Marshal(v.mcpElicitationTitledSingleSelectEnumSchema)
	case len(bytes.TrimSpace(v.RawJSON)) > 0:
		if !json.Valid(v.RawJSON) {
			return nil, fmt.Errorf("invalid McpElicitationSingleSelectEnumSchema raw fallback")
		}
		return append([]byte(nil), v.RawJSON...), nil
	default:
		return nil, DecodeError{Field: "", Reason: "empty McpElicitationSingleSelectEnumSchema"}
	}
}

func (v *McpElicitationSingleSelectEnumSchema) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if rawDiscriminator, ok := raw["type"]; ok {
		var discriminator string
		if err := json.Unmarshal(rawDiscriminator, &discriminator); err != nil {
			return fmt.Errorf("field type: %w", err)
		}
		matchedDiscriminator := false
		if discriminator == "string" {
			matchedDiscriminator = true
			var value McpElicitationUntitledSingleSelectEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationSingleSelectEnumSchema{mcpElicitationUntitledSingleSelectEnumSchema: &value}
				return nil
			}
		}
		if discriminator == "string" {
			matchedDiscriminator = true
			var value McpElicitationTitledSingleSelectEnumSchema
			if err := json.Unmarshal(data, &value); err == nil {
				*v = McpElicitationSingleSelectEnumSchema{mcpElicitationTitledSingleSelectEnumSchema: &value}
				return nil
			}
		}
		if matchedDiscriminator {
			return DecodeError{Field: "type", Reason: "no matching union variant"}
		}
	}
	*v = McpElicitationSingleSelectEnumSchema{RawJSON: append([]byte(nil), trimmed...)}
	return nil
}

type McpElicitationStringFormat string

const (
	McpElicitationStringFormatEmail    McpElicitationStringFormat = "email"
	McpElicitationStringFormatUri      McpElicitationStringFormat = "uri"
	McpElicitationStringFormatDate     McpElicitationStringFormat = "date"
	McpElicitationStringFormatDateTime McpElicitationStringFormat = "date-time"
)

type McpElicitationStringSchema struct {
	DefaultValue Optional[string]                     `json:"default,omitzero"`
	Description  Optional[string]                     `json:"description,omitzero"`
	Format       Optional[McpElicitationStringFormat] `json:"format,omitzero"`
	MaxLength    Optional[uint32]                     `json:"maxLength,omitzero"`
	MinLength    Optional[uint32]                     `json:"minLength,omitzero"`
	Title        Optional[string]                     `json:"title,omitzero"`
	TypeValue    McpElicitationStringType             `json:"type"`
}

var mcpElicitationStringSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "format"},
		{name: "maxLength"},
		{name: "minLength"},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationStringSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationStringSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationStringSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationStringSchema
	var s objectScan
	if _, err := s.begin(data, &mcpElicitationStringSchemaSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueMaxLength, ok := v.MaxLength.Value(); ok {
		if valueMaxLength < 0 {
			return DecodeError{Field: "maxLength", Reason: "below minimum 0"}
		}
	}
	if valueMinLength, ok := v.MinLength.Value(); ok {
		if valueMinLength < 0 {
			return DecodeError{Field: "minLength", Reason: "below minimum 0"}
		}
	}
	return nil
}

type McpElicitationStringType string

const (
	McpElicitationStringTypeString McpElicitationStringType = "string"
)

type McpElicitationTitledEnumItems struct {
	AnyOf []McpElicitationConstOption `json:"anyOf"`
}

var mcpElicitationTitledEnumItemsSpec = objectSpec{
	fields: []objectField{
		{name: "anyOf", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationTitledEnumItems) MarshalJSON() ([]byte, error) {
	type wire McpElicitationTitledEnumItems
	return json.Marshal(wire(v))
}

func (v *McpElicitationTitledEnumItems) UnmarshalJSON(data []byte) error {
	type wire McpElicitationTitledEnumItems
	return decodeObject(data, &mcpElicitationTitledEnumItemsSpec, (*wire)(v))
}

type McpElicitationTitledMultiSelectEnumSchema struct {
	DefaultValue Optional[[]string]            `json:"default,omitzero"`
	Description  Optional[string]              `json:"description,omitzero"`
	Items        McpElicitationTitledEnumItems `json:"items"`
	MaxItems     Optional[uint64]              `json:"maxItems,omitzero"`
	MinItems     Optional[uint64]              `json:"minItems,omitzero"`
	Title        Optional[string]              `json:"title,omitzero"`
	TypeValue    McpElicitationArrayType       `json:"type"`
}

var mcpElicitationTitledMultiSelectEnumSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "items", flags: fieldRequired | fieldRejectNull},
		{name: "maxItems"},
		{name: "minItems"},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationTitledMultiSelectEnumSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationTitledMultiSelectEnumSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationTitledMultiSelectEnumSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationTitledMultiSelectEnumSchema
	var s objectScan
	if _, err := s.begin(data, &mcpElicitationTitledMultiSelectEnumSchemaSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueMaxItems, ok := v.MaxItems.Value(); ok {
		if valueMaxItems < 0 {
			return DecodeError{Field: "maxItems", Reason: "below minimum 0"}
		}
	}
	if valueMinItems, ok := v.MinItems.Value(); ok {
		if valueMinItems < 0 {
			return DecodeError{Field: "minItems", Reason: "below minimum 0"}
		}
	}
	return nil
}

type McpElicitationTitledSingleSelectEnumSchema struct {
	DefaultValue Optional[string]            `json:"default,omitzero"`
	Description  Optional[string]            `json:"description,omitzero"`
	OneOf        []McpElicitationConstOption `json:"oneOf"`
	Title        Optional[string]            `json:"title,omitzero"`
	TypeValue    McpElicitationStringType    `json:"type"`
}

var mcpElicitationTitledSingleSelectEnumSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "oneOf", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationTitledSingleSelectEnumSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationTitledSingleSelectEnumSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationTitledSingleSelectEnumSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationTitledSingleSelectEnumSchema
	return decodeObject(data, &mcpElicitationTitledSingleSelectEnumSchemaSpec, (*wire)(v))
}

type McpElicitationUntitledEnumItems struct {
	Enum      []string                 `json:"enum"`
	TypeValue McpElicitationStringType `json:"type"`
}

var mcpElicitationUntitledEnumItemsSpec = objectSpec{
	fields: []objectField{
		{name: "enum", flags: fieldRequired | fieldRejectNull},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationUntitledEnumItems) MarshalJSON() ([]byte, error) {
	type wire McpElicitationUntitledEnumItems
	return json.Marshal(wire(v))
}

func (v *McpElicitationUntitledEnumItems) UnmarshalJSON(data []byte) error {
	type wire McpElicitationUntitledEnumItems
	return decodeObject(data, &mcpElicitationUntitledEnumItemsSpec, (*wire)(v))
}

type McpElicitationUntitledMultiSelectEnumSchema struct {
	DefaultValue Optional[[]string]              `json:"default,omitzero"`
	Description  Optional[string]                `json:"description,omitzero"`
	Items        McpElicitationUntitledEnumItems `json:"items"`
	MaxItems     Optional[uint64]                `json:"maxItems,omitzero"`
	MinItems     Optional[uint64]                `json:"minItems,omitzero"`
	Title        Optional[string]                `json:"title,omitzero"`
	TypeValue    McpElicitationArrayType         `json:"type"`
}

var mcpElicitationUntitledMultiSelectEnumSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "items", flags: fieldRequired | fieldRejectNull},
		{name: "maxItems"},
		{name: "minItems"},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationUntitledMultiSelectEnumSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationUntitledMultiSelectEnumSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationUntitledMultiSelectEnumSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationUntitledMultiSelectEnumSchema
	var s objectScan
	if _, err := s.begin(data, &mcpElicitationUntitledMultiSelectEnumSchemaSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueMaxItems, ok := v.MaxItems.Value(); ok {
		if valueMaxItems < 0 {
			return DecodeError{Field: "maxItems", Reason: "below minimum 0"}
		}
	}
	if valueMinItems, ok := v.MinItems.Value(); ok {
		if valueMinItems < 0 {
			return DecodeError{Field: "minItems", Reason: "below minimum 0"}
		}
	}
	return nil
}

type McpElicitationUntitledSingleSelectEnumSchema struct {
	DefaultValue Optional[string]         `json:"default,omitzero"`
	Description  Optional[string]         `json:"description,omitzero"`
	Enum         []string                 `json:"enum"`
	Title        Optional[string]         `json:"title,omitzero"`
	TypeValue    McpElicitationStringType `json:"type"`
}

var mcpElicitationUntitledSingleSelectEnumSchemaSpec = objectSpec{
	fields: []objectField{
		{name: "default"},
		{name: "description"},
		{name: "enum", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpElicitationUntitledSingleSelectEnumSchema) MarshalJSON() ([]byte, error) {
	type wire McpElicitationUntitledSingleSelectEnumSchema
	return json.Marshal(wire(v))
}

func (v *McpElicitationUntitledSingleSelectEnumSchema) UnmarshalJSON(data []byte) error {
	type wire McpElicitationUntitledSingleSelectEnumSchema
	return decodeObject(data, &mcpElicitationUntitledSingleSelectEnumSchemaSpec, (*wire)(v))
}

type McpServerElicitationAction string

const (
	McpServerElicitationActionAccept  McpServerElicitationAction = "accept"
	McpServerElicitationActionDecline McpServerElicitationAction = "decline"
	McpServerElicitationActionCancel  McpServerElicitationAction = "cancel"
)

type McpServerElicitationRequestParams struct {
	Meta            json.RawMessage            `json:"_meta,omitempty"`
	Challenge       OptionalNonNull[string]    `json:"challenge,omitzero"`
	Description     OptionalNonNull[string]    `json:"description,omitzero"`
	ElicitationID   OptionalNonNull[string]    `json:"elicitationId,omitzero"`
	Message         OptionalNonNull[string]    `json:"message,omitzero"`
	Mode            OptionalNonNull[string]    `json:"mode,omitzero"`
	RequestedSchema json.RawMessage            `json:"requestedSchema,omitempty"`
	ServerName      string                     `json:"serverName"`
	ThreadID        string                     `json:"threadId"`
	Title           OptionalNonNull[string]    `json:"title,omitzero"`
	TurnID          Optional[string]           `json:"turnId,omitzero"`
	URL             OptionalNonNull[string]    `json:"url,omitzero"`
	RawJSON         json.RawMessage            `json:"-"`
	Request         map[string]json.RawMessage `json:"-"`
}

var mcpServerElicitationRequestParamsSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "challenge", flags: fieldNonNullOptional},
		{name: "description", flags: fieldNonNullOptional},
		{name: "elicitationId", flags: fieldNonNullOptional},
		{name: "message", flags: fieldNonNullOptional},
		{name: "mode", flags: fieldNonNullOptional},
		{name: "requestedSchema"},
		{name: "serverName", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "title", flags: fieldNonNullOptional},
		{name: "turnId"},
		{name: "url", flags: fieldNonNullOptional},
	},
}

func (v McpServerElicitationRequestParams) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	for key, value := range v.Request {
		out[key] = value
	}
	if len(v.Meta) > 0 {
		out["_meta"] = v.Meta
	}
	if v.Challenge.IsSet() {
		out["challenge"] = v.Challenge
	}
	if v.Description.IsSet() {
		out["description"] = v.Description
	}
	if v.ElicitationID.IsSet() {
		out["elicitationId"] = v.ElicitationID
	}
	if v.Message.IsSet() {
		out["message"] = v.Message
	}
	if v.Mode.IsSet() {
		out["mode"] = v.Mode
	}
	if len(v.RequestedSchema) > 0 {
		out["requestedSchema"] = v.RequestedSchema
	}
	out["serverName"] = v.ServerName
	out["threadId"] = v.ThreadID
	if v.Title.IsSet() {
		out["title"] = v.Title
	}
	if v.TurnID.IsSet() {
		out["turnId"] = v.TurnID
	}
	if v.URL.IsSet() {
		out["url"] = v.URL
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	untaggedOneOfVariant0ConstMatches := true
	if _, ok := out["challenge"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if _, ok := out["description"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if _, ok := out["mode"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if _, ok := out["title"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if rawValue, ok := out["mode"]; !ok {
		untaggedOneOfVariant0Matches = false
		untaggedOneOfVariant0ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"openai/userVerification\"")) {
		untaggedOneOfVariant0Matches = false
		untaggedOneOfVariant0ConstMatches = false
	}
	if untaggedOneOfVariant0ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	untaggedOneOfVariant1ConstMatches := true
	if _, ok := out["message"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if _, ok := out["mode"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if _, ok := out["requestedSchema"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if rawValue, ok := out["requestedSchema"]; ok {
		if rawJSON, err := json.Marshal(rawValue); err != nil || bytes.Equal(bytes.TrimSpace(rawJSON), []byte("null")) {
			untaggedOneOfVariant1Matches = false
		}
	}
	if rawValue, ok := out["mode"]; !ok {
		untaggedOneOfVariant1Matches = false
		untaggedOneOfVariant1ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"form\"")) {
		untaggedOneOfVariant1Matches = false
		untaggedOneOfVariant1ConstMatches = false
	}
	if untaggedOneOfVariant1ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant2Matches := true
	untaggedOneOfVariant2ConstMatches := true
	if _, ok := out["message"]; !ok {
		untaggedOneOfVariant2Matches = false
	}
	if _, ok := out["mode"]; !ok {
		untaggedOneOfVariant2Matches = false
	}
	if _, ok := out["requestedSchema"]; !ok {
		untaggedOneOfVariant2Matches = false
	}
	if rawValue, ok := out["mode"]; !ok {
		untaggedOneOfVariant2Matches = false
		untaggedOneOfVariant2ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"openai/form\"")) {
		untaggedOneOfVariant2Matches = false
		untaggedOneOfVariant2ConstMatches = false
	}
	if untaggedOneOfVariant2ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant2Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant3Matches := true
	untaggedOneOfVariant3ConstMatches := true
	if _, ok := out["message"]; !ok {
		untaggedOneOfVariant3Matches = false
	}
	if _, ok := out["mode"]; !ok {
		untaggedOneOfVariant3Matches = false
	}
	if _, ok := out["requestedSchema"]; !ok {
		untaggedOneOfVariant3Matches = false
	}
	if rawValue, ok := out["mode"]; !ok {
		untaggedOneOfVariant3Matches = false
		untaggedOneOfVariant3ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"openaiForm\"")) {
		untaggedOneOfVariant3Matches = false
		untaggedOneOfVariant3ConstMatches = false
	}
	if untaggedOneOfVariant3ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant3Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant4Matches := true
	untaggedOneOfVariant4ConstMatches := true
	if _, ok := out["elicitationId"]; !ok {
		untaggedOneOfVariant4Matches = false
	}
	if _, ok := out["message"]; !ok {
		untaggedOneOfVariant4Matches = false
	}
	if _, ok := out["mode"]; !ok {
		untaggedOneOfVariant4Matches = false
	}
	if _, ok := out["url"]; !ok {
		untaggedOneOfVariant4Matches = false
	}
	if rawValue, ok := out["mode"]; !ok {
		untaggedOneOfVariant4Matches = false
		untaggedOneOfVariant4ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"url\"")) {
		untaggedOneOfVariant4Matches = false
		untaggedOneOfVariant4ConstMatches = false
	}
	if untaggedOneOfVariant4ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant4Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid McpServerElicitationRequestParams raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *McpServerElicitationRequestParams) UnmarshalJSON(data []byte) error {
	type wire McpServerElicitationRequestParams
	var s objectScan
	if _, err := s.begin(data, &mcpServerElicitationRequestParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.Request = s.unknownMembers()
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.rawEquals("mode", "\"openai/userVerification\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("challenge") && s.nonNull("description") && s.nonNull("mode") && s.nonNull("title") && s.rawEquals("mode", "\"openai/userVerification\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("mode", "\"form\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("message") && s.nonNull("mode") && s.nonNull("requestedSchema") && s.rawEquals("mode", "\"form\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("mode", "\"openai/form\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("message") && s.nonNull("mode") && s.has("requestedSchema") && s.rawEquals("mode", "\"openai/form\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("mode", "\"openaiForm\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("message") && s.nonNull("mode") && s.has("requestedSchema") && s.rawEquals("mode", "\"openaiForm\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("mode", "\"url\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("elicitationId") && s.nonNull("message") && s.nonNull("mode") && s.nonNull("url") && s.rawEquals("mode", "\"url\"") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type McpServerElicitationRequestResponse struct {
	Meta    json.RawMessage            `json:"_meta,omitempty"`
	Action  McpServerElicitationAction `json:"action"`
	Content json.RawMessage            `json:"content,omitempty"`
}

var mcpServerElicitationRequestResponseSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "action", flags: fieldRequired | fieldRejectNull},
		{name: "content"},
	},
}

func (v McpServerElicitationRequestResponse) MarshalJSON() ([]byte, error) {
	type wire McpServerElicitationRequestResponse
	return json.Marshal(wire(v))
}

func (v *McpServerElicitationRequestResponse) UnmarshalJSON(data []byte) error {
	type wire McpServerElicitationRequestResponse
	return decodeObject(data, &mcpServerElicitationRequestResponseSpec, (*wire)(v))
}

type NetworkApprovalContext struct {
	Host     string                  `json:"host"`
	Protocol NetworkApprovalProtocol `json:"protocol"`
}

var networkApprovalContextSpec = objectSpec{
	fields: []objectField{
		{name: "host", flags: fieldRequired | fieldRejectNull},
		{name: "protocol", flags: fieldRequired | fieldRejectNull},
	},
}

func (v NetworkApprovalContext) MarshalJSON() ([]byte, error) {
	type wire NetworkApprovalContext
	return json.Marshal(wire(v))
}

func (v *NetworkApprovalContext) UnmarshalJSON(data []byte) error {
	type wire NetworkApprovalContext
	return decodeObject(data, &networkApprovalContextSpec, (*wire)(v))
}

type NetworkPolicyAmendment struct {
	Action NetworkPolicyRuleAction `json:"action"`
	Host   string                  `json:"host"`
}

var networkPolicyAmendmentSpec = objectSpec{
	fields: []objectField{
		{name: "action", flags: fieldRequired | fieldRejectNull},
		{name: "host", flags: fieldRequired | fieldRejectNull},
	},
}

func (v NetworkPolicyAmendment) MarshalJSON() ([]byte, error) {
	type wire NetworkPolicyAmendment
	return json.Marshal(wire(v))
}

func (v *NetworkPolicyAmendment) UnmarshalJSON(data []byte) error {
	type wire NetworkPolicyAmendment
	return decodeObject(data, &networkPolicyAmendmentSpec, (*wire)(v))
}

type NetworkPolicyRuleAction string

const (
	NetworkPolicyRuleActionAllow NetworkPolicyRuleAction = "allow"
	NetworkPolicyRuleActionDeny  NetworkPolicyRuleAction = "deny"
)

type ParsedCommand struct {
	Cmd       OptionalNonNull[string] `json:"cmd,omitzero"`
	Name      OptionalNonNull[string] `json:"name,omitzero"`
	Path      Optional[string]        `json:"path,omitzero"`
	Query     Optional[string]        `json:"query,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var parsedCommandSpec = objectSpec{
	fields: []objectField{
		{name: "cmd", flags: fieldNonNullOptional},
		{name: "name", flags: fieldNonNullOptional},
		{name: "path"},
		{name: "query"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  4,
	tags: []string{"read", "list_files", "search", "unknown"},
}

func (v ParsedCommand) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "read":
		if !v.Cmd.IsSet() {
			return nil, DecodeError{Field: "cmd", Reason: "missing required field for type read"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type read"}
		}
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type read"}
		}
		if v.Path.IsNull() {
			return nil, DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "list_files":
		if !v.Cmd.IsSet() {
			return nil, DecodeError{Field: "cmd", Reason: "missing required field for type list_files"}
		}
	case "search":
		if !v.Cmd.IsSet() {
			return nil, DecodeError{Field: "cmd", Reason: "missing required field for type search"}
		}
	case "unknown":
		if !v.Cmd.IsSet() {
			return nil, DecodeError{Field: "cmd", Reason: "missing required field for type unknown"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ParsedCommand
	return json.Marshal(wire(v))
}

func (v *ParsedCommand) UnmarshalJSON(data []byte) error {
	type wire ParsedCommand
	var s objectScan
	known, err := s.begin(data, &parsedCommandSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "read":
		if !s.has("cmd") {
			return DecodeError{Field: "cmd", Reason: "missing required field for type read"}
		}
		if s.isNull("cmd") {
			return DecodeError{Field: "cmd", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type read"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type read"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "list_files":
		if !s.has("cmd") {
			return DecodeError{Field: "cmd", Reason: "missing required field for type list_files"}
		}
		if s.isNull("cmd") {
			return DecodeError{Field: "cmd", Reason: "cannot be null"}
		}
	case "search":
		if !s.has("cmd") {
			return DecodeError{Field: "cmd", Reason: "missing required field for type search"}
		}
		if s.isNull("cmd") {
			return DecodeError{Field: "cmd", Reason: "cannot be null"}
		}
	case "unknown":
		if !s.has("cmd") {
			return DecodeError{Field: "cmd", Reason: "missing required field for type unknown"}
		}
		if s.isNull("cmd") {
			return DecodeError{Field: "cmd", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type PermissionGrantScope string

const (
	PermissionGrantScopeTurn    PermissionGrantScope = "turn"
	PermissionGrantScopeSession PermissionGrantScope = "session"
)

type PermissionsRequestApprovalParams struct {
	Cwd           LegacyAppPathString      `json:"cwd"`
	EnvironmentID Optional[string]         `json:"environmentId,omitzero"`
	ItemID        string                   `json:"itemId"`
	Permissions   RequestPermissionProfile `json:"permissions"`
	Reason        Optional[string]         `json:"reason,omitzero"`
	StartedAtMs   int64                    `json:"startedAtMs"`
	ThreadID      string                   `json:"threadId"`
	TurnID        string                   `json:"turnId"`
}

var permissionsRequestApprovalParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "environmentId", defaultJSON: "null"},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "permissions", flags: fieldRequired | fieldRejectNull},
		{name: "reason"},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PermissionsRequestApprovalParams) MarshalJSON() ([]byte, error) {
	type wire PermissionsRequestApprovalParams
	return json.Marshal(wire(v))
}

func (v *PermissionsRequestApprovalParams) UnmarshalJSON(data []byte) error {
	type wire PermissionsRequestApprovalParams
	return decodeObject(data, &permissionsRequestApprovalParamsSpec, (*wire)(v))
}

type PermissionsRequestApprovalResponse struct {
	Permissions      GrantedPermissionProfile              `json:"permissions"`
	Scope            OptionalNonNull[PermissionGrantScope] `json:"scope,omitzero"`
	StrictAutoReview Optional[bool]                        `json:"strictAutoReview,omitzero"`
}

var permissionsRequestApprovalResponseSpec = objectSpec{
	fields: []objectField{
		{name: "permissions", flags: fieldRequired | fieldRejectNull},
		{name: "scope", flags: fieldNonNullOptional, defaultJSON: "\"turn\""},
		{name: "strictAutoReview"},
	},
}

func (v PermissionsRequestApprovalResponse) MarshalJSON() ([]byte, error) {
	type wire PermissionsRequestApprovalResponse
	w := wire(v)
	w.StrictAutoReview = w.StrictAutoReview.withoutNull()
	return json.Marshal(w)
}

func (v *PermissionsRequestApprovalResponse) UnmarshalJSON(data []byte) error {
	type wire PermissionsRequestApprovalResponse
	return decodeObject(data, &permissionsRequestApprovalResponseSpec, (*wire)(v))
}

type ReviewDecision struct {
	StringValue                 string                                      `json:"-"`
	ApprovedExecpolicyAmendment OptionalNonNull[map[string]json.RawMessage] `json:"approved_execpolicy_amendment,omitzero"`
	Denied                      OptionalNonNull[map[string]json.RawMessage] `json:"denied,omitzero"`
	NetworkPolicyAmendment      OptionalNonNull[map[string]json.RawMessage] `json:"network_policy_amendment,omitzero"`
	RawJSON                     json.RawMessage                             `json:"-"`
}

var (
	ReviewDecisionApproved                   = ReviewDecision{StringValue: "approved"}
	ReviewDecisionApprovedForSession         = ReviewDecision{StringValue: "approved_for_session"}
	ReviewDecisionApprovedMcpPolicyAmendment = ReviewDecision{StringValue: "approved_mcp_policy_amendment"}
	ReviewDecisionTimedOut                   = ReviewDecision{StringValue: "timed_out"}
	ReviewDecisionAbort                      = ReviewDecision{StringValue: "abort"}
)

func (v ReviewDecision) IsSet() bool {
	return v.StringValue != "" || len(bytes.TrimSpace(v.RawJSON)) > 0 || v.ApprovedExecpolicyAmendment.IsSet() || v.Denied.IsSet() || v.NetworkPolicyAmendment.IsSet()
}

var reviewDecisionSpec = objectSpec{
	fields: []objectField{
		{name: "approved_execpolicy_amendment", flags: fieldNonNullOptional},
		{name: "denied", flags: fieldNonNullOptional},
		{name: "network_policy_amendment", flags: fieldNonNullOptional},
	},
	fieldOffset: 1,
}

func (v ReviewDecision) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.ApprovedExecpolicyAmendment.IsSet() {
		out["approved_execpolicy_amendment"] = v.ApprovedExecpolicyAmendment
	}
	if v.Denied.IsSet() {
		out["denied"] = v.Denied
	}
	if v.NetworkPolicyAmendment.IsSet() {
		out["network_policy_amendment"] = v.NetworkPolicyAmendment
	}
	if v.StringValue != "" {
		if len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 {
			return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
		}
		switch v.StringValue {
		case "approved", "approved_for_session", "approved_mcp_policy_amendment", "timed_out", "abort":
			return json.Marshal(v.StringValue)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported ReviewDecision string value %q", v.StringValue)}
		}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	if _, ok := out["approved_execpolicy_amendment"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	if _, ok := out["network_policy_amendment"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant2Matches := true
	if _, ok := out["denied"]; !ok {
		untaggedOneOfVariant2Matches = false
	}
	if untaggedOneOfVariant2Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid ReviewDecision raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *ReviewDecision) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var stringValue string
	if err := json.Unmarshal(trimmed, &stringValue); err == nil {
		switch stringValue {
		case "approved", "approved_for_session", "approved_mcp_policy_amendment", "timed_out", "abort":
			*v = ReviewDecision{StringValue: stringValue}
		default:
			*v = ReviewDecision{RawJSON: append(json.RawMessage(nil), data...)}
		}
		return nil
	}
	v.StringValue = ""
	type wire ReviewDecision
	var s objectScan
	if _, err := s.begin(data, &reviewDecisionSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.nonNull("approved_execpolicy_amendment") {
		untaggedOneOfMatches++
	}
	if s.nonNull("network_policy_amendment") {
		untaggedOneOfMatches++
	}
	if s.nonNull("denied") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type ServerNotification struct {
	Method string          `json:"method"`
	Params json.RawMessage `json:"params,omitempty"`
}

func (v ServerNotification) MarshalJSON() ([]byte, error) {
	type wire struct {
		Method string          `json:"method"`
		Params json.RawMessage `json:"params,omitempty"`
	}
	return json.Marshal(wire{Method: v.Method, Params: v.Params})
}

func (v *ServerNotification) UnmarshalJSON(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	rawMethod, ok := raw["method"]
	if !ok {
		return DecodeError{Field: "method", Reason: "missing required field"}
	}
	if bytes.Equal(rawMethod, []byte("null")) {
		return DecodeError{Field: "method", Reason: "cannot be null"}
	}
	if err := json.Unmarshal(rawMethod, &v.Method); err != nil {
		return fmt.Errorf("field method: %w", err)
	}
	v.Params = nil
	if rawParams, ok := raw["params"]; ok {
		v.Params = append(v.Params[:0], rawParams...)
	}
	return nil
}

func (v ServerNotification) ErrorParams() (ErrorNotification, bool, error) {
	if v.Method != "error" {
		return ErrorNotification{}, false, nil
	}
	var params ErrorNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadStartedParams() (ThreadStartedNotification, bool, error) {
	if v.Method != "thread/started" {
		return ThreadStartedNotification{}, false, nil
	}
	var params ThreadStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadStatusChangedParams() (ThreadStatusChangedNotification, bool, error) {
	if v.Method != "thread/status/changed" {
		return ThreadStatusChangedNotification{}, false, nil
	}
	var params ThreadStatusChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadReadStateChangedParams() (ThreadReadStateChangedNotification, bool, error) {
	if v.Method != "thread/readState/changed" {
		return ThreadReadStateChangedNotification{}, false, nil
	}
	var params ThreadReadStateChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadArchivedParams() (ThreadArchivedNotification, bool, error) {
	if v.Method != "thread/archived" {
		return ThreadArchivedNotification{}, false, nil
	}
	var params ThreadArchivedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadDeletedParams() (ThreadDeletedNotification, bool, error) {
	if v.Method != "thread/deleted" {
		return ThreadDeletedNotification{}, false, nil
	}
	var params ThreadDeletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadUnarchivedParams() (ThreadUnarchivedNotification, bool, error) {
	if v.Method != "thread/unarchived" {
		return ThreadUnarchivedNotification{}, false, nil
	}
	var params ThreadUnarchivedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadClosedParams() (ThreadClosedNotification, bool, error) {
	if v.Method != "thread/closed" {
		return ThreadClosedNotification{}, false, nil
	}
	var params ThreadClosedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRevertedParams() (ThreadRevertedNotification, bool, error) {
	if v.Method != "thread/reverted" {
		return ThreadRevertedNotification{}, false, nil
	}
	var params ThreadRevertedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) SkillsChangedParams() (SkillsChangedNotification, bool, error) {
	if v.Method != "skills/changed" {
		return SkillsChangedNotification{}, false, nil
	}
	var params SkillsChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadNameUpdatedParams() (ThreadNameUpdatedNotification, bool, error) {
	if v.Method != "thread/name/updated" {
		return ThreadNameUpdatedNotification{}, false, nil
	}
	var params ThreadNameUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadAttachmentUpdatedParams() (ThreadAttachmentUpdatedNotification, bool, error) {
	if v.Method != "thread/attachment/updated" {
		return ThreadAttachmentUpdatedNotification{}, false, nil
	}
	var params ThreadAttachmentUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadGoalUpdatedParams() (ThreadGoalUpdatedNotification, bool, error) {
	if v.Method != "thread/goal/updated" {
		return ThreadGoalUpdatedNotification{}, false, nil
	}
	var params ThreadGoalUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadPredictionUpdatedParams() (ThreadPredictionUpdatedNotification, bool, error) {
	if v.Method != "thread/prediction/updated" {
		return ThreadPredictionUpdatedNotification{}, false, nil
	}
	var params ThreadPredictionUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadGoalClearedParams() (ThreadGoalClearedNotification, bool, error) {
	if v.Method != "thread/goal/cleared" {
		return ThreadGoalClearedNotification{}, false, nil
	}
	var params ThreadGoalClearedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadQueueChangedParams() (ThreadQueueChangedNotification, bool, error) {
	if v.Method != "thread/queue/changed" {
		return ThreadQueueChangedNotification{}, false, nil
	}
	var params ThreadQueueChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadWorkflowUpdatedParams() (ThreadWorkflowUpdatedNotification, bool, error) {
	if v.Method != "thread/workflow/updated" {
		return ThreadWorkflowUpdatedNotification{}, false, nil
	}
	var params ThreadWorkflowUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ProjectChangedParams() (ProjectChangedNotification, bool, error) {
	if v.Method != "project/changed" {
		return ProjectChangedNotification{}, false, nil
	}
	var params ProjectChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadProjectUpdatedParams() (ThreadProjectUpdatedNotification, bool, error) {
	if v.Method != "thread/project/updated" {
		return ThreadProjectUpdatedNotification{}, false, nil
	}
	var params ThreadProjectUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadEnvironmentConnectedParams() (EnvironmentConnectionNotification, bool, error) {
	if v.Method != "thread/environment/connected" {
		return EnvironmentConnectionNotification{}, false, nil
	}
	var params EnvironmentConnectionNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadEnvironmentDisconnectedParams() (EnvironmentConnectionNotification, bool, error) {
	if v.Method != "thread/environment/disconnected" {
		return EnvironmentConnectionNotification{}, false, nil
	}
	var params EnvironmentConnectionNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadSettingsUpdatedParams() (ThreadSettingsUpdatedNotification, bool, error) {
	if v.Method != "thread/settings/updated" {
		return ThreadSettingsUpdatedNotification{}, false, nil
	}
	var params ThreadSettingsUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadTokenUsageUpdatedParams() (ThreadTokenUsageUpdatedNotification, bool, error) {
	if v.Method != "thread/tokenUsage/updated" {
		return ThreadTokenUsageUpdatedNotification{}, false, nil
	}
	var params ThreadTokenUsageUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) TurnStartedParams() (TurnStartedNotification, bool, error) {
	if v.Method != "turn/started" {
		return TurnStartedNotification{}, false, nil
	}
	var params TurnStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) HookStartedParams() (HookStartedNotification, bool, error) {
	if v.Method != "hook/started" {
		return HookStartedNotification{}, false, nil
	}
	var params HookStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) TurnCompletedParams() (TurnCompletedNotification, bool, error) {
	if v.Method != "turn/completed" {
		return TurnCompletedNotification{}, false, nil
	}
	var params TurnCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) HookCompletedParams() (HookCompletedNotification, bool, error) {
	if v.Method != "hook/completed" {
		return HookCompletedNotification{}, false, nil
	}
	var params HookCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) TurnDiffUpdatedParams() (TurnDiffUpdatedNotification, bool, error) {
	if v.Method != "turn/diff/updated" {
		return TurnDiffUpdatedNotification{}, false, nil
	}
	var params TurnDiffUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) TurnPlanUpdatedParams() (TurnPlanUpdatedNotification, bool, error) {
	if v.Method != "turn/plan/updated" {
		return TurnPlanUpdatedNotification{}, false, nil
	}
	var params TurnPlanUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemStartedParams() (ItemStartedNotification, bool, error) {
	if v.Method != "item/started" {
		return ItemStartedNotification{}, false, nil
	}
	var params ItemStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemAutoApprovalReviewStartedParams() (ItemGuardianApprovalReviewStartedNotification, bool, error) {
	if v.Method != "item/autoApprovalReview/started" {
		return ItemGuardianApprovalReviewStartedNotification{}, false, nil
	}
	var params ItemGuardianApprovalReviewStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemAutoApprovalReviewCompletedParams() (ItemGuardianApprovalReviewCompletedNotification, bool, error) {
	if v.Method != "item/autoApprovalReview/completed" {
		return ItemGuardianApprovalReviewCompletedNotification{}, false, nil
	}
	var params ItemGuardianApprovalReviewCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) AutoApprovalReviewStrictReviewRequiredParams() (StrictReviewRequiredNotification, bool, error) {
	if v.Method != "autoApprovalReview/strictReviewRequired" {
		return StrictReviewRequiredNotification{}, false, nil
	}
	var params StrictReviewRequiredNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemCompletedParams() (ItemCompletedNotification, bool, error) {
	if v.Method != "item/completed" {
		return ItemCompletedNotification{}, false, nil
	}
	var params ItemCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemAgentMessageDeltaParams() (AgentMessageDeltaNotification, bool, error) {
	if v.Method != "item/agentMessage/delta" {
		return AgentMessageDeltaNotification{}, false, nil
	}
	var params AgentMessageDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemPlanDeltaParams() (PlanDeltaNotification, bool, error) {
	if v.Method != "item/plan/delta" {
		return PlanDeltaNotification{}, false, nil
	}
	var params PlanDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) CommandExecOutputDeltaParams() (CommandExecOutputDeltaNotification, bool, error) {
	if v.Method != "command/exec/outputDelta" {
		return CommandExecOutputDeltaNotification{}, false, nil
	}
	var params CommandExecOutputDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ProcessOutputDeltaParams() (ProcessOutputDeltaNotification, bool, error) {
	if v.Method != "process/outputDelta" {
		return ProcessOutputDeltaNotification{}, false, nil
	}
	var params ProcessOutputDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ProcessExitedParams() (ProcessExitedNotification, bool, error) {
	if v.Method != "process/exited" {
		return ProcessExitedNotification{}, false, nil
	}
	var params ProcessExitedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemCommandExecutionOutputDeltaParams() (CommandExecutionOutputDeltaNotification, bool, error) {
	if v.Method != "item/commandExecution/outputDelta" {
		return CommandExecutionOutputDeltaNotification{}, false, nil
	}
	var params CommandExecutionOutputDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemCommandExecutionTerminalInteractionParams() (TerminalInteractionNotification, bool, error) {
	if v.Method != "item/commandExecution/terminalInteraction" {
		return TerminalInteractionNotification{}, false, nil
	}
	var params TerminalInteractionNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemFileChangeOutputDeltaParams() (FileChangeOutputDeltaNotification, bool, error) {
	if v.Method != "item/fileChange/outputDelta" {
		return FileChangeOutputDeltaNotification{}, false, nil
	}
	var params FileChangeOutputDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemFileChangePatchUpdatedParams() (FileChangePatchUpdatedNotification, bool, error) {
	if v.Method != "item/fileChange/patchUpdated" {
		return FileChangePatchUpdatedNotification{}, false, nil
	}
	var params FileChangePatchUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ServerRequestResolvedParams() (ServerRequestResolvedNotification, bool, error) {
	if v.Method != "serverRequest/resolved" {
		return ServerRequestResolvedNotification{}, false, nil
	}
	var params ServerRequestResolvedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemMcpToolCallProgressParams() (McpToolCallProgressNotification, bool, error) {
	if v.Method != "item/mcpToolCall/progress" {
		return McpToolCallProgressNotification{}, false, nil
	}
	var params McpToolCallProgressNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) McpServerOauthLoginCompletedParams() (McpServerOauthLoginCompletedNotification, bool, error) {
	if v.Method != "mcpServer/oauthLogin/completed" {
		return McpServerOauthLoginCompletedNotification{}, false, nil
	}
	var params McpServerOauthLoginCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) McpServerStartupStatusUpdatedParams() (McpServerStatusUpdatedNotification, bool, error) {
	if v.Method != "mcpServer/startupStatus/updated" {
		return McpServerStatusUpdatedNotification{}, false, nil
	}
	var params McpServerStatusUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) McpServerEventStreamNotificationParams() (McpServerEventStreamNotification, bool, error) {
	if v.Method != "mcpServer/event/stream/notification" {
		return McpServerEventStreamNotification{}, false, nil
	}
	var params McpServerEventStreamNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) AccountUpdatedParams() (AccountUpdatedNotification, bool, error) {
	if v.Method != "account/updated" {
		return AccountUpdatedNotification{}, false, nil
	}
	var params AccountUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) AccountGatewayOAuthChangedParams() (GatewayOAuthChangedNotification, bool, error) {
	if v.Method != "account/gatewayOAuth/changed" {
		return GatewayOAuthChangedNotification{}, false, nil
	}
	var params GatewayOAuthChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) AccountRateLimitsUpdatedParams() (AccountRateLimitsUpdatedNotification, bool, error) {
	if v.Method != "account/rateLimits/updated" {
		return AccountRateLimitsUpdatedNotification{}, false, nil
	}
	var params AccountRateLimitsUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) AppListUpdatedParams() (AppListUpdatedNotification, bool, error) {
	if v.Method != "app/list/updated" {
		return AppListUpdatedNotification{}, false, nil
	}
	var params AppListUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) RemoteControlStatusChangedParams() (RemoteControlStatusChangedNotification, bool, error) {
	if v.Method != "remoteControl/status/changed" {
		return RemoteControlStatusChangedNotification{}, false, nil
	}
	var params RemoteControlStatusChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ExternalAgentConfigImportProgressParams() (ExternalAgentConfigImportProgressNotification, bool, error) {
	if v.Method != "externalAgentConfig/import/progress" {
		return ExternalAgentConfigImportProgressNotification{}, false, nil
	}
	var params ExternalAgentConfigImportProgressNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ExternalAgentConfigImportCompletedParams() (ExternalAgentConfigImportCompletedNotification, bool, error) {
	if v.Method != "externalAgentConfig/import/completed" {
		return ExternalAgentConfigImportCompletedNotification{}, false, nil
	}
	var params ExternalAgentConfigImportCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) FsChangedParams() (FsChangedNotification, bool, error) {
	if v.Method != "fs/changed" {
		return FsChangedNotification{}, false, nil
	}
	var params FsChangedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemReasoningSummaryTextDeltaParams() (ReasoningSummaryTextDeltaNotification, bool, error) {
	if v.Method != "item/reasoning/summaryTextDelta" {
		return ReasoningSummaryTextDeltaNotification{}, false, nil
	}
	var params ReasoningSummaryTextDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemReasoningSummaryPartAddedParams() (ReasoningSummaryPartAddedNotification, bool, error) {
	if v.Method != "item/reasoning/summaryPartAdded" {
		return ReasoningSummaryPartAddedNotification{}, false, nil
	}
	var params ReasoningSummaryPartAddedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ItemReasoningTextDeltaParams() (ReasoningTextDeltaNotification, bool, error) {
	if v.Method != "item/reasoning/textDelta" {
		return ReasoningTextDeltaNotification{}, false, nil
	}
	var params ReasoningTextDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadCompactedParams() (ContextCompactedNotification, bool, error) {
	if v.Method != "thread/compacted" {
		return ContextCompactedNotification{}, false, nil
	}
	var params ContextCompactedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ModelReroutedParams() (ModelReroutedNotification, bool, error) {
	if v.Method != "model/rerouted" {
		return ModelReroutedNotification{}, false, nil
	}
	var params ModelReroutedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ModelVerificationParams() (ModelVerificationNotification, bool, error) {
	if v.Method != "model/verification" {
		return ModelVerificationNotification{}, false, nil
	}
	var params ModelVerificationNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ModelProviderAuthRecoveryStartedParams() (AuthRecoveryNotification, bool, error) {
	if v.Method != "modelProvider/authRecoveryStarted" {
		return AuthRecoveryNotification{}, false, nil
	}
	var params AuthRecoveryNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ModelProviderAuthRecoveryCompletedParams() (AuthRecoveryNotification, bool, error) {
	if v.Method != "modelProvider/authRecoveryCompleted" {
		return AuthRecoveryNotification{}, false, nil
	}
	var params AuthRecoveryNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) TurnModerationMetadataParams() (TurnModerationMetadataNotification, bool, error) {
	if v.Method != "turn/moderationMetadata" {
		return TurnModerationMetadataNotification{}, false, nil
	}
	var params TurnModerationMetadataNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ModelSafetyBufferingUpdatedParams() (ModelSafetyBufferingUpdatedNotification, bool, error) {
	if v.Method != "model/safetyBuffering/updated" {
		return ModelSafetyBufferingUpdatedNotification{}, false, nil
	}
	var params ModelSafetyBufferingUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) WarningParams() (WarningNotification, bool, error) {
	if v.Method != "warning" {
		return WarningNotification{}, false, nil
	}
	var params WarningNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) GuardianWarningParams() (GuardianWarningNotification, bool, error) {
	if v.Method != "guardianWarning" {
		return GuardianWarningNotification{}, false, nil
	}
	var params GuardianWarningNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) DeprecationNoticeParams() (DeprecationNoticeNotification, bool, error) {
	if v.Method != "deprecationNotice" {
		return DeprecationNoticeNotification{}, false, nil
	}
	var params DeprecationNoticeNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ConfigWarningParams() (ConfigWarningNotification, bool, error) {
	if v.Method != "configWarning" {
		return ConfigWarningNotification{}, false, nil
	}
	var params ConfigWarningNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) FuzzyFileSearchSessionUpdatedParams() (FuzzyFileSearchSessionUpdatedNotification, bool, error) {
	if v.Method != "fuzzyFileSearch/sessionUpdated" {
		return FuzzyFileSearchSessionUpdatedNotification{}, false, nil
	}
	var params FuzzyFileSearchSessionUpdatedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) FuzzyFileSearchSessionCompletedParams() (FuzzyFileSearchSessionCompletedNotification, bool, error) {
	if v.Method != "fuzzyFileSearch/sessionCompleted" {
		return FuzzyFileSearchSessionCompletedNotification{}, false, nil
	}
	var params FuzzyFileSearchSessionCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeStartedParams() (ThreadRealtimeStartedNotification, bool, error) {
	if v.Method != "thread/realtime/started" {
		return ThreadRealtimeStartedNotification{}, false, nil
	}
	var params ThreadRealtimeStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeItemAddedParams() (ThreadRealtimeItemAddedNotification, bool, error) {
	if v.Method != "thread/realtime/itemAdded" {
		return ThreadRealtimeItemAddedNotification{}, false, nil
	}
	var params ThreadRealtimeItemAddedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeItemStartedParams() (ThreadRealtimeItemStartedNotification, bool, error) {
	if v.Method != "thread/realtime/item/started" {
		return ThreadRealtimeItemStartedNotification{}, false, nil
	}
	var params ThreadRealtimeItemStartedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeItemTranscriptDeltaParams() (ThreadRealtimeItemTranscriptDeltaNotification, bool, error) {
	if v.Method != "thread/realtime/item/transcript/delta" {
		return ThreadRealtimeItemTranscriptDeltaNotification{}, false, nil
	}
	var params ThreadRealtimeItemTranscriptDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeItemCompletedParams() (ThreadRealtimeItemCompletedNotification, bool, error) {
	if v.Method != "thread/realtime/item/completed" {
		return ThreadRealtimeItemCompletedNotification{}, false, nil
	}
	var params ThreadRealtimeItemCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeTranscriptDeltaParams() (ThreadRealtimeTranscriptDeltaNotification, bool, error) {
	if v.Method != "thread/realtime/transcript/delta" {
		return ThreadRealtimeTranscriptDeltaNotification{}, false, nil
	}
	var params ThreadRealtimeTranscriptDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeTranscriptDoneParams() (ThreadRealtimeTranscriptDoneNotification, bool, error) {
	if v.Method != "thread/realtime/transcript/done" {
		return ThreadRealtimeTranscriptDoneNotification{}, false, nil
	}
	var params ThreadRealtimeTranscriptDoneNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeOutputAudioDeltaParams() (ThreadRealtimeOutputAudioDeltaNotification, bool, error) {
	if v.Method != "thread/realtime/outputAudio/delta" {
		return ThreadRealtimeOutputAudioDeltaNotification{}, false, nil
	}
	var params ThreadRealtimeOutputAudioDeltaNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeSdpParams() (ThreadRealtimeSdpNotification, bool, error) {
	if v.Method != "thread/realtime/sdp" {
		return ThreadRealtimeSdpNotification{}, false, nil
	}
	var params ThreadRealtimeSdpNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeErrorParams() (ThreadRealtimeErrorNotification, bool, error) {
	if v.Method != "thread/realtime/error" {
		return ThreadRealtimeErrorNotification{}, false, nil
	}
	var params ThreadRealtimeErrorNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) ThreadRealtimeClosedParams() (ThreadRealtimeClosedNotification, bool, error) {
	if v.Method != "thread/realtime/closed" {
		return ThreadRealtimeClosedNotification{}, false, nil
	}
	var params ThreadRealtimeClosedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) WindowsWorldWritableWarningParams() (WindowsWorldWritableWarningNotification, bool, error) {
	if v.Method != "windows/worldWritableWarning" {
		return WindowsWorldWritableWarningNotification{}, false, nil
	}
	var params WindowsWorldWritableWarningNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) WindowsSandboxSetupCompletedParams() (WindowsSandboxSetupCompletedNotification, bool, error) {
	if v.Method != "windowsSandbox/setupCompleted" {
		return WindowsSandboxSetupCompletedNotification{}, false, nil
	}
	var params WindowsSandboxSetupCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerNotification) AccountLoginCompletedParams() (AccountLoginCompletedNotification, bool, error) {
	if v.Method != "account/login/completed" {
		return AccountLoginCompletedNotification{}, false, nil
	}
	var params AccountLoginCompletedNotification
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

type ServerRequest struct {
	ID     RequestID       `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params,omitempty"`
}

func (v ServerRequest) MarshalJSON() ([]byte, error) {
	type wire struct {
		ID     RequestID       `json:"id"`
		Method string          `json:"method"`
		Params json.RawMessage `json:"params,omitempty"`
	}
	return json.Marshal(wire{ID: v.ID, Method: v.Method, Params: v.Params})
}

func (v *ServerRequest) UnmarshalJSON(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	rawID, ok := raw["id"]
	if !ok {
		return DecodeError{Field: "id", Reason: "missing required field"}
	}
	if err := json.Unmarshal(rawID, &v.ID); err != nil {
		return fmt.Errorf("field id: %w", err)
	}
	rawMethod, ok := raw["method"]
	if !ok {
		return DecodeError{Field: "method", Reason: "missing required field"}
	}
	if bytes.Equal(rawMethod, []byte("null")) {
		return DecodeError{Field: "method", Reason: "cannot be null"}
	}
	if err := json.Unmarshal(rawMethod, &v.Method); err != nil {
		return fmt.Errorf("field method: %w", err)
	}
	v.Params = nil
	if rawParams, ok := raw["params"]; ok {
		v.Params = append(v.Params[:0], rawParams...)
	}
	return nil
}

func (v ServerRequest) ItemCommandExecutionRequestApprovalParams() (CommandExecutionRequestApprovalParams, bool, error) {
	if v.Method != "item/commandExecution/requestApproval" {
		return CommandExecutionRequestApprovalParams{}, false, nil
	}
	var params CommandExecutionRequestApprovalParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) ItemFileChangeRequestApprovalParams() (FileChangeRequestApprovalParams, bool, error) {
	if v.Method != "item/fileChange/requestApproval" {
		return FileChangeRequestApprovalParams{}, false, nil
	}
	var params FileChangeRequestApprovalParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) ItemToolRequestUserInputParams() (ToolRequestUserInputParams, bool, error) {
	if v.Method != "item/tool/requestUserInput" {
		return ToolRequestUserInputParams{}, false, nil
	}
	var params ToolRequestUserInputParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) McpServerElicitationRequestParams() (McpServerElicitationRequestParams, bool, error) {
	if v.Method != "mcpServer/elicitation/request" {
		return McpServerElicitationRequestParams{}, false, nil
	}
	var params McpServerElicitationRequestParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) ItemPermissionsRequestApprovalParams() (PermissionsRequestApprovalParams, bool, error) {
	if v.Method != "item/permissions/requestApproval" {
		return PermissionsRequestApprovalParams{}, false, nil
	}
	var params PermissionsRequestApprovalParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) ItemToolCallParams() (DynamicToolCallParams, bool, error) {
	if v.Method != "item/tool/call" {
		return DynamicToolCallParams{}, false, nil
	}
	var params DynamicToolCallParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) AccountChatgptAuthTokensRefreshParams() (ChatgptAuthTokensRefreshParams, bool, error) {
	if v.Method != "account/chatgptAuthTokens/refresh" {
		return ChatgptAuthTokensRefreshParams{}, false, nil
	}
	var params ChatgptAuthTokensRefreshParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) AttestationGenerateParams() (AttestationGenerateParams, bool, error) {
	if v.Method != "attestation/generate" {
		return AttestationGenerateParams{}, false, nil
	}
	var params AttestationGenerateParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

func (v ServerRequest) CurrentTimeReadParams() (CurrentTimeReadParams, bool, error) {
	if v.Method != "currentTime/read" {
		return CurrentTimeReadParams{}, false, nil
	}
	var params CurrentTimeReadParams
	if len(bytes.TrimSpace(v.Params)) == 0 {
		return params, true, DecodeError{Field: "params", Reason: "missing required field"}
	}
	if err := json.Unmarshal(v.Params, &params); err != nil {
		return params, true, err
	}
	return params, true, nil
}

type ToolRequestUserInputAnswer struct {
	Answers []string `json:"answers"`
}

var toolRequestUserInputAnswerSpec = objectSpec{
	fields: []objectField{
		{name: "answers", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ToolRequestUserInputAnswer) MarshalJSON() ([]byte, error) {
	type wire ToolRequestUserInputAnswer
	return json.Marshal(wire(v))
}

func (v *ToolRequestUserInputAnswer) UnmarshalJSON(data []byte) error {
	type wire ToolRequestUserInputAnswer
	return decodeObject(data, &toolRequestUserInputAnswerSpec, (*wire)(v))
}

type ToolRequestUserInputOption struct {
	Description string `json:"description"`
	Label       string `json:"label"`
}

var toolRequestUserInputOptionSpec = objectSpec{
	fields: []objectField{
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "label", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ToolRequestUserInputOption) MarshalJSON() ([]byte, error) {
	type wire ToolRequestUserInputOption
	return json.Marshal(wire(v))
}

func (v *ToolRequestUserInputOption) UnmarshalJSON(data []byte) error {
	type wire ToolRequestUserInputOption
	return decodeObject(data, &toolRequestUserInputOptionSpec, (*wire)(v))
}

type ToolRequestUserInputParams struct {
	AutoResolutionMs Optional[uint64]               `json:"autoResolutionMs,omitzero"`
	IsBlocking       bool                           `json:"isBlocking"`
	ItemID           string                         `json:"itemId"`
	Questions        []ToolRequestUserInputQuestion `json:"questions"`
	ThreadID         string                         `json:"threadId"`
	TurnID           string                         `json:"turnId"`
}

var toolRequestUserInputParamsSpec = objectSpec{
	fields: []objectField{
		{name: "autoResolutionMs", defaultJSON: "null"},
		{name: "isBlocking", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "questions", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ToolRequestUserInputParams) MarshalJSON() ([]byte, error) {
	type wire ToolRequestUserInputParams
	return json.Marshal(wire(v))
}

func (v *ToolRequestUserInputParams) UnmarshalJSON(data []byte) error {
	type wire ToolRequestUserInputParams
	var s objectScan
	if _, err := s.begin(data, &toolRequestUserInputParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueAutoResolutionMs, ok := v.AutoResolutionMs.Value(); ok {
		if valueAutoResolutionMs < 0 {
			return DecodeError{Field: "autoResolutionMs", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ToolRequestUserInputQuestion struct {
	Header   string                                 `json:"header"`
	ID       string                                 `json:"id"`
	IsOther  OptionalNonNull[bool]                  `json:"isOther,omitzero"`
	IsSecret OptionalNonNull[bool]                  `json:"isSecret,omitzero"`
	Options  Optional[[]ToolRequestUserInputOption] `json:"options,omitzero"`
	Question string                                 `json:"question"`
}

var toolRequestUserInputQuestionSpec = objectSpec{
	fields: []objectField{
		{name: "header", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "isOther", flags: fieldNonNullOptional},
		{name: "isSecret", flags: fieldNonNullOptional},
		{name: "options"},
		{name: "question", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ToolRequestUserInputQuestion) MarshalJSON() ([]byte, error) {
	type wire ToolRequestUserInputQuestion
	return json.Marshal(wire(v))
}

func (v *ToolRequestUserInputQuestion) UnmarshalJSON(data []byte) error {
	type wire ToolRequestUserInputQuestion
	return decodeObject(data, &toolRequestUserInputQuestionSpec, (*wire)(v))
}

type ToolRequestUserInputResponse struct {
	Answers map[string]ToolRequestUserInputAnswer `json:"answers"`
}

var toolRequestUserInputResponseSpec = objectSpec{
	fields: []objectField{
		{name: "answers", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ToolRequestUserInputResponse) MarshalJSON() ([]byte, error) {
	type wire ToolRequestUserInputResponse
	return json.Marshal(wire(v))
}

func (v *ToolRequestUserInputResponse) UnmarshalJSON(data []byte) error {
	type wire ToolRequestUserInputResponse
	return decodeObject(data, &toolRequestUserInputResponseSpec, (*wire)(v))
}

type W3cTraceContext struct {
	Traceparent Optional[string] `json:"traceparent,omitzero"`
	Tracestate  Optional[string] `json:"tracestate,omitzero"`
}

var w3cTraceContextSpec = objectSpec{
	fields: []objectField{
		{name: "traceparent"},
		{name: "tracestate"},
	},
}

func (v W3cTraceContext) MarshalJSON() ([]byte, error) {
	type wire W3cTraceContext
	return json.Marshal(wire(v))
}

func (v *W3cTraceContext) UnmarshalJSON(data []byte) error {
	type wire W3cTraceContext
	return decodeObject(data, &w3cTraceContextSpec, (*wire)(v))
}

type AbsolutePathBuf string

type Account struct {
	Email                       Optional[string]          `json:"email,omitzero"`
	PlanType                    OptionalNonNull[PlanType] `json:"planType,omitzero"`
	TypeValue                   string                    `json:"type"`
	UsesCodexManagedCredentials OptionalNonNull[bool]     `json:"usesCodexManagedCredentials,omitzero"`
	WorkspaceName               Optional[string]          `json:"workspaceName,omitzero"`
	RawJSON                     json.RawMessage           `json:"-"`
}

var accountSpec = objectSpec{
	fields: []objectField{
		{name: "email"},
		{name: "planType", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "usesCodexManagedCredentials", flags: fieldNonNullOptional},
		{name: "workspaceName"},
	},
	tag:  2,
	tags: []string{"apiKey", "chatgpt", "amazonBedrock", "gcp"},
}

func (v Account) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "apiKey":
	case "chatgpt":
		if !v.Email.IsSet() {
			return nil, DecodeError{Field: "email", Reason: "missing required field for type chatgpt"}
		}
		if !v.PlanType.IsSet() {
			return nil, DecodeError{Field: "planType", Reason: "missing required field for type chatgpt"}
		}
	case "amazonBedrock":
	case "gcp":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire Account
	return json.Marshal(wire(v))
}

func (v *Account) UnmarshalJSON(data []byte) error {
	type wire Account
	var s objectScan
	known, err := s.begin(data, &accountSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "apiKey":
	case "chatgpt":
		if !s.has("email") {
			return DecodeError{Field: "email", Reason: "missing required field for type chatgpt"}
		}
		if !s.has("planType") {
			return DecodeError{Field: "planType", Reason: "missing required field for type chatgpt"}
		}
		if s.isNull("planType") {
			return DecodeError{Field: "planType", Reason: "cannot be null"}
		}
	case "amazonBedrock":
	case "gcp":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type AccountLoginCompletedNotification struct {
	Error                Optional[string]                      `json:"error,omitzero"`
	LoginID              Optional[string]                      `json:"loginId,omitzero"`
	OnboardingEntrypoint Optional[DesktopOnboardingEntrypoint] `json:"onboardingEntrypoint,omitzero"`
	Success              bool                                  `json:"success"`
}

var accountLoginCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "error"},
		{name: "loginId"},
		{name: "onboardingEntrypoint"},
		{name: "success", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AccountLoginCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire AccountLoginCompletedNotification
	return json.Marshal(wire(v))
}

func (v *AccountLoginCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire AccountLoginCompletedNotification
	return decodeObject(data, &accountLoginCompletedNotificationSpec, (*wire)(v))
}

type AccountRateLimitsUpdatedNotification struct {
	RateLimits RateLimitSnapshot `json:"rateLimits"`
}

var accountRateLimitsUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "rateLimits", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AccountRateLimitsUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire AccountRateLimitsUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *AccountRateLimitsUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire AccountRateLimitsUpdatedNotification
	return decodeObject(data, &accountRateLimitsUpdatedNotificationSpec, (*wire)(v))
}

type AccountRoutingOverride string

const (
	AccountRoutingOverrideNoConstraint AccountRoutingOverride = "NO_CONSTRAINT"
	AccountRoutingOverrideUs           AccountRoutingOverride = "us"
	AccountRoutingOverrideUsCr         AccountRoutingOverride = "us_cr"
)

type AccountSession struct {
	DisplayName                Optional[string]          `json:"displayName,omitzero"`
	Email                      Optional[string]          `json:"email,omitzero"`
	ImageURL                   Optional[string]          `json:"imageUrl,omitzero"`
	IsActive                   bool                      `json:"isActive"`
	LastUsedAt                 int64                     `json:"lastUsedAt"`
	SelectedWorkspaceAccountID Optional[string]          `json:"selectedWorkspaceAccountId,omitzero"`
	SessionID                  string                    `json:"sessionId"`
	UserID                     Optional[string]          `json:"userId,omitzero"`
	Workspaces                 []AccountSessionWorkspace `json:"workspaces"`
}

var accountSessionSpec = objectSpec{
	fields: []objectField{
		{name: "displayName"},
		{name: "email"},
		{name: "imageUrl"},
		{name: "isActive", flags: fieldRequired | fieldRejectNull},
		{name: "lastUsedAt", flags: fieldRequired | fieldRejectNull},
		{name: "selectedWorkspaceAccountId"},
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
		{name: "userId"},
		{name: "workspaces", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AccountSession) MarshalJSON() ([]byte, error) {
	type wire AccountSession
	return json.Marshal(wire(v))
}

func (v *AccountSession) UnmarshalJSON(data []byte) error {
	type wire AccountSession
	return decodeObject(data, &accountSessionSpec, (*wire)(v))
}

type AccountSessionWorkspace struct {
	AccountID string                                `json:"accountId"`
	ImageURL  Optional[string]                      `json:"imageUrl,omitzero"`
	Kind      Optional[AccountSessionWorkspaceKind] `json:"kind,omitzero"`
	Name      Optional[string]                      `json:"name,omitzero"`
}

var accountSessionWorkspaceSpec = objectSpec{
	fields: []objectField{
		{name: "accountId", flags: fieldRequired | fieldRejectNull},
		{name: "imageUrl"},
		{name: "kind"},
		{name: "name"},
	},
}

func (v AccountSessionWorkspace) MarshalJSON() ([]byte, error) {
	type wire AccountSessionWorkspace
	return json.Marshal(wire(v))
}

func (v *AccountSessionWorkspace) UnmarshalJSON(data []byte) error {
	type wire AccountSessionWorkspace
	return decodeObject(data, &accountSessionWorkspaceSpec, (*wire)(v))
}

type AccountSessionWorkspaceKind string

const (
	AccountSessionWorkspaceKindPersonal  AccountSessionWorkspaceKind = "personal"
	AccountSessionWorkspaceKindWorkspace AccountSessionWorkspaceKind = "workspace"
)

type AccountSessionsListParams struct {
	RefreshWorkspaceMetadata OptionalNonNull[bool] `json:"refreshWorkspaceMetadata,omitzero"`
}

var accountSessionsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "refreshWorkspaceMetadata", flags: fieldNonNullOptional},
	},
}

func (v AccountSessionsListParams) MarshalJSON() ([]byte, error) {
	type wire AccountSessionsListParams
	return json.Marshal(wire(v))
}

func (v *AccountSessionsListParams) UnmarshalJSON(data []byte) error {
	type wire AccountSessionsListParams
	return decodeObject(data, &accountSessionsListParamsSpec, (*wire)(v))
}

type AccountSessionsResponse struct {
	ActiveSessionID Optional[string] `json:"activeSessionId,omitzero"`
	Sessions        []AccountSession `json:"sessions"`
}

var accountSessionsResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activeSessionId"},
		{name: "sessions", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AccountSessionsResponse) MarshalJSON() ([]byte, error) {
	type wire AccountSessionsResponse
	return json.Marshal(wire(v))
}

func (v *AccountSessionsResponse) UnmarshalJSON(data []byte) error {
	type wire AccountSessionsResponse
	return decodeObject(data, &accountSessionsResponseSpec, (*wire)(v))
}

type AccountSessionsSwitchParams struct {
	AccountID         string                `json:"accountId"`
	AllowBrowserLogin OptionalNonNull[bool] `json:"allowBrowserLogin,omitzero"`
	SessionID         string                `json:"sessionId"`
}

var accountSessionsSwitchParamsSpec = objectSpec{
	fields: []objectField{
		{name: "accountId", flags: fieldRequired | fieldRejectNull},
		{name: "allowBrowserLogin", flags: fieldNonNullOptional},
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AccountSessionsSwitchParams) MarshalJSON() ([]byte, error) {
	type wire AccountSessionsSwitchParams
	return json.Marshal(wire(v))
}

func (v *AccountSessionsSwitchParams) UnmarshalJSON(data []byte) error {
	type wire AccountSessionsSwitchParams
	return decodeObject(data, &accountSessionsSwitchParamsSpec, (*wire)(v))
}

type AccountSessionsSwitchResponse = json.RawMessage

type AccountTokenUsageDailyBucket struct {
	StartDate string `json:"startDate"`
	Tokens    int64  `json:"tokens"`
}

var accountTokenUsageDailyBucketSpec = objectSpec{
	fields: []objectField{
		{name: "startDate", flags: fieldRequired | fieldRejectNull},
		{name: "tokens", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AccountTokenUsageDailyBucket) MarshalJSON() ([]byte, error) {
	type wire AccountTokenUsageDailyBucket
	return json.Marshal(wire(v))
}

func (v *AccountTokenUsageDailyBucket) UnmarshalJSON(data []byte) error {
	type wire AccountTokenUsageDailyBucket
	return decodeObject(data, &accountTokenUsageDailyBucketSpec, (*wire)(v))
}

type AccountTokenUsageSummary struct {
	CurrentStreakDays     Optional[int64] `json:"currentStreakDays,omitzero"`
	LifetimeTokens        Optional[int64] `json:"lifetimeTokens,omitzero"`
	LongestRunningTurnSec Optional[int64] `json:"longestRunningTurnSec,omitzero"`
	LongestStreakDays     Optional[int64] `json:"longestStreakDays,omitzero"`
	PeakDailyTokens       Optional[int64] `json:"peakDailyTokens,omitzero"`
}

var accountTokenUsageSummarySpec = objectSpec{
	fields: []objectField{
		{name: "currentStreakDays"},
		{name: "lifetimeTokens"},
		{name: "longestRunningTurnSec"},
		{name: "longestStreakDays"},
		{name: "peakDailyTokens"},
	},
}

func (v AccountTokenUsageSummary) MarshalJSON() ([]byte, error) {
	type wire AccountTokenUsageSummary
	return json.Marshal(wire(v))
}

func (v *AccountTokenUsageSummary) UnmarshalJSON(data []byte) error {
	type wire AccountTokenUsageSummary
	return decodeObject(data, &accountTokenUsageSummarySpec, (*wire)(v))
}

type AccountUpdatedNotification struct {
	AuthMode Optional[AuthMode] `json:"authMode,omitzero"`
	PlanType Optional[PlanType] `json:"planType,omitzero"`
}

var accountUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "authMode"},
		{name: "planType"},
	},
}

func (v AccountUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire AccountUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *AccountUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire AccountUpdatedNotification
	return decodeObject(data, &accountUpdatedNotificationSpec, (*wire)(v))
}

type ActivePermissionProfile struct {
	Extends Optional[string] `json:"extends,omitzero"`
	ID      string           `json:"id"`
}

var activePermissionProfileSpec = objectSpec{
	fields: []objectField{
		{name: "extends", defaultJSON: "null"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ActivePermissionProfile) MarshalJSON() ([]byte, error) {
	type wire ActivePermissionProfile
	return json.Marshal(wire(v))
}

func (v *ActivePermissionProfile) UnmarshalJSON(data []byte) error {
	type wire ActivePermissionProfile
	return decodeObject(data, &activePermissionProfileSpec, (*wire)(v))
}

type AddCreditsNudgeCreditType string

const (
	AddCreditsNudgeCreditTypeCredits    AddCreditsNudgeCreditType = "credits"
	AddCreditsNudgeCreditTypeUsageLimit AddCreditsNudgeCreditType = "usage_limit"
)

type AddCreditsNudgeEmailStatus string

const (
	AddCreditsNudgeEmailStatusSent           AddCreditsNudgeEmailStatus = "sent"
	AddCreditsNudgeEmailStatusCooldownActive AddCreditsNudgeEmailStatus = "cooldown_active"
)

type AdditionalContextEntry struct {
	Kind  AdditionalContextKind `json:"kind"`
	Name  Optional[string]      `json:"name,omitzero"`
	Value string                `json:"value"`
}

var additionalContextEntrySpec = objectSpec{
	fields: []objectField{
		{name: "kind", flags: fieldRequired | fieldRejectNull},
		{name: "name"},
		{name: "value", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AdditionalContextEntry) MarshalJSON() ([]byte, error) {
	type wire AdditionalContextEntry
	return json.Marshal(wire(v))
}

func (v *AdditionalContextEntry) UnmarshalJSON(data []byte) error {
	type wire AdditionalContextEntry
	return decodeObject(data, &additionalContextEntrySpec, (*wire)(v))
}

type AdditionalContextKind string

const (
	AdditionalContextKindUntrusted   AdditionalContextKind = "untrusted"
	AdditionalContextKindApplication AdditionalContextKind = "application"
)

type AdditionalFileSystemPermissions struct {
	Entries          Optional[[]FileSystemSandboxEntry] `json:"entries,omitzero"`
	GlobScanMaxDepth Optional[uint64]                   `json:"globScanMaxDepth,omitzero"`
	Read             Optional[[]LegacyAppPathString]    `json:"read,omitzero"`
	Write            Optional[[]LegacyAppPathString]    `json:"write,omitzero"`
}

var additionalFileSystemPermissionsSpec = objectSpec{
	fields: []objectField{
		{name: "entries"},
		{name: "globScanMaxDepth"},
		{name: "read"},
		{name: "write"},
	},
}

func (v AdditionalFileSystemPermissions) MarshalJSON() ([]byte, error) {
	type wire AdditionalFileSystemPermissions
	w := wire(v)
	w.Entries = w.Entries.withoutNull()
	w.GlobScanMaxDepth = w.GlobScanMaxDepth.withoutNull()
	return json.Marshal(w)
}

func (v *AdditionalFileSystemPermissions) UnmarshalJSON(data []byte) error {
	type wire AdditionalFileSystemPermissions
	var s objectScan
	if _, err := s.begin(data, &additionalFileSystemPermissionsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueGlobScanMaxDepth, ok := v.GlobScanMaxDepth.Value(); ok {
		if valueGlobScanMaxDepth < 1 {
			return DecodeError{Field: "globScanMaxDepth", Reason: "below minimum 1"}
		}
	}
	return nil
}

type AdditionalNetworkPermissions struct {
	Enabled Optional[bool] `json:"enabled,omitzero"`
}

var additionalNetworkPermissionsSpec = objectSpec{
	fields: []objectField{
		{name: "enabled"},
	},
}

func (v AdditionalNetworkPermissions) MarshalJSON() ([]byte, error) {
	type wire AdditionalNetworkPermissions
	return json.Marshal(wire(v))
}

func (v *AdditionalNetworkPermissions) UnmarshalJSON(data []byte) error {
	type wire AdditionalNetworkPermissions
	return decodeObject(data, &additionalNetworkPermissionsSpec, (*wire)(v))
}

type AgentMessageDelivery string

const (
	AgentMessageDeliveryAsync AgentMessageDelivery = "async"
)

type AgentMessageDeltaNotification struct {
	Delta    string `json:"delta"`
	ItemID   string `json:"itemId"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var agentMessageDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AgentMessageDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire AgentMessageDeltaNotification
	return json.Marshal(wire(v))
}

func (v *AgentMessageDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire AgentMessageDeltaNotification
	return decodeObject(data, &agentMessageDeltaNotificationSpec, (*wire)(v))
}

type AgentMessageInputContent struct {
	EncryptedContent OptionalNonNull[string] `json:"encrypted_content,omitzero"`
	Text             OptionalNonNull[string] `json:"text,omitzero"`
	TypeValue        string                  `json:"type"`
	RawJSON          json.RawMessage         `json:"-"`
}

var agentMessageInputContentSpec = objectSpec{
	fields: []objectField{
		{name: "encrypted_content", flags: fieldNonNullOptional},
		{name: "text", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  2,
	tags: []string{"input_text", "encrypted_content"},
}

func (v AgentMessageInputContent) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "input_text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type input_text"}
		}
	case "encrypted_content":
		if !v.EncryptedContent.IsSet() {
			return nil, DecodeError{Field: "encrypted_content", Reason: "missing required field for type encrypted_content"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire AgentMessageInputContent
	return json.Marshal(wire(v))
}

func (v *AgentMessageInputContent) UnmarshalJSON(data []byte) error {
	type wire AgentMessageInputContent
	var s objectScan
	known, err := s.begin(data, &agentMessageInputContentSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "input_text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type input_text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "encrypted_content":
		if !s.has("encrypted_content") {
			return DecodeError{Field: "encrypted_content", Reason: "missing required field for type encrypted_content"}
		}
		if s.isNull("encrypted_content") {
			return DecodeError{Field: "encrypted_content", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type AgentPath string

type AllowDenyRequirement string

const (
	AllowDenyRequirementAllow AllowDenyRequirement = "allow"
	AllowDenyRequirementDeny  AllowDenyRequirement = "deny"
)

type AnalyticsConfig struct {
	Enabled    Optional[bool]             `json:"enabled,omitzero"`
	Additional map[string]json.RawMessage `json:"-"`
}

var analyticsConfigSpec = objectSpec{
	fields: []objectField{
		{name: "enabled"},
	},
}

func (v AnalyticsConfig) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	for key, value := range v.Additional {
		out[key] = value
	}
	if v.Enabled.IsSet() {
		out["enabled"] = v.Enabled
	}
	return json.Marshal(out)
}

func (v *AnalyticsConfig) UnmarshalJSON(data []byte) error {
	type wire AnalyticsConfig
	var s objectScan
	if _, err := s.begin(data, &analyticsConfigSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.Additional = s.unknownMembers()
	if len(v.Additional) == 0 {
		if err := json.Unmarshal([]byte("{}"), &v.Additional); err != nil {
			return fmt.Errorf("field *: %w", err)
		}
	}
	return nil
}

type AppBranding struct {
	Category          Optional[string] `json:"category,omitzero"`
	Developer         Optional[string] `json:"developer,omitzero"`
	IsDiscoverableApp bool             `json:"isDiscoverableApp"`
	PrivacyPolicy     Optional[string] `json:"privacyPolicy,omitzero"`
	TermsOfService    Optional[string] `json:"termsOfService,omitzero"`
	Website           Optional[string] `json:"website,omitzero"`
}

var appBrandingSpec = objectSpec{
	fields: []objectField{
		{name: "category"},
		{name: "developer"},
		{name: "isDiscoverableApp", flags: fieldRequired | fieldRejectNull},
		{name: "privacyPolicy"},
		{name: "termsOfService"},
		{name: "website"},
	},
}

func (v AppBranding) MarshalJSON() ([]byte, error) {
	type wire AppBranding
	return json.Marshal(wire(v))
}

func (v *AppBranding) UnmarshalJSON(data []byte) error {
	type wire AppBranding
	return decodeObject(data, &appBrandingSpec, (*wire)(v))
}

type AppConfig struct {
	ApprovalsReviewer        Optional[ApprovalsReviewer]     `json:"approvals_reviewer,omitzero"`
	DefaultToolsApprovalMode Optional[AppToolApproval]       `json:"default_tools_approval_mode,omitzero"`
	DefaultToolsEnabled      Optional[bool]                  `json:"default_tools_enabled,omitzero"`
	DestructiveEnabled       Optional[bool]                  `json:"destructive_enabled,omitzero"`
	Enabled                  OptionalNonNull[bool]           `json:"enabled,omitzero"`
	Links                    Optional[AppLinksConfig]        `json:"links,omitzero"`
	OmitToolsFrom            Optional[[]ToolExposureSurface] `json:"omit_tools_from,omitzero"`
	OpenWorldEnabled         Optional[bool]                  `json:"open_world_enabled,omitzero"`
	Tools                    Optional[AppToolsConfig]        `json:"tools,omitzero"`
}

var appConfigSpec = objectSpec{
	fields: []objectField{
		{name: "approvals_reviewer"},
		{name: "default_tools_approval_mode"},
		{name: "default_tools_enabled"},
		{name: "destructive_enabled"},
		{name: "enabled", flags: fieldNonNullOptional},
		{name: "links"},
		{name: "omit_tools_from"},
		{name: "open_world_enabled"},
		{name: "tools"},
	},
}

func (v AppConfig) MarshalJSON() ([]byte, error) {
	type wire AppConfig
	return json.Marshal(wire(v))
}

func (v *AppConfig) UnmarshalJSON(data []byte) error {
	type wire AppConfig
	return decodeObject(data, &appConfigSpec, (*wire)(v))
}

type AppInfo struct {
	AppMetadata         Optional[AppMetadata]       `json:"appMetadata,omitzero"`
	Branding            Optional[AppBranding]       `json:"branding,omitzero"`
	Description         Optional[string]            `json:"description,omitzero"`
	DistributionChannel Optional[string]            `json:"distributionChannel,omitzero"`
	IconAssets          Optional[map[string]string] `json:"iconAssets,omitzero"`
	IconDarkAssets      Optional[map[string]string] `json:"iconDarkAssets,omitzero"`
	ID                  string                      `json:"id"`
	InstallURL          Optional[string]            `json:"installUrl,omitzero"`
	IsAccessible        OptionalNonNull[bool]       `json:"isAccessible,omitzero"`
	IsEnabled           OptionalNonNull[bool]       `json:"isEnabled,omitzero"`
	Labels              Optional[map[string]string] `json:"labels,omitzero"`
	LogoURL             Optional[string]            `json:"logoUrl,omitzero"`
	LogoURLDark         Optional[string]            `json:"logoUrlDark,omitzero"`
	Name                string                      `json:"name"`
	PluginDisplayNames  OptionalNonNull[[]string]   `json:"pluginDisplayNames,omitzero"`
}

var appInfoSpec = objectSpec{
	fields: []objectField{
		{name: "appMetadata"},
		{name: "branding"},
		{name: "description"},
		{name: "distributionChannel"},
		{name: "iconAssets"},
		{name: "iconDarkAssets"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "installUrl"},
		{name: "isAccessible", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "isEnabled", flags: fieldNonNullOptional, defaultJSON: "true"},
		{name: "labels"},
		{name: "logoUrl"},
		{name: "logoUrlDark"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "pluginDisplayNames", flags: fieldNonNullOptional, defaultJSON: "[]"},
	},
}

func (v AppInfo) MarshalJSON() ([]byte, error) {
	type wire AppInfo
	return json.Marshal(wire(v))
}

func (v *AppInfo) UnmarshalJSON(data []byte) error {
	type wire AppInfo
	return decodeObject(data, &appInfoSpec, (*wire)(v))
}

type AppLinkConfig struct {
	ApprovalsReviewer        Optional[ApprovalsReviewer] `json:"approvals_reviewer,omitzero"`
	DefaultToolsApprovalMode Optional[AppToolApproval]   `json:"default_tools_approval_mode,omitzero"`
}

var appLinkConfigSpec = objectSpec{
	fields: []objectField{
		{name: "approvals_reviewer"},
		{name: "default_tools_approval_mode"},
	},
}

func (v AppLinkConfig) MarshalJSON() ([]byte, error) {
	type wire AppLinkConfig
	return json.Marshal(wire(v))
}

func (v *AppLinkConfig) UnmarshalJSON(data []byte) error {
	type wire AppLinkConfig
	return decodeObject(data, &appLinkConfigSpec, (*wire)(v))
}

type AppLinksConfig map[string]json.RawMessage

func (v AppLinksConfig) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *AppLinksConfig) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = AppLinksConfig(raw)
	return nil
}

type AppListUpdatedNotification struct {
	Data []AppInfo `json:"data"`
}

var appListUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AppListUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire AppListUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *AppListUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire AppListUpdatedNotification
	return decodeObject(data, &appListUpdatedNotificationSpec, (*wire)(v))
}

type AppMetadata struct {
	Categories                 Optional[[]string]        `json:"categories,omitzero"`
	Developer                  Optional[string]          `json:"developer,omitzero"`
	FirstPartyRequiresInstall  Optional[bool]            `json:"firstPartyRequiresInstall,omitzero"`
	Review                     Optional[AppReview]       `json:"review,omitzero"`
	Screenshots                Optional[[]AppScreenshot] `json:"screenshots,omitzero"`
	SeoDescription             Optional[string]          `json:"seoDescription,omitzero"`
	ShowInComposerWhenUnlinked Optional[bool]            `json:"showInComposerWhenUnlinked,omitzero"`
	SubCategories              Optional[[]string]        `json:"subCategories,omitzero"`
	Version                    Optional[string]          `json:"version,omitzero"`
	VersionID                  Optional[string]          `json:"versionId,omitzero"`
	VersionNotes               Optional[string]          `json:"versionNotes,omitzero"`
}

var appMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "categories"},
		{name: "developer"},
		{name: "firstPartyRequiresInstall"},
		{name: "review"},
		{name: "screenshots"},
		{name: "seoDescription"},
		{name: "showInComposerWhenUnlinked"},
		{name: "subCategories"},
		{name: "version"},
		{name: "versionId"},
		{name: "versionNotes"},
	},
}

func (v AppMetadata) MarshalJSON() ([]byte, error) {
	type wire AppMetadata
	return json.Marshal(wire(v))
}

func (v *AppMetadata) UnmarshalJSON(data []byte) error {
	type wire AppMetadata
	return decodeObject(data, &appMetadataSpec, (*wire)(v))
}

type AppReview struct {
	Status string `json:"status"`
}

var appReviewSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AppReview) MarshalJSON() ([]byte, error) {
	type wire AppReview
	return json.Marshal(wire(v))
}

func (v *AppReview) UnmarshalJSON(data []byte) error {
	type wire AppReview
	return decodeObject(data, &appReviewSpec, (*wire)(v))
}

type AppScreenshot struct {
	FileID     Optional[string] `json:"fileId,omitzero"`
	URL        Optional[string] `json:"url,omitzero"`
	UserPrompt string           `json:"userPrompt"`
}

var appScreenshotSpec = objectSpec{
	fields: []objectField{
		{name: "fileId", aliases: []string{"file_id"}},
		{name: "url"},
		{name: "userPrompt", flags: fieldRequired | fieldRejectNull, aliases: []string{"user_prompt"}},
	},
	hasAliases: true,
}

func (v AppScreenshot) MarshalJSON() ([]byte, error) {
	type wire AppScreenshot
	return json.Marshal(wire(v))
}

func (v *AppScreenshot) UnmarshalJSON(data []byte) error {
	type wire AppScreenshot
	return decodeObject(data, &appScreenshotSpec, (*wire)(v))
}

type AppSummary struct {
	Category    Optional[string] `json:"category,omitzero"`
	Description Optional[string] `json:"description,omitzero"`
	ID          string           `json:"id"`
	InstallURL  Optional[string] `json:"installUrl,omitzero"`
	Name        string           `json:"name"`
}

var appSummarySpec = objectSpec{
	fields: []objectField{
		{name: "category"},
		{name: "description"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "installUrl"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AppSummary) MarshalJSON() ([]byte, error) {
	type wire AppSummary
	return json.Marshal(wire(v))
}

func (v *AppSummary) UnmarshalJSON(data []byte) error {
	type wire AppSummary
	return decodeObject(data, &appSummarySpec, (*wire)(v))
}

type AppTemplateSummary struct {
	CanonicalConnectorID Optional[string]                       `json:"canonicalConnectorId,omitzero"`
	Category             Optional[string]                       `json:"category,omitzero"`
	Description          Optional[string]                       `json:"description,omitzero"`
	LogoURL              Optional[string]                       `json:"logoUrl,omitzero"`
	LogoURLDark          Optional[string]                       `json:"logoUrlDark,omitzero"`
	MaterializedAppIDs   []string                               `json:"materializedAppIds"`
	Name                 string                                 `json:"name"`
	Reason               Optional[AppTemplateUnavailableReason] `json:"reason,omitzero"`
	TemplateID           string                                 `json:"templateId"`
}

var appTemplateSummarySpec = objectSpec{
	fields: []objectField{
		{name: "canonicalConnectorId"},
		{name: "category"},
		{name: "description"},
		{name: "logoUrl"},
		{name: "logoUrlDark"},
		{name: "materializedAppIds", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "reason"},
		{name: "templateId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AppTemplateSummary) MarshalJSON() ([]byte, error) {
	type wire AppTemplateSummary
	return json.Marshal(wire(v))
}

func (v *AppTemplateSummary) UnmarshalJSON(data []byte) error {
	type wire AppTemplateSummary
	return decodeObject(data, &appTemplateSummarySpec, (*wire)(v))
}

type AppTemplateUnavailableReason string

const (
	AppTemplateUnavailableReasonNotConfiguredForWorkspace AppTemplateUnavailableReason = "NOT_CONFIGURED_FOR_WORKSPACE"
	AppTemplateUnavailableReasonNoActiveWorkspace         AppTemplateUnavailableReason = "NO_ACTIVE_WORKSPACE"
)

type AppToolApproval string

const (
	AppToolApprovalAuto    AppToolApproval = "auto"
	AppToolApprovalPrompt  AppToolApproval = "prompt"
	AppToolApprovalWrites  AppToolApproval = "writes"
	AppToolApprovalApprove AppToolApproval = "approve"
)

type AppToolConfig struct {
	ApprovalMode Optional[AppToolApproval] `json:"approval_mode,omitzero"`
	Enabled      Optional[bool]            `json:"enabled,omitzero"`
}

var appToolConfigSpec = objectSpec{
	fields: []objectField{
		{name: "approval_mode"},
		{name: "enabled"},
	},
}

func (v AppToolConfig) MarshalJSON() ([]byte, error) {
	type wire AppToolConfig
	return json.Marshal(wire(v))
}

func (v *AppToolConfig) UnmarshalJSON(data []byte) error {
	type wire AppToolConfig
	return decodeObject(data, &appToolConfigSpec, (*wire)(v))
}

type AppToolSummary struct {
	Description    string                `json:"description"`
	DisabledReason Optional[string]      `json:"disabledReason,omitzero"`
	IsEnabled      OptionalNonNull[bool] `json:"isEnabled,omitzero"`
	IsReadOnly     OptionalNonNull[bool] `json:"isReadOnly,omitzero"`
	Name           string                `json:"name"`
	Title          Optional[string]      `json:"title,omitzero"`
}

var appToolSummarySpec = objectSpec{
	fields: []objectField{
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "disabledReason"},
		{name: "isEnabled", flags: fieldNonNullOptional},
		{name: "isReadOnly", flags: fieldNonNullOptional},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
	},
}

func (v AppToolSummary) MarshalJSON() ([]byte, error) {
	type wire AppToolSummary
	return json.Marshal(wire(v))
}

func (v *AppToolSummary) UnmarshalJSON(data []byte) error {
	type wire AppToolSummary
	return decodeObject(data, &appToolSummarySpec, (*wire)(v))
}

type AppToolsConfig map[string]json.RawMessage

func (v AppToolsConfig) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *AppToolsConfig) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = AppToolsConfig(raw)
	return nil
}

type ApplicationNetworkRequirements struct {
	Domains map[string]NetworkDomainPermission `json:"domains"`
	Enabled bool                               `json:"enabled"`
}

var applicationNetworkRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "domains", flags: fieldRequired | fieldRejectNull},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ApplicationNetworkRequirements) MarshalJSON() ([]byte, error) {
	type wire ApplicationNetworkRequirements
	return json.Marshal(wire(v))
}

func (v *ApplicationNetworkRequirements) UnmarshalJSON(data []byte) error {
	type wire ApplicationNetworkRequirements
	return decodeObject(data, &applicationNetworkRequirementsSpec, (*wire)(v))
}

type ApplicationRequirements struct {
	Network Optional[ApplicationNetworkRequirements] `json:"network,omitzero"`
	Storage Optional[ApplicationStorageRequirements] `json:"storage,omitzero"`
}

var applicationRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "network"},
		{name: "storage"},
	},
}

func (v ApplicationRequirements) MarshalJSON() ([]byte, error) {
	type wire ApplicationRequirements
	return json.Marshal(wire(v))
}

func (v *ApplicationRequirements) UnmarshalJSON(data []byte) error {
	type wire ApplicationRequirements
	return decodeObject(data, &applicationRequirementsSpec, (*wire)(v))
}

type ApplicationStorageRequirements struct {
	DefaultProjectParent     Optional[LegacyAppPathString] `json:"defaultProjectParent,omitzero"`
	ProjectlessWorkspaceRoot Optional[LegacyAppPathString] `json:"projectlessWorkspaceRoot,omitzero"`
}

var applicationStorageRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "defaultProjectParent"},
		{name: "projectlessWorkspaceRoot"},
	},
}

func (v ApplicationStorageRequirements) MarshalJSON() ([]byte, error) {
	type wire ApplicationStorageRequirements
	return json.Marshal(wire(v))
}

func (v *ApplicationStorageRequirements) UnmarshalJSON(data []byte) error {
	type wire ApplicationStorageRequirements
	return decodeObject(data, &applicationStorageRequirementsSpec, (*wire)(v))
}

type ApprovalsReviewer string

const (
	ApprovalsReviewerUser       ApprovalsReviewer = "user"
	ApprovalsReviewerAutoReview ApprovalsReviewer = "auto_review"
)

func (v *ApprovalsReviewer) UnmarshalJSON(data []byte) error {
	var value string
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	switch value {
	case "guardian_subagent":
		*v = ApprovalsReviewerAutoReview
	default:
		*v = ApprovalsReviewer(value)
	}
	return nil
}

func (v ApprovalsReviewer) MarshalJSON() ([]byte, error) {
	switch string(v) {
	case "guardian_subagent":
		return json.Marshal("auto_review")
	default:
		return json.Marshal(string(v))
	}
}

type AppsConfig struct {
	DefaultValue Optional[AppsDefaultConfig] `json:"_default,omitzero"`
	Apps         map[string]json.RawMessage  `json:"-"`
}

var appsConfigSpec = objectSpec{
	fields: []objectField{
		{name: "_default", defaultJSON: "null"},
	},
}

func (v AppsConfig) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	for key, value := range v.Apps {
		out[key] = value
	}
	if v.DefaultValue.IsSet() {
		out["_default"] = v.DefaultValue
	}
	return json.Marshal(out)
}

func (v *AppsConfig) UnmarshalJSON(data []byte) error {
	type wire AppsConfig
	var s objectScan
	if _, err := s.begin(data, &appsConfigSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.Apps = s.unknownMembers()
	if len(v.Apps) == 0 {
		if err := json.Unmarshal([]byte("{}"), &v.Apps); err != nil {
			return fmt.Errorf("field *: %w", err)
		}
	}
	return nil
}

type AppsDefaultConfig struct {
	ApprovalsReviewer        Optional[ApprovalsReviewer] `json:"approvals_reviewer,omitzero"`
	DefaultToolsApprovalMode Optional[AppToolApproval]   `json:"default_tools_approval_mode,omitzero"`
	DestructiveEnabled       OptionalNonNull[bool]       `json:"destructive_enabled,omitzero"`
	Enabled                  OptionalNonNull[bool]       `json:"enabled,omitzero"`
	OpenWorldEnabled         OptionalNonNull[bool]       `json:"open_world_enabled,omitzero"`
}

var appsDefaultConfigSpec = objectSpec{
	fields: []objectField{
		{name: "approvals_reviewer"},
		{name: "default_tools_approval_mode"},
		{name: "destructive_enabled", flags: fieldNonNullOptional},
		{name: "enabled", flags: fieldNonNullOptional, defaultJSON: "true"},
		{name: "open_world_enabled", flags: fieldNonNullOptional},
	},
}

func (v AppsDefaultConfig) MarshalJSON() ([]byte, error) {
	type wire AppsDefaultConfig
	return json.Marshal(wire(v))
}

func (v *AppsDefaultConfig) UnmarshalJSON(data []byte) error {
	type wire AppsDefaultConfig
	return decodeObject(data, &appsDefaultConfigSpec, (*wire)(v))
}

type AppsInstalledParams struct {
	ForceRefresh OptionalNonNull[bool] `json:"forceRefresh,omitzero"`
	ThreadID     Optional[string]      `json:"threadId,omitzero"`
}

var appsInstalledParamsSpec = objectSpec{
	fields: []objectField{
		{name: "forceRefresh", flags: fieldNonNullOptional},
		{name: "threadId"},
	},
}

func (v AppsInstalledParams) MarshalJSON() ([]byte, error) {
	type wire AppsInstalledParams
	return json.Marshal(wire(v))
}

func (v *AppsInstalledParams) UnmarshalJSON(data []byte) error {
	type wire AppsInstalledParams
	return decodeObject(data, &appsInstalledParamsSpec, (*wire)(v))
}

type AppsInstalledResponse struct {
	Apps []InstalledApp `json:"apps"`
}

var appsInstalledResponseSpec = objectSpec{
	fields: []objectField{
		{name: "apps", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AppsInstalledResponse) MarshalJSON() ([]byte, error) {
	type wire AppsInstalledResponse
	return json.Marshal(wire(v))
}

func (v *AppsInstalledResponse) UnmarshalJSON(data []byte) error {
	type wire AppsInstalledResponse
	return decodeObject(data, &appsInstalledResponseSpec, (*wire)(v))
}

type AppsListParams struct {
	Cursor       Optional[string]      `json:"cursor,omitzero"`
	ForceRefetch OptionalNonNull[bool] `json:"forceRefetch,omitzero"`
	Limit        Optional[uint32]      `json:"limit,omitzero"`
	ThreadID     Optional[string]      `json:"threadId,omitzero"`
}

var appsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "forceRefetch", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "limit"},
		{name: "threadId"},
	},
}

func (v AppsListParams) MarshalJSON() ([]byte, error) {
	type wire AppsListParams
	w := wire(v)
	w.ForceRefetch = nonNullWithoutFalse(w.ForceRefetch)
	return json.Marshal(w)
}

func (v *AppsListParams) UnmarshalJSON(data []byte) error {
	type wire AppsListParams
	var s objectScan
	if _, err := s.begin(data, &appsListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type AppsListResponse struct {
	Data       []AppInfo        `json:"data"`
	NextCursor Optional[string] `json:"nextCursor,omitzero"`
}

var appsListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v AppsListResponse) MarshalJSON() ([]byte, error) {
	type wire AppsListResponse
	return json.Marshal(wire(v))
}

func (v *AppsListResponse) UnmarshalJSON(data []byte) error {
	type wire AppsListResponse
	return decodeObject(data, &appsListResponseSpec, (*wire)(v))
}

type AppsReadParams struct {
	AppIDs       []string              `json:"appIds"`
	IncludeTools OptionalNonNull[bool] `json:"includeTools,omitzero"`
	ThreadID     Optional[string]      `json:"threadId,omitzero"`
}

var appsReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "appIds", flags: fieldRequired | fieldRejectNull},
		{name: "includeTools", flags: fieldNonNullOptional},
		{name: "threadId"},
	},
}

func (v AppsReadParams) MarshalJSON() ([]byte, error) {
	type wire AppsReadParams
	return json.Marshal(wire(v))
}

func (v *AppsReadParams) UnmarshalJSON(data []byte) error {
	type wire AppsReadParams
	return decodeObject(data, &appsReadParamsSpec, (*wire)(v))
}

type AppsReadResponse struct {
	Apps          []ConnectorMetadata `json:"apps"`
	MissingAppIDs []string            `json:"missingAppIds"`
}

var appsReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "apps", flags: fieldRequired | fieldRejectNull},
		{name: "missingAppIds", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AppsReadResponse) MarshalJSON() ([]byte, error) {
	type wire AppsReadResponse
	return json.Marshal(wire(v))
}

func (v *AppsReadResponse) UnmarshalJSON(data []byte) error {
	type wire AppsReadResponse
	return decodeObject(data, &appsReadResponseSpec, (*wire)(v))
}

type AskForApproval struct {
	StringValue string                                      `json:"-"`
	Granular    OptionalNonNull[map[string]json.RawMessage] `json:"granular,omitzero"`
	RawJSON     json.RawMessage                             `json:"-"`
}

var (
	AskForApprovalUntrusted = AskForApproval{StringValue: "untrusted"}
	AskForApprovalOnRequest = AskForApproval{StringValue: "on-request"}
	AskForApprovalNever     = AskForApproval{StringValue: "never"}
)

func (v AskForApproval) IsSet() bool {
	return v.StringValue != "" || len(bytes.TrimSpace(v.RawJSON)) > 0 || v.Granular.IsSet()
}

var askForApprovalSpec = objectSpec{
	fields: []objectField{
		{name: "granular", flags: fieldNonNullOptional},
	},
	fieldOffset: 1,
}

func (v AskForApproval) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.Granular.IsSet() {
		out["granular"] = v.Granular
	}
	if v.StringValue != "" {
		if len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 {
			return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
		}
		switch v.StringValue {
		case "untrusted", "on-request", "never":
			return json.Marshal(v.StringValue)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported AskForApproval string value %q", v.StringValue)}
		}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	if _, ok := out["granular"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid AskForApproval raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *AskForApproval) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var stringValue string
	if err := json.Unmarshal(trimmed, &stringValue); err == nil {
		switch stringValue {
		case "untrusted", "on-request", "never":
			*v = AskForApproval{StringValue: stringValue}
		default:
			*v = AskForApproval{RawJSON: append(json.RawMessage(nil), data...)}
		}
		return nil
	}
	v.StringValue = ""
	type wire AskForApproval
	var s objectScan
	if _, err := s.begin(data, &askForApprovalSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.nonNull("granular") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type AsyncUserInputQuestion struct {
	Options Optional[[]string] `json:"options,omitzero"`
	Title   string             `json:"title"`
}

var asyncUserInputQuestionSpec = objectSpec{
	fields: []objectField{
		{name: "options"},
		{name: "title", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AsyncUserInputQuestion) MarshalJSON() ([]byte, error) {
	type wire AsyncUserInputQuestion
	return json.Marshal(wire(v))
}

func (v *AsyncUserInputQuestion) UnmarshalJSON(data []byte) error {
	type wire AsyncUserInputQuestion
	return decodeObject(data, &asyncUserInputQuestionSpec, (*wire)(v))
}

type AuthMode string

const (
	AuthModeApikey              AuthMode = "apikey"
	AuthModeChatgpt             AuthMode = "chatgpt"
	AuthModeChatgptAuthTokens   AuthMode = "chatgptAuthTokens"
	AuthModeHeaders             AuthMode = "headers"
	AuthModeAgentIdentity       AuthMode = "agentIdentity"
	AuthModePersonalAccessToken AuthMode = "personalAccessToken"
	AuthModeBedrockApiKey       AuthMode = "bedrockApiKey"
	AuthModeBedrockAccessKeys   AuthMode = "bedrockAccessKeys"
)

type AuthRecoveryNotification struct {
	Message  string `json:"message"`
	Provider string `json:"provider"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var authRecoveryNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "provider", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v AuthRecoveryNotification) MarshalJSON() ([]byte, error) {
	type wire AuthRecoveryNotification
	return json.Marshal(wire(v))
}

func (v *AuthRecoveryNotification) UnmarshalJSON(data []byte) error {
	type wire AuthRecoveryNotification
	return decodeObject(data, &authRecoveryNotificationSpec, (*wire)(v))
}

type AutoCompactTokenLimitScope string

const (
	AutoCompactTokenLimitScopeTotal           AutoCompactTokenLimitScope = "total"
	AutoCompactTokenLimitScopeBodyAfterPrefix AutoCompactTokenLimitScope = "body_after_prefix"
)

type AutoReviewDecisionSource string

const (
	AutoReviewDecisionSourceAgent AutoReviewDecisionSource = "agent"
)

type AutoReviewRequirements struct {
	IgnoreRules      Optional[[]string] `json:"ignoreRules,omitzero"`
	RequiredOnModels Optional[[]string] `json:"requiredOnModels,omitzero"`
}

var autoReviewRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "ignoreRules"},
		{name: "requiredOnModels"},
	},
}

func (v AutoReviewRequirements) MarshalJSON() ([]byte, error) {
	type wire AutoReviewRequirements
	return json.Marshal(wire(v))
}

func (v *AutoReviewRequirements) UnmarshalJSON(data []byte) error {
	type wire AutoReviewRequirements
	return decodeObject(data, &autoReviewRequirementsSpec, (*wire)(v))
}

type AwsCredentialType string

const (
	AwsCredentialTypeAccessKeys    AwsCredentialType = "accessKeys"
	AwsCredentialTypeBedrockApiKey AwsCredentialType = "bedrockApiKey"
)

type BedrockAwsProfile struct {
	Name   string           `json:"name"`
	Region Optional[string] `json:"region,omitzero"`
}

var bedrockAwsProfileSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "region"},
	},
}

func (v BedrockAwsProfile) MarshalJSON() ([]byte, error) {
	type wire BedrockAwsProfile
	return json.Marshal(wire(v))
}

func (v *BedrockAwsProfile) UnmarshalJSON(data []byte) error {
	type wire BedrockAwsProfile
	return decodeObject(data, &bedrockAwsProfileSpec, (*wire)(v))
}

type BedrockCheckGovCloudRequirementsParams map[string]json.RawMessage

func (v BedrockCheckGovCloudRequirementsParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *BedrockCheckGovCloudRequirementsParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = BedrockCheckGovCloudRequirementsParams(raw)
	return nil
}

type BedrockCheckGovCloudRequirementsResponse struct {
	IsGovCloud bool `json:"isGovCloud"`
	ShouldWarn bool `json:"shouldWarn"`
}

var bedrockCheckGovCloudRequirementsResponseSpec = objectSpec{
	fields: []objectField{
		{name: "isGovCloud", flags: fieldRequired | fieldRejectNull},
		{name: "shouldWarn", flags: fieldRequired | fieldRejectNull},
	},
}

func (v BedrockCheckGovCloudRequirementsResponse) MarshalJSON() ([]byte, error) {
	type wire BedrockCheckGovCloudRequirementsResponse
	return json.Marshal(wire(v))
}

func (v *BedrockCheckGovCloudRequirementsResponse) UnmarshalJSON(data []byte) error {
	type wire BedrockCheckGovCloudRequirementsResponse
	return decodeObject(data, &bedrockCheckGovCloudRequirementsResponseSpec, (*wire)(v))
}

type BedrockDiscoverParams map[string]json.RawMessage

func (v BedrockDiscoverParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *BedrockDiscoverParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = BedrockDiscoverParams(raw)
	return nil
}

type BedrockDiscoverResponse struct {
	EnvironmentCredentials []BedrockEnvironmentCredential `json:"environmentCredentials"`
	Profiles               []BedrockAwsProfile            `json:"profiles"`
}

var bedrockDiscoverResponseSpec = objectSpec{
	fields: []objectField{
		{name: "environmentCredentials", flags: fieldRequired | fieldRejectNull},
		{name: "profiles", flags: fieldRequired | fieldRejectNull},
	},
}

func (v BedrockDiscoverResponse) MarshalJSON() ([]byte, error) {
	type wire BedrockDiscoverResponse
	return json.Marshal(wire(v))
}

func (v *BedrockDiscoverResponse) UnmarshalJSON(data []byte) error {
	type wire BedrockDiscoverResponse
	return decodeObject(data, &bedrockDiscoverResponseSpec, (*wire)(v))
}

type BedrockEnvironmentCredential struct {
	Region    Optional[string]  `json:"region,omitzero"`
	TypeValue AwsCredentialType `json:"type"`
}

var bedrockEnvironmentCredentialSpec = objectSpec{
	fields: []objectField{
		{name: "region"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
}

func (v BedrockEnvironmentCredential) MarshalJSON() ([]byte, error) {
	type wire BedrockEnvironmentCredential
	return json.Marshal(wire(v))
}

func (v *BedrockEnvironmentCredential) UnmarshalJSON(data []byte) error {
	type wire BedrockEnvironmentCredential
	return decodeObject(data, &bedrockEnvironmentCredentialSpec, (*wire)(v))
}

type BedrockSetupParams struct {
	Profile   OptionalNonNull[string] `json:"profile,omitzero"`
	Region    OptionalNonNull[string] `json:"region,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var bedrockSetupParamsSpec = objectSpec{
	fields: []objectField{
		{name: "profile", flags: fieldNonNullOptional},
		{name: "region", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  2,
	tags: []string{"profile", "environment"},
}

func (v BedrockSetupParams) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "profile":
		if !v.Profile.IsSet() {
			return nil, DecodeError{Field: "profile", Reason: "missing required field for type profile"}
		}
		if !v.Region.IsSet() {
			return nil, DecodeError{Field: "region", Reason: "missing required field for type profile"}
		}
	case "environment":
		if !v.Region.IsSet() {
			return nil, DecodeError{Field: "region", Reason: "missing required field for type environment"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire BedrockSetupParams
	return json.Marshal(wire(v))
}

func (v *BedrockSetupParams) UnmarshalJSON(data []byte) error {
	type wire BedrockSetupParams
	var s objectScan
	known, err := s.begin(data, &bedrockSetupParamsSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "profile":
		if !s.has("profile") {
			return DecodeError{Field: "profile", Reason: "missing required field for type profile"}
		}
		if s.isNull("profile") {
			return DecodeError{Field: "profile", Reason: "cannot be null"}
		}
		if !s.has("region") {
			return DecodeError{Field: "region", Reason: "missing required field for type profile"}
		}
		if s.isNull("region") {
			return DecodeError{Field: "region", Reason: "cannot be null"}
		}
	case "environment":
		if !s.has("region") {
			return DecodeError{Field: "region", Reason: "missing required field for type environment"}
		}
		if s.isNull("region") {
			return DecodeError{Field: "region", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type BedrockSetupResponse map[string]json.RawMessage

func (v BedrockSetupResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *BedrockSetupResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = BedrockSetupResponse(raw)
	return nil
}

type BrowserUseAccessApprovalLifetime string

const (
	BrowserUseAccessApprovalLifetimeTurn   BrowserUseAccessApprovalLifetime = "turn"
	BrowserUseAccessApprovalLifetimeThread BrowserUseAccessApprovalLifetime = "thread"
)

type BrowserUseConfig struct {
	AllowHistoryAccess  Optional[bool]                                    `json:"allow_history_access,omitzero"`
	DefaultOriginPolicy Optional[BrowserUseOriginPolicyConfig]            `json:"default_origin_policy,omitzero"`
	Origins             Optional[map[string]BrowserUseOriginPolicyConfig] `json:"origins,omitzero"`
}

var browserUseConfigSpec = objectSpec{
	fields: []objectField{
		{name: "allow_history_access"},
		{name: "default_origin_policy"},
		{name: "origins"},
	},
}

func (v BrowserUseConfig) MarshalJSON() ([]byte, error) {
	type wire BrowserUseConfig
	return json.Marshal(wire(v))
}

func (v *BrowserUseConfig) UnmarshalJSON(data []byte) error {
	type wire BrowserUseConfig
	return decodeObject(data, &browserUseConfigSpec, (*wire)(v))
}

type BrowserUseExtensionRequirements struct {
	RequestHeaders Optional[[]RequestHeader] `json:"requestHeaders,omitzero"`
}

var browserUseExtensionRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "requestHeaders"},
	},
}

func (v BrowserUseExtensionRequirements) MarshalJSON() ([]byte, error) {
	type wire BrowserUseExtensionRequirements
	return json.Marshal(wire(v))
}

func (v *BrowserUseExtensionRequirements) UnmarshalJSON(data []byte) error {
	type wire BrowserUseExtensionRequirements
	return decodeObject(data, &browserUseExtensionRequirementsSpec, (*wire)(v))
}

type BrowserUseOriginPolicy struct {
	Access                 Optional[AllowDenyRequirement]             `json:"access,omitzero"`
	AccessApprovalLifetime Optional[BrowserUseAccessApprovalLifetime] `json:"accessApprovalLifetime,omitzero"`
	AutoReview             Optional[AllowDenyRequirement]             `json:"autoReview,omitzero"`
	Downloads              Optional[AllowDenyRequirement]             `json:"downloads,omitzero"`
	FullCdpAccess          Optional[AllowDenyRequirement]             `json:"fullCdpAccess,omitzero"`
	PersistentApproval     Optional[bool]                             `json:"persistentApproval,omitzero"`
	Uploads                Optional[AllowDenyRequirement]             `json:"uploads,omitzero"`
}

var browserUseOriginPolicySpec = objectSpec{
	fields: []objectField{
		{name: "access"},
		{name: "accessApprovalLifetime"},
		{name: "autoReview"},
		{name: "downloads"},
		{name: "fullCdpAccess"},
		{name: "persistentApproval"},
		{name: "uploads"},
	},
}

func (v BrowserUseOriginPolicy) MarshalJSON() ([]byte, error) {
	type wire BrowserUseOriginPolicy
	return json.Marshal(wire(v))
}

func (v *BrowserUseOriginPolicy) UnmarshalJSON(data []byte) error {
	type wire BrowserUseOriginPolicy
	return decodeObject(data, &browserUseOriginPolicySpec, (*wire)(v))
}

type BrowserUseOriginPolicyConfig struct {
	Access        Optional[AllowDenyRequirement] `json:"access,omitzero"`
	Downloads     Optional[AllowDenyRequirement] `json:"downloads,omitzero"`
	FullCdpAccess Optional[AllowDenyRequirement] `json:"full_cdp_access,omitzero"`
	Uploads       Optional[AllowDenyRequirement] `json:"uploads,omitzero"`
}

var browserUseOriginPolicyConfigSpec = objectSpec{
	fields: []objectField{
		{name: "access"},
		{name: "downloads"},
		{name: "full_cdp_access"},
		{name: "uploads"},
	},
}

func (v BrowserUseOriginPolicyConfig) MarshalJSON() ([]byte, error) {
	type wire BrowserUseOriginPolicyConfig
	return json.Marshal(wire(v))
}

func (v *BrowserUseOriginPolicyConfig) UnmarshalJSON(data []byte) error {
	type wire BrowserUseOriginPolicyConfig
	return decodeObject(data, &browserUseOriginPolicyConfigSpec, (*wire)(v))
}

type BrowserUseRequirements struct {
	AllowGlobalPersistentApproval Optional[bool]                              `json:"allowGlobalPersistentApproval,omitzero"`
	AllowHistoryAccess            Optional[bool]                              `json:"allowHistoryAccess,omitzero"`
	AllowWebmcp                   Optional[bool]                              `json:"allowWebmcp,omitzero"`
	DefaultOriginPolicy           Optional[BrowserUseOriginPolicy]            `json:"defaultOriginPolicy,omitzero"`
	DisableAutoReview             Optional[bool]                              `json:"disableAutoReview,omitzero"`
	Extension                     Optional[BrowserUseExtensionRequirements]   `json:"extension,omitzero"`
	Origins                       Optional[map[string]BrowserUseOriginPolicy] `json:"origins,omitzero"`
}

var browserUseRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "allowGlobalPersistentApproval"},
		{name: "allowHistoryAccess"},
		{name: "allowWebmcp"},
		{name: "defaultOriginPolicy"},
		{name: "disableAutoReview"},
		{name: "extension"},
		{name: "origins"},
	},
}

func (v BrowserUseRequirements) MarshalJSON() ([]byte, error) {
	type wire BrowserUseRequirements
	return json.Marshal(wire(v))
}

func (v *BrowserUseRequirements) UnmarshalJSON(data []byte) error {
	type wire BrowserUseRequirements
	return decodeObject(data, &browserUseRequirementsSpec, (*wire)(v))
}

type ByteRange struct {
	End   uint64 `json:"end"`
	Start uint64 `json:"start"`
}

var byteRangeSpec = objectSpec{
	fields: []objectField{
		{name: "end", flags: fieldRequired | fieldRejectNull},
		{name: "start", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ByteRange) MarshalJSON() ([]byte, error) {
	type wire ByteRange
	return json.Marshal(wire(v))
}

func (v *ByteRange) UnmarshalJSON(data []byte) error {
	type wire ByteRange
	var s objectScan
	if _, err := s.begin(data, &byteRangeSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.End < 0 {
		return DecodeError{Field: "end", Reason: "below minimum 0"}
	}
	if v.Start < 0 {
		return DecodeError{Field: "start", Reason: "below minimum 0"}
	}
	return nil
}

type CancelLoginAccountParams struct {
	LoginID string `json:"loginId"`
}

var cancelLoginAccountParamsSpec = objectSpec{
	fields: []objectField{
		{name: "loginId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CancelLoginAccountParams) MarshalJSON() ([]byte, error) {
	type wire CancelLoginAccountParams
	return json.Marshal(wire(v))
}

func (v *CancelLoginAccountParams) UnmarshalJSON(data []byte) error {
	type wire CancelLoginAccountParams
	return decodeObject(data, &cancelLoginAccountParamsSpec, (*wire)(v))
}

type CancelLoginAccountResponse struct {
	Status CancelLoginAccountStatus `json:"status"`
}

var cancelLoginAccountResponseSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CancelLoginAccountResponse) MarshalJSON() ([]byte, error) {
	type wire CancelLoginAccountResponse
	return json.Marshal(wire(v))
}

func (v *CancelLoginAccountResponse) UnmarshalJSON(data []byte) error {
	type wire CancelLoginAccountResponse
	return decodeObject(data, &cancelLoginAccountResponseSpec, (*wire)(v))
}

type CancelLoginAccountStatus string

const (
	CancelLoginAccountStatusCanceled CancelLoginAccountStatus = "canceled"
	CancelLoginAccountStatusNotFound CancelLoginAccountStatus = "notFound"
)

type CapabilityRootLocation struct {
	EnvironmentID OptionalNonNull[string] `json:"environmentId,omitzero"`
	Path          OptionalNonNull[string] `json:"path,omitzero"`
	TypeValue     string                  `json:"type"`
	RawJSON       json.RawMessage         `json:"-"`
}

var capabilityRootLocationSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId", flags: fieldNonNullOptional},
		{name: "path", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  2,
	tags: []string{"environment"},
}

func (v CapabilityRootLocation) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "environment":
		if !v.EnvironmentID.IsSet() {
			return nil, DecodeError{Field: "environmentId", Reason: "missing required field for type environment"}
		}
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type environment"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire CapabilityRootLocation
	return json.Marshal(wire(v))
}

func (v *CapabilityRootLocation) UnmarshalJSON(data []byte) error {
	type wire CapabilityRootLocation
	var s objectScan
	known, err := s.begin(data, &capabilityRootLocationSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "environment":
		if !s.has("environmentId") {
			return DecodeError{Field: "environmentId", Reason: "missing required field for type environment"}
		}
		if s.isNull("environmentId") {
			return DecodeError{Field: "environmentId", Reason: "cannot be null"}
		}
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type environment"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type CliAuthCredentialsStoreMode string

const (
	CliAuthCredentialsStoreModeFile      CliAuthCredentialsStoreMode = "file"
	CliAuthCredentialsStoreModeKeyring   CliAuthCredentialsStoreMode = "keyring"
	CliAuthCredentialsStoreModeAuto      CliAuthCredentialsStoreMode = "auto"
	CliAuthCredentialsStoreModeEphemeral CliAuthCredentialsStoreMode = "ephemeral"
)

type CodexErrorInfo struct {
	StringValue                    string                                      `json:"-"`
	ActiveTurnNotSteerable         OptionalNonNull[map[string]json.RawMessage] `json:"activeTurnNotSteerable,omitzero"`
	HttpConnectionFailed           OptionalNonNull[map[string]json.RawMessage] `json:"httpConnectionFailed,omitzero"`
	ResponseStreamConnectionFailed OptionalNonNull[map[string]json.RawMessage] `json:"responseStreamConnectionFailed,omitzero"`
	ResponseStreamDisconnected     OptionalNonNull[map[string]json.RawMessage] `json:"responseStreamDisconnected,omitzero"`
	ResponseTooManyFailedAttempts  OptionalNonNull[map[string]json.RawMessage] `json:"responseTooManyFailedAttempts,omitzero"`
	RawJSON                        json.RawMessage                             `json:"-"`
}

var (
	CodexErrorInfoContextWindowExceeded       = CodexErrorInfo{StringValue: "contextWindowExceeded"}
	CodexErrorInfoSessionBudgetExceeded       = CodexErrorInfo{StringValue: "sessionBudgetExceeded"}
	CodexErrorInfoUsageLimitExceeded          = CodexErrorInfo{StringValue: "usageLimitExceeded"}
	CodexErrorInfoRateLimitExceeded           = CodexErrorInfo{StringValue: "rateLimitExceeded"}
	CodexErrorInfoFlexUnavailable             = CodexErrorInfo{StringValue: "flexUnavailable"}
	CodexErrorInfoServerOverloaded            = CodexErrorInfo{StringValue: "serverOverloaded"}
	CodexErrorInfoModelNotFound               = CodexErrorInfo{StringValue: "modelNotFound"}
	CodexErrorInfoPolicyCapacityUnavailable   = CodexErrorInfo{StringValue: "policyCapacityUnavailable"}
	CodexErrorInfoCyberPolicy                 = CodexErrorInfo{StringValue: "cyberPolicy"}
	CodexErrorInfoMisalignmentPolicyViolation = CodexErrorInfo{StringValue: "misalignmentPolicyViolation"}
	CodexErrorInfoTooManyDenials              = CodexErrorInfo{StringValue: "tooManyDenials"}
	CodexErrorInfoInternalServerError         = CodexErrorInfo{StringValue: "internalServerError"}
	CodexErrorInfoUnauthorized                = CodexErrorInfo{StringValue: "unauthorized"}
	CodexErrorInfoBadRequest                  = CodexErrorInfo{StringValue: "badRequest"}
	CodexErrorInfoThreadRollbackFailed        = CodexErrorInfo{StringValue: "threadRollbackFailed"}
	CodexErrorInfoSandboxError                = CodexErrorInfo{StringValue: "sandboxError"}
	CodexErrorInfoOther                       = CodexErrorInfo{StringValue: "other"}
)

func (v CodexErrorInfo) IsSet() bool {
	return v.StringValue != "" || len(bytes.TrimSpace(v.RawJSON)) > 0 || v.ActiveTurnNotSteerable.IsSet() || v.HttpConnectionFailed.IsSet() || v.ResponseStreamConnectionFailed.IsSet() || v.ResponseStreamDisconnected.IsSet() || v.ResponseTooManyFailedAttempts.IsSet()
}

var codexErrorInfoSpec = objectSpec{
	fields: []objectField{
		{name: "activeTurnNotSteerable", flags: fieldNonNullOptional},
		{name: "httpConnectionFailed", flags: fieldNonNullOptional},
		{name: "responseStreamConnectionFailed", flags: fieldNonNullOptional},
		{name: "responseStreamDisconnected", flags: fieldNonNullOptional},
		{name: "responseTooManyFailedAttempts", flags: fieldNonNullOptional},
	},
	fieldOffset: 1,
}

func (v CodexErrorInfo) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.ActiveTurnNotSteerable.IsSet() {
		out["activeTurnNotSteerable"] = v.ActiveTurnNotSteerable
	}
	if v.HttpConnectionFailed.IsSet() {
		out["httpConnectionFailed"] = v.HttpConnectionFailed
	}
	if v.ResponseStreamConnectionFailed.IsSet() {
		out["responseStreamConnectionFailed"] = v.ResponseStreamConnectionFailed
	}
	if v.ResponseStreamDisconnected.IsSet() {
		out["responseStreamDisconnected"] = v.ResponseStreamDisconnected
	}
	if v.ResponseTooManyFailedAttempts.IsSet() {
		out["responseTooManyFailedAttempts"] = v.ResponseTooManyFailedAttempts
	}
	if v.StringValue != "" {
		if len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 {
			return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
		}
		switch v.StringValue {
		case "contextWindowExceeded", "sessionBudgetExceeded", "usageLimitExceeded", "rateLimitExceeded", "flexUnavailable", "serverOverloaded", "modelNotFound", "policyCapacityUnavailable", "cyberPolicy", "misalignmentPolicyViolation", "tooManyDenials", "internalServerError", "unauthorized", "badRequest", "threadRollbackFailed", "sandboxError", "other":
			return json.Marshal(v.StringValue)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported CodexErrorInfo string value %q", v.StringValue)}
		}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	if _, ok := out["httpConnectionFailed"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	if _, ok := out["responseStreamConnectionFailed"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant2Matches := true
	if _, ok := out["responseStreamDisconnected"]; !ok {
		untaggedOneOfVariant2Matches = false
	}
	if untaggedOneOfVariant2Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant3Matches := true
	if _, ok := out["responseTooManyFailedAttempts"]; !ok {
		untaggedOneOfVariant3Matches = false
	}
	if untaggedOneOfVariant3Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant4Matches := true
	if _, ok := out["activeTurnNotSteerable"]; !ok {
		untaggedOneOfVariant4Matches = false
	}
	if untaggedOneOfVariant4Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid CodexErrorInfo raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *CodexErrorInfo) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var stringValue string
	if err := json.Unmarshal(trimmed, &stringValue); err == nil {
		switch stringValue {
		case "contextWindowExceeded", "sessionBudgetExceeded", "usageLimitExceeded", "rateLimitExceeded", "flexUnavailable", "serverOverloaded", "modelNotFound", "policyCapacityUnavailable", "cyberPolicy", "misalignmentPolicyViolation", "tooManyDenials", "internalServerError", "unauthorized", "badRequest", "threadRollbackFailed", "sandboxError", "other":
			*v = CodexErrorInfo{StringValue: stringValue}
		default:
			*v = CodexErrorInfo{RawJSON: append(json.RawMessage(nil), data...)}
		}
		return nil
	}
	v.StringValue = ""
	type wire CodexErrorInfo
	var s objectScan
	if _, err := s.begin(data, &codexErrorInfoSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.nonNull("httpConnectionFailed") {
		untaggedOneOfMatches++
	}
	if s.nonNull("responseStreamConnectionFailed") {
		untaggedOneOfMatches++
	}
	if s.nonNull("responseStreamDisconnected") {
		untaggedOneOfMatches++
	}
	if s.nonNull("responseTooManyFailedAttempts") {
		untaggedOneOfMatches++
	}
	if s.nonNull("activeTurnNotSteerable") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type CodexResponseHandoffMode string

const (
	CodexResponseHandoffModeThinking   CodexResponseHandoffMode = "thinking"
	CodexResponseHandoffModeCommentary CodexResponseHandoffMode = "commentary"
	CodexResponseHandoffModeBemTags    CodexResponseHandoffMode = "bemTags"
)

type CollabAgentState struct {
	Message Optional[string]  `json:"message,omitzero"`
	Status  CollabAgentStatus `json:"status"`
}

var collabAgentStateSpec = objectSpec{
	fields: []objectField{
		{name: "message"},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CollabAgentState) MarshalJSON() ([]byte, error) {
	type wire CollabAgentState
	return json.Marshal(wire(v))
}

func (v *CollabAgentState) UnmarshalJSON(data []byte) error {
	type wire CollabAgentState
	return decodeObject(data, &collabAgentStateSpec, (*wire)(v))
}

type CollabAgentStatus string

const (
	CollabAgentStatusPendingInit CollabAgentStatus = "pendingInit"
	CollabAgentStatusRunning     CollabAgentStatus = "running"
	CollabAgentStatusInterrupted CollabAgentStatus = "interrupted"
	CollabAgentStatusCompleted   CollabAgentStatus = "completed"
	CollabAgentStatusErrored     CollabAgentStatus = "errored"
	CollabAgentStatusShutdown    CollabAgentStatus = "shutdown"
	CollabAgentStatusNotFound    CollabAgentStatus = "notFound"
)

type CollabAgentTool string

const (
	CollabAgentToolSpawnAgent     CollabAgentTool = "spawnAgent"
	CollabAgentToolSendInput      CollabAgentTool = "sendInput"
	CollabAgentToolResumeAgent    CollabAgentTool = "resumeAgent"
	CollabAgentToolWait           CollabAgentTool = "wait"
	CollabAgentToolCloseAgent     CollabAgentTool = "closeAgent"
	CollabAgentToolSendMessage    CollabAgentTool = "sendMessage"
	CollabAgentToolFollowupTask   CollabAgentTool = "followupTask"
	CollabAgentToolInterruptAgent CollabAgentTool = "interruptAgent"
	CollabAgentToolListAgents     CollabAgentTool = "listAgents"
)

type CollabAgentToolCallStatus string

const (
	CollabAgentToolCallStatusInProgress  CollabAgentToolCallStatus = "inProgress"
	CollabAgentToolCallStatusCompleted   CollabAgentToolCallStatus = "completed"
	CollabAgentToolCallStatusFailed      CollabAgentToolCallStatus = "failed"
	CollabAgentToolCallStatusInterrupted CollabAgentToolCallStatus = "interrupted"
)

type CollaborationMode struct {
	Mode     ModeKind `json:"mode"`
	Settings Settings `json:"settings"`
}

var collaborationModeSpec = objectSpec{
	fields: []objectField{
		{name: "mode", flags: fieldRequired | fieldRejectNull},
		{name: "settings", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CollaborationMode) MarshalJSON() ([]byte, error) {
	type wire CollaborationMode
	return json.Marshal(wire(v))
}

func (v *CollaborationMode) UnmarshalJSON(data []byte) error {
	type wire CollaborationMode
	return decodeObject(data, &collaborationModeSpec, (*wire)(v))
}

type CollaborationModeListParams map[string]json.RawMessage

func (v CollaborationModeListParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *CollaborationModeListParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = CollaborationModeListParams(raw)
	return nil
}

type CollaborationModeListResponse struct {
	Data []CollaborationModeMask `json:"data"`
}

var collaborationModeListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CollaborationModeListResponse) MarshalJSON() ([]byte, error) {
	type wire CollaborationModeListResponse
	return json.Marshal(wire(v))
}

func (v *CollaborationModeListResponse) UnmarshalJSON(data []byte) error {
	type wire CollaborationModeListResponse
	return decodeObject(data, &collaborationModeListResponseSpec, (*wire)(v))
}

type CollaborationModeMask struct {
	Mode            Optional[ModeKind] `json:"mode,omitzero"`
	Model           Optional[string]   `json:"model,omitzero"`
	Name            string             `json:"name"`
	ReasoningEffort json.RawMessage    `json:"reasoning_effort,omitempty"`
}

var collaborationModeMaskSpec = objectSpec{
	fields: []objectField{
		{name: "mode"},
		{name: "model"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "reasoning_effort", flags: fieldRejectNull},
	},
}

func (v CollaborationModeMask) MarshalJSON() ([]byte, error) {
	if bytes.Equal(v.ReasoningEffort, []byte("null")) {
		return nil, DecodeError{Field: "reasoning_effort", Reason: "cannot be null"}
	}
	type wire CollaborationModeMask
	return json.Marshal(wire(v))
}

func (v *CollaborationModeMask) UnmarshalJSON(data []byte) error {
	type wire CollaborationModeMask
	return decodeObject(data, &collaborationModeMaskSpec, (*wire)(v))
}

type CommandAction struct {
	Command   OptionalNonNull[string]       `json:"command,omitzero"`
	Name      OptionalNonNull[string]       `json:"name,omitzero"`
	Path      Optional[LegacyAppPathString] `json:"path,omitzero"`
	Query     Optional[string]              `json:"query,omitzero"`
	TypeValue string                        `json:"type"`
	RawJSON   json.RawMessage               `json:"-"`
}

var commandActionSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldNonNullOptional},
		{name: "name", flags: fieldNonNullOptional},
		{name: "path"},
		{name: "query"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  4,
	tags: []string{"read", "listFiles", "search", "unknown"},
}

func (v CommandAction) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "read":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type read"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type read"}
		}
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type read"}
		}
		if v.Path.IsNull() {
			return nil, DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "listFiles":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type listFiles"}
		}
	case "search":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type search"}
		}
	case "unknown":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type unknown"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire CommandAction
	return json.Marshal(wire(v))
}

func (v *CommandAction) UnmarshalJSON(data []byte) error {
	type wire CommandAction
	var s objectScan
	known, err := s.begin(data, &commandActionSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "read":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type read"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type read"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type read"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "listFiles":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type listFiles"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
	case "search":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type search"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
	case "unknown":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type unknown"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type CommandExecOutputDeltaNotification struct {
	CapReached  bool                    `json:"capReached"`
	DeltaBase64 string                  `json:"deltaBase64"`
	ProcessID   string                  `json:"processId"`
	Stream      CommandExecOutputStream `json:"stream"`
}

var commandExecOutputDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "capReached", flags: fieldRequired | fieldRejectNull},
		{name: "deltaBase64", flags: fieldRequired | fieldRejectNull},
		{name: "processId", flags: fieldRequired | fieldRejectNull},
		{name: "stream", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecOutputDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire CommandExecOutputDeltaNotification
	return json.Marshal(wire(v))
}

func (v *CommandExecOutputDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire CommandExecOutputDeltaNotification
	return decodeObject(data, &commandExecOutputDeltaNotificationSpec, (*wire)(v))
}

type CommandExecOutputStream string

const (
	CommandExecOutputStreamStdout CommandExecOutputStream = "stdout"
	CommandExecOutputStreamStderr CommandExecOutputStream = "stderr"
)

type CommandExecParams struct {
	Command            []string                              `json:"command"`
	Cwd                Optional[string]                      `json:"cwd,omitzero"`
	DisableOutputCap   OptionalNonNull[bool]                 `json:"disableOutputCap,omitzero"`
	DisableTimeout     OptionalNonNull[bool]                 `json:"disableTimeout,omitzero"`
	Env                Optional[map[string]Optional[string]] `json:"env,omitzero"`
	OutputBytesCap     Optional[uint64]                      `json:"outputBytesCap,omitzero"`
	PermissionProfile  Optional[string]                      `json:"permissionProfile,omitzero"`
	ProcessID          Optional[string]                      `json:"processId,omitzero"`
	SandboxPolicy      Optional[SandboxPolicy]               `json:"sandboxPolicy,omitzero"`
	Size               Optional[CommandExecTerminalSize]     `json:"size,omitzero"`
	StreamStdin        OptionalNonNull[bool]                 `json:"streamStdin,omitzero"`
	StreamStdoutStderr OptionalNonNull[bool]                 `json:"streamStdoutStderr,omitzero"`
	TimeoutMs          Optional[int64]                       `json:"timeoutMs,omitzero"`
	Tty                OptionalNonNull[bool]                 `json:"tty,omitzero"`
}

var commandExecParamsSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldRequired | fieldRejectNull},
		{name: "cwd"},
		{name: "disableOutputCap", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "disableTimeout", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "env"},
		{name: "outputBytesCap"},
		{name: "permissionProfile"},
		{name: "processId"},
		{name: "sandboxPolicy"},
		{name: "size"},
		{name: "streamStdin", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "streamStdoutStderr", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "timeoutMs"},
		{name: "tty", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v CommandExecParams) MarshalJSON() ([]byte, error) {
	type wire CommandExecParams
	w := wire(v)
	w.DisableOutputCap = nonNullWithoutFalse(w.DisableOutputCap)
	w.DisableTimeout = nonNullWithoutFalse(w.DisableTimeout)
	w.StreamStdin = nonNullWithoutFalse(w.StreamStdin)
	w.StreamStdoutStderr = nonNullWithoutFalse(w.StreamStdoutStderr)
	w.Tty = nonNullWithoutFalse(w.Tty)
	return json.Marshal(w)
}

func (v *CommandExecParams) UnmarshalJSON(data []byte) error {
	type wire CommandExecParams
	var s objectScan
	if _, err := s.begin(data, &commandExecParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueOutputBytesCap, ok := v.OutputBytesCap.Value(); ok {
		if valueOutputBytesCap < 0 {
			return DecodeError{Field: "outputBytesCap", Reason: "below minimum 0"}
		}
	}
	return nil
}

type CommandExecResizeParams struct {
	ProcessID string                  `json:"processId"`
	Size      CommandExecTerminalSize `json:"size"`
}

var commandExecResizeParamsSpec = objectSpec{
	fields: []objectField{
		{name: "processId", flags: fieldRequired | fieldRejectNull},
		{name: "size", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecResizeParams) MarshalJSON() ([]byte, error) {
	type wire CommandExecResizeParams
	return json.Marshal(wire(v))
}

func (v *CommandExecResizeParams) UnmarshalJSON(data []byte) error {
	type wire CommandExecResizeParams
	return decodeObject(data, &commandExecResizeParamsSpec, (*wire)(v))
}

type CommandExecResizeResponse map[string]json.RawMessage

func (v CommandExecResizeResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *CommandExecResizeResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = CommandExecResizeResponse(raw)
	return nil
}

type CommandExecResponse struct {
	ExitCode int32  `json:"exitCode"`
	Stderr   string `json:"stderr"`
	Stdout   string `json:"stdout"`
}

var commandExecResponseSpec = objectSpec{
	fields: []objectField{
		{name: "exitCode", flags: fieldRequired | fieldRejectNull},
		{name: "stderr", flags: fieldRequired | fieldRejectNull},
		{name: "stdout", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecResponse) MarshalJSON() ([]byte, error) {
	type wire CommandExecResponse
	return json.Marshal(wire(v))
}

func (v *CommandExecResponse) UnmarshalJSON(data []byte) error {
	type wire CommandExecResponse
	return decodeObject(data, &commandExecResponseSpec, (*wire)(v))
}

type CommandExecTerminalSize struct {
	Cols uint16 `json:"cols"`
	Rows uint16 `json:"rows"`
}

var commandExecTerminalSizeSpec = objectSpec{
	fields: []objectField{
		{name: "cols", flags: fieldRequired | fieldRejectNull},
		{name: "rows", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecTerminalSize) MarshalJSON() ([]byte, error) {
	type wire CommandExecTerminalSize
	return json.Marshal(wire(v))
}

func (v *CommandExecTerminalSize) UnmarshalJSON(data []byte) error {
	type wire CommandExecTerminalSize
	var s objectScan
	if _, err := s.begin(data, &commandExecTerminalSizeSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Cols < 0 {
		return DecodeError{Field: "cols", Reason: "below minimum 0"}
	}
	if v.Rows < 0 {
		return DecodeError{Field: "rows", Reason: "below minimum 0"}
	}
	return nil
}

type CommandExecTerminateParams struct {
	ProcessID string `json:"processId"`
}

var commandExecTerminateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "processId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecTerminateParams) MarshalJSON() ([]byte, error) {
	type wire CommandExecTerminateParams
	return json.Marshal(wire(v))
}

func (v *CommandExecTerminateParams) UnmarshalJSON(data []byte) error {
	type wire CommandExecTerminateParams
	return decodeObject(data, &commandExecTerminateParamsSpec, (*wire)(v))
}

type CommandExecTerminateResponse map[string]json.RawMessage

func (v CommandExecTerminateResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *CommandExecTerminateResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = CommandExecTerminateResponse(raw)
	return nil
}

type CommandExecWriteParams struct {
	CloseStdin  OptionalNonNull[bool] `json:"closeStdin,omitzero"`
	DeltaBase64 Optional[string]      `json:"deltaBase64,omitzero"`
	ProcessID   string                `json:"processId"`
}

var commandExecWriteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "closeStdin", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "deltaBase64"},
		{name: "processId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecWriteParams) MarshalJSON() ([]byte, error) {
	type wire CommandExecWriteParams
	w := wire(v)
	w.CloseStdin = nonNullWithoutFalse(w.CloseStdin)
	return json.Marshal(w)
}

func (v *CommandExecWriteParams) UnmarshalJSON(data []byte) error {
	type wire CommandExecWriteParams
	return decodeObject(data, &commandExecWriteParamsSpec, (*wire)(v))
}

type CommandExecWriteResponse map[string]json.RawMessage

func (v CommandExecWriteResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *CommandExecWriteResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = CommandExecWriteResponse(raw)
	return nil
}

type CommandExecutionOutputDeltaNotification struct {
	Delta    string `json:"delta"`
	ItemID   string `json:"itemId"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var commandExecutionOutputDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandExecutionOutputDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire CommandExecutionOutputDeltaNotification
	return json.Marshal(wire(v))
}

func (v *CommandExecutionOutputDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire CommandExecutionOutputDeltaNotification
	return decodeObject(data, &commandExecutionOutputDeltaNotificationSpec, (*wire)(v))
}

type CommandExecutionSource string

const (
	CommandExecutionSourceAgent                  CommandExecutionSource = "agent"
	CommandExecutionSourceUserShell              CommandExecutionSource = "userShell"
	CommandExecutionSourceUnifiedExecStartup     CommandExecutionSource = "unifiedExecStartup"
	CommandExecutionSourceUnifiedExecInteraction CommandExecutionSource = "unifiedExecInteraction"
)

type CommandExecutionStatus string

const (
	CommandExecutionStatusInProgress CommandExecutionStatus = "inProgress"
	CommandExecutionStatusCompleted  CommandExecutionStatus = "completed"
	CommandExecutionStatusFailed     CommandExecutionStatus = "failed"
	CommandExecutionStatusDeclined   CommandExecutionStatus = "declined"
)

type CommandMigration struct {
	Name string `json:"name"`
}

var commandMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CommandMigration) MarshalJSON() ([]byte, error) {
	type wire CommandMigration
	return json.Marshal(wire(v))
}

func (v *CommandMigration) UnmarshalJSON(data []byte) error {
	type wire CommandMigration
	return decodeObject(data, &commandMigrationSpec, (*wire)(v))
}

type ComputerUseConfig struct {
	DefaultAppAccess Optional[AllowDenyRequirement]     `json:"default_app_access,omitzero"`
	Macos            Optional[ComputerUseMacosConfig]   `json:"macos,omitzero"`
	Windows          Optional[ComputerUseWindowsConfig] `json:"windows,omitzero"`
}

var computerUseConfigSpec = objectSpec{
	fields: []objectField{
		{name: "default_app_access"},
		{name: "macos"},
		{name: "windows"},
	},
}

func (v ComputerUseConfig) MarshalJSON() ([]byte, error) {
	type wire ComputerUseConfig
	return json.Marshal(wire(v))
}

func (v *ComputerUseConfig) UnmarshalJSON(data []byte) error {
	type wire ComputerUseConfig
	return decodeObject(data, &computerUseConfigSpec, (*wire)(v))
}

type ComputerUseMacosConfig struct {
	BundleIDs Optional[map[string]AllowDenyRequirement] `json:"bundle_ids,omitzero"`
}

var computerUseMacosConfigSpec = objectSpec{
	fields: []objectField{
		{name: "bundle_ids"},
	},
}

func (v ComputerUseMacosConfig) MarshalJSON() ([]byte, error) {
	type wire ComputerUseMacosConfig
	return json.Marshal(wire(v))
}

func (v *ComputerUseMacosConfig) UnmarshalJSON(data []byte) error {
	type wire ComputerUseMacosConfig
	return decodeObject(data, &computerUseMacosConfigSpec, (*wire)(v))
}

type ComputerUseMacosRequirements struct {
	BundleIDs Optional[map[string]AllowDenyRequirement] `json:"bundleIds,omitzero"`
}

var computerUseMacosRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "bundleIds"},
	},
}

func (v ComputerUseMacosRequirements) MarshalJSON() ([]byte, error) {
	type wire ComputerUseMacosRequirements
	return json.Marshal(wire(v))
}

func (v *ComputerUseMacosRequirements) UnmarshalJSON(data []byte) error {
	type wire ComputerUseMacosRequirements
	return decodeObject(data, &computerUseMacosRequirementsSpec, (*wire)(v))
}

type ComputerUseRequirements struct {
	AllowLockedComputerUse  Optional[bool]                           `json:"allowLockedComputerUse,omitzero"`
	AllowPersistentApproval Optional[bool]                           `json:"allowPersistentApproval,omitzero"`
	DefaultAppAccess        Optional[AllowDenyRequirement]           `json:"defaultAppAccess,omitzero"`
	Macos                   Optional[ComputerUseMacosRequirements]   `json:"macos,omitzero"`
	Windows                 Optional[ComputerUseWindowsRequirements] `json:"windows,omitzero"`
}

var computerUseRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "allowLockedComputerUse"},
		{name: "allowPersistentApproval"},
		{name: "defaultAppAccess"},
		{name: "macos"},
		{name: "windows"},
	},
}

func (v ComputerUseRequirements) MarshalJSON() ([]byte, error) {
	type wire ComputerUseRequirements
	return json.Marshal(wire(v))
}

func (v *ComputerUseRequirements) UnmarshalJSON(data []byte) error {
	type wire ComputerUseRequirements
	return decodeObject(data, &computerUseRequirementsSpec, (*wire)(v))
}

type ComputerUseWindowsConfig struct {
	Aumids Optional[map[string]AllowDenyRequirement] `json:"aumids,omitzero"`
	Exes   Optional[[]ComputerUseWindowsExeConfig]   `json:"exes,omitzero"`
}

var computerUseWindowsConfigSpec = objectSpec{
	fields: []objectField{
		{name: "aumids"},
		{name: "exes"},
	},
}

func (v ComputerUseWindowsConfig) MarshalJSON() ([]byte, error) {
	type wire ComputerUseWindowsConfig
	return json.Marshal(wire(v))
}

func (v *ComputerUseWindowsConfig) UnmarshalJSON(data []byte) error {
	type wire ComputerUseWindowsConfig
	return decodeObject(data, &computerUseWindowsConfigSpec, (*wire)(v))
}

type ComputerUseWindowsExeConfig struct {
	Access        AllowDenyRequirement `json:"access"`
	BinaryName    Optional[string]     `json:"binary_name,omitzero"`
	ProductName   string               `json:"product_name"`
	PublisherName string               `json:"publisher_name"`
}

var computerUseWindowsExeConfigSpec = objectSpec{
	fields: []objectField{
		{name: "access", flags: fieldRequired | fieldRejectNull},
		{name: "binary_name"},
		{name: "product_name", flags: fieldRequired | fieldRejectNull},
		{name: "publisher_name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ComputerUseWindowsExeConfig) MarshalJSON() ([]byte, error) {
	type wire ComputerUseWindowsExeConfig
	return json.Marshal(wire(v))
}

func (v *ComputerUseWindowsExeConfig) UnmarshalJSON(data []byte) error {
	type wire ComputerUseWindowsExeConfig
	return decodeObject(data, &computerUseWindowsExeConfigSpec, (*wire)(v))
}

type ComputerUseWindowsExeRequirement struct {
	Access        AllowDenyRequirement `json:"access"`
	BinaryName    Optional[string]     `json:"binaryName,omitzero"`
	ProductName   string               `json:"productName"`
	PublisherName string               `json:"publisherName"`
}

var computerUseWindowsExeRequirementSpec = objectSpec{
	fields: []objectField{
		{name: "access", flags: fieldRequired | fieldRejectNull},
		{name: "binaryName"},
		{name: "productName", flags: fieldRequired | fieldRejectNull},
		{name: "publisherName", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ComputerUseWindowsExeRequirement) MarshalJSON() ([]byte, error) {
	type wire ComputerUseWindowsExeRequirement
	return json.Marshal(wire(v))
}

func (v *ComputerUseWindowsExeRequirement) UnmarshalJSON(data []byte) error {
	type wire ComputerUseWindowsExeRequirement
	return decodeObject(data, &computerUseWindowsExeRequirementSpec, (*wire)(v))
}

type ComputerUseWindowsRequirements struct {
	Aumids Optional[map[string]AllowDenyRequirement]    `json:"aumids,omitzero"`
	Exes   Optional[[]ComputerUseWindowsExeRequirement] `json:"exes,omitzero"`
}

var computerUseWindowsRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "aumids"},
		{name: "exes"},
	},
}

func (v ComputerUseWindowsRequirements) MarshalJSON() ([]byte, error) {
	type wire ComputerUseWindowsRequirements
	return json.Marshal(wire(v))
}

func (v *ComputerUseWindowsRequirements) UnmarshalJSON(data []byte) error {
	type wire ComputerUseWindowsRequirements
	return decodeObject(data, &computerUseWindowsRequirementsSpec, (*wire)(v))
}

type Config struct {
	Analytics                       Optional[AnalyticsConfig]            `json:"analytics,omitzero"`
	ApprovalPolicy                  Optional[AskForApproval]             `json:"approval_policy,omitzero"`
	ApprovalsReviewer               Optional[ApprovalsReviewer]          `json:"approvals_reviewer,omitzero"`
	Apps                            Optional[AppsConfig]                 `json:"apps,omitzero"`
	BrowserUse                      Optional[BrowserUseConfig]           `json:"browser_use,omitzero"`
	CompactPrompt                   Optional[string]                     `json:"compact_prompt,omitzero"`
	ComputerUse                     Optional[ComputerUseConfig]          `json:"computer_use,omitzero"`
	Desktop                         Optional[map[string]json.RawMessage] `json:"desktop,omitzero"`
	DeveloperInstructions           Optional[string]                     `json:"developer_instructions,omitzero"`
	ForcedChatgptWorkspaceID        Optional[ForcedChatgptWorkspaceIDs]  `json:"forced_chatgpt_workspace_id,omitzero"`
	ForcedLoginMethod               Optional[ForcedLoginMethod]          `json:"forced_login_method,omitzero"`
	Instructions                    Optional[string]                     `json:"instructions,omitzero"`
	Model                           Optional[string]                     `json:"model,omitzero"`
	ModelAutoCompactTokenLimit      Optional[int64]                      `json:"model_auto_compact_token_limit,omitzero"`
	ModelAutoCompactTokenLimitScope Optional[AutoCompactTokenLimitScope] `json:"model_auto_compact_token_limit_scope,omitzero"`
	ModelContextWindow              Optional[int64]                      `json:"model_context_window,omitzero"`
	ModelProvider                   Optional[string]                     `json:"model_provider,omitzero"`
	ModelReasoningEffort            Optional[ReasoningEffort]            `json:"model_reasoning_effort,omitzero"`
	ModelReasoningSummary           Optional[ReasoningSummary]           `json:"model_reasoning_summary,omitzero"`
	ModelVerbosity                  Optional[Verbosity]                  `json:"model_verbosity,omitzero"`
	ReviewModel                     Optional[string]                     `json:"review_model,omitzero"`
	SandboxMode                     Optional[SandboxMode]                `json:"sandbox_mode,omitzero"`
	SandboxWorkspaceWrite           Optional[SandboxWorkspaceWrite]      `json:"sandbox_workspace_write,omitzero"`
	ServiceTier                     Optional[string]                     `json:"service_tier,omitzero"`
	Tools                           Optional[ToolsV2]                    `json:"tools,omitzero"`
	WebSearch                       Optional[WebSearchMode]              `json:"web_search,omitzero"`
	Additional                      map[string]json.RawMessage           `json:"-"`
}

var configSpec = objectSpec{
	fields: []objectField{
		{name: "analytics"},
		{name: "approval_policy"},
		{name: "approvals_reviewer"},
		{name: "apps", defaultJSON: "null"},
		{name: "browser_use"},
		{name: "compact_prompt"},
		{name: "computer_use"},
		{name: "desktop"},
		{name: "developer_instructions"},
		{name: "forced_chatgpt_workspace_id"},
		{name: "forced_login_method"},
		{name: "instructions"},
		{name: "model"},
		{name: "model_auto_compact_token_limit"},
		{name: "model_auto_compact_token_limit_scope"},
		{name: "model_context_window"},
		{name: "model_provider"},
		{name: "model_reasoning_effort"},
		{name: "model_reasoning_summary"},
		{name: "model_verbosity"},
		{name: "review_model"},
		{name: "sandbox_mode"},
		{name: "sandbox_workspace_write"},
		{name: "service_tier"},
		{name: "tools"},
		{name: "web_search"},
	},
}

func (v Config) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	for key, value := range v.Additional {
		out[key] = value
	}
	if v.Analytics.IsSet() {
		out["analytics"] = v.Analytics
	}
	if v.ApprovalPolicy.IsSet() {
		out["approval_policy"] = v.ApprovalPolicy
	}
	if v.ApprovalsReviewer.IsSet() {
		out["approvals_reviewer"] = v.ApprovalsReviewer
	}
	if v.Apps.IsSet() {
		out["apps"] = v.Apps
	}
	if v.BrowserUse.IsSet() {
		out["browser_use"] = v.BrowserUse
	}
	if v.CompactPrompt.IsSet() {
		out["compact_prompt"] = v.CompactPrompt
	}
	if v.ComputerUse.IsSet() {
		out["computer_use"] = v.ComputerUse
	}
	if v.Desktop.IsSet() {
		out["desktop"] = v.Desktop
	}
	if v.DeveloperInstructions.IsSet() {
		out["developer_instructions"] = v.DeveloperInstructions
	}
	if v.ForcedChatgptWorkspaceID.IsSet() {
		out["forced_chatgpt_workspace_id"] = v.ForcedChatgptWorkspaceID
	}
	if v.ForcedLoginMethod.IsSet() {
		out["forced_login_method"] = v.ForcedLoginMethod
	}
	if v.Instructions.IsSet() {
		out["instructions"] = v.Instructions
	}
	if v.Model.IsSet() {
		out["model"] = v.Model
	}
	if v.ModelAutoCompactTokenLimit.IsSet() {
		out["model_auto_compact_token_limit"] = v.ModelAutoCompactTokenLimit
	}
	if v.ModelAutoCompactTokenLimitScope.IsSet() {
		out["model_auto_compact_token_limit_scope"] = v.ModelAutoCompactTokenLimitScope
	}
	if v.ModelContextWindow.IsSet() {
		out["model_context_window"] = v.ModelContextWindow
	}
	if v.ModelProvider.IsSet() {
		out["model_provider"] = v.ModelProvider
	}
	if v.ModelReasoningEffort.IsSet() {
		out["model_reasoning_effort"] = v.ModelReasoningEffort
	}
	if v.ModelReasoningSummary.IsSet() {
		out["model_reasoning_summary"] = v.ModelReasoningSummary
	}
	if v.ModelVerbosity.IsSet() {
		out["model_verbosity"] = v.ModelVerbosity
	}
	if v.ReviewModel.IsSet() {
		out["review_model"] = v.ReviewModel
	}
	if v.SandboxMode.IsSet() {
		out["sandbox_mode"] = v.SandboxMode
	}
	if v.SandboxWorkspaceWrite.IsSet() {
		out["sandbox_workspace_write"] = v.SandboxWorkspaceWrite
	}
	if v.ServiceTier.IsSet() {
		out["service_tier"] = v.ServiceTier
	}
	if v.Tools.IsSet() {
		out["tools"] = v.Tools
	}
	if v.WebSearch.IsSet() {
		out["web_search"] = v.WebSearch
	}
	return json.Marshal(out)
}

func (v *Config) UnmarshalJSON(data []byte) error {
	type wire Config
	var s objectScan
	if _, err := s.begin(data, &configSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.Additional = s.unknownMembers()
	if len(v.Additional) == 0 {
		if err := json.Unmarshal([]byte("{}"), &v.Additional); err != nil {
			return fmt.Errorf("field *: %w", err)
		}
	}
	return nil
}

type ConfigBatchWriteParams struct {
	Edits            []ConfigEdit          `json:"edits"`
	ExpectedVersion  Optional[string]      `json:"expectedVersion,omitzero"`
	FilePath         Optional[string]      `json:"filePath,omitzero"`
	ReloadUserConfig OptionalNonNull[bool] `json:"reloadUserConfig,omitzero"`
}

var configBatchWriteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "edits", flags: fieldRequired | fieldRejectNull},
		{name: "expectedVersion"},
		{name: "filePath"},
		{name: "reloadUserConfig", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v ConfigBatchWriteParams) MarshalJSON() ([]byte, error) {
	type wire ConfigBatchWriteParams
	w := wire(v)
	w.ReloadUserConfig = nonNullWithoutFalse(w.ReloadUserConfig)
	return json.Marshal(w)
}

func (v *ConfigBatchWriteParams) UnmarshalJSON(data []byte) error {
	type wire ConfigBatchWriteParams
	return decodeObject(data, &configBatchWriteParamsSpec, (*wire)(v))
}

type ConfigEdit struct {
	KeyPath       string          `json:"keyPath"`
	MergeStrategy MergeStrategy   `json:"mergeStrategy"`
	Value         json.RawMessage `json:"value"`
}

var configEditSpec = objectSpec{
	fields: []objectField{
		{name: "keyPath", flags: fieldRequired | fieldRejectNull},
		{name: "mergeStrategy", flags: fieldRequired | fieldRejectNull},
		{name: "value", flags: fieldRequired},
	},
}

func (v ConfigEdit) MarshalJSON() ([]byte, error) {
	type wire ConfigEdit
	return json.Marshal(wire(v))
}

func (v *ConfigEdit) UnmarshalJSON(data []byte) error {
	type wire ConfigEdit
	return decodeObject(data, &configEditSpec, (*wire)(v))
}

type ConfigLayer struct {
	Config         json.RawMessage   `json:"config"`
	DisabledReason Optional[string]  `json:"disabledReason,omitzero"`
	Name           ConfigLayerSource `json:"name"`
	Version        string            `json:"version"`
}

var configLayerSpec = objectSpec{
	fields: []objectField{
		{name: "config", flags: fieldRequired},
		{name: "disabledReason"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "version", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConfigLayer) MarshalJSON() ([]byte, error) {
	type wire ConfigLayer
	w := wire(v)
	w.DisabledReason = w.DisabledReason.withoutNull()
	return json.Marshal(w)
}

func (v *ConfigLayer) UnmarshalJSON(data []byte) error {
	type wire ConfigLayer
	return decodeObject(data, &configLayerSpec, (*wire)(v))
}

type ConfigLayerMetadata struct {
	Name    ConfigLayerSource `json:"name"`
	Version string            `json:"version"`
}

var configLayerMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "version", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConfigLayerMetadata) MarshalJSON() ([]byte, error) {
	type wire ConfigLayerMetadata
	return json.Marshal(wire(v))
}

func (v *ConfigLayerMetadata) UnmarshalJSON(data []byte) error {
	type wire ConfigLayerMetadata
	return decodeObject(data, &configLayerMetadataSpec, (*wire)(v))
}

type ConfigLayerSource struct {
	Domain         OptionalNonNull[string]          `json:"domain,omitzero"`
	DotCodexFolder OptionalNonNull[AbsolutePathBuf] `json:"dotCodexFolder,omitzero"`
	File           OptionalNonNull[AbsolutePathBuf] `json:"file,omitzero"`
	ID             OptionalNonNull[string]          `json:"id,omitzero"`
	Key            OptionalNonNull[string]          `json:"key,omitzero"`
	Name           OptionalNonNull[string]          `json:"name,omitzero"`
	Profile        Optional[string]                 `json:"profile,omitzero"`
	TypeValue      string                           `json:"type"`
	RawJSON        json.RawMessage                  `json:"-"`
}

var configLayerSourceSpec = objectSpec{
	fields: []objectField{
		{name: "domain", flags: fieldNonNullOptional},
		{name: "dotCodexFolder", flags: fieldNonNullOptional},
		{name: "file", flags: fieldNonNullOptional},
		{name: "id", flags: fieldNonNullOptional},
		{name: "key", flags: fieldNonNullOptional},
		{name: "name", flags: fieldNonNullOptional},
		{name: "profile"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  7,
	tags: []string{"packagedDefaults", "mdm", "system", "enterpriseManaged", "user", "project", "sessionFlags", "legacyManagedConfigTomlFromFile", "legacyManagedConfigTomlFromMdm"},
}

func (v ConfigLayerSource) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "packagedDefaults":
		if !v.File.IsSet() {
			return nil, DecodeError{Field: "file", Reason: "missing required field for type packagedDefaults"}
		}
	case "mdm":
		if !v.Domain.IsSet() {
			return nil, DecodeError{Field: "domain", Reason: "missing required field for type mdm"}
		}
		if !v.Key.IsSet() {
			return nil, DecodeError{Field: "key", Reason: "missing required field for type mdm"}
		}
	case "system":
		if !v.File.IsSet() {
			return nil, DecodeError{Field: "file", Reason: "missing required field for type system"}
		}
	case "enterpriseManaged":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type enterpriseManaged"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type enterpriseManaged"}
		}
	case "user":
		if !v.File.IsSet() {
			return nil, DecodeError{Field: "file", Reason: "missing required field for type user"}
		}
	case "project":
		if !v.DotCodexFolder.IsSet() {
			return nil, DecodeError{Field: "dotCodexFolder", Reason: "missing required field for type project"}
		}
	case "sessionFlags":
	case "legacyManagedConfigTomlFromFile":
		if !v.File.IsSet() {
			return nil, DecodeError{Field: "file", Reason: "missing required field for type legacyManagedConfigTomlFromFile"}
		}
	case "legacyManagedConfigTomlFromMdm":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ConfigLayerSource
	return json.Marshal(wire(v))
}

func (v *ConfigLayerSource) UnmarshalJSON(data []byte) error {
	type wire ConfigLayerSource
	var s objectScan
	known, err := s.begin(data, &configLayerSourceSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "packagedDefaults":
		if !s.has("file") {
			return DecodeError{Field: "file", Reason: "missing required field for type packagedDefaults"}
		}
		if s.isNull("file") {
			return DecodeError{Field: "file", Reason: "cannot be null"}
		}
	case "mdm":
		if !s.has("domain") {
			return DecodeError{Field: "domain", Reason: "missing required field for type mdm"}
		}
		if s.isNull("domain") {
			return DecodeError{Field: "domain", Reason: "cannot be null"}
		}
		if !s.has("key") {
			return DecodeError{Field: "key", Reason: "missing required field for type mdm"}
		}
		if s.isNull("key") {
			return DecodeError{Field: "key", Reason: "cannot be null"}
		}
	case "system":
		if !s.has("file") {
			return DecodeError{Field: "file", Reason: "missing required field for type system"}
		}
		if s.isNull("file") {
			return DecodeError{Field: "file", Reason: "cannot be null"}
		}
	case "enterpriseManaged":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type enterpriseManaged"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type enterpriseManaged"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
	case "user":
		if !s.has("file") {
			return DecodeError{Field: "file", Reason: "missing required field for type user"}
		}
		if s.isNull("file") {
			return DecodeError{Field: "file", Reason: "cannot be null"}
		}
	case "project":
		if !s.has("dotCodexFolder") {
			return DecodeError{Field: "dotCodexFolder", Reason: "missing required field for type project"}
		}
		if s.isNull("dotCodexFolder") {
			return DecodeError{Field: "dotCodexFolder", Reason: "cannot be null"}
		}
	case "sessionFlags":
	case "legacyManagedConfigTomlFromFile":
		if !s.has("file") {
			return DecodeError{Field: "file", Reason: "missing required field for type legacyManagedConfigTomlFromFile"}
		}
		if s.isNull("file") {
			return DecodeError{Field: "file", Reason: "cannot be null"}
		}
	case "legacyManagedConfigTomlFromMdm":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ConfigReadParams struct {
	Cwd           Optional[string]      `json:"cwd,omitzero"`
	IncludeLayers OptionalNonNull[bool] `json:"includeLayers,omitzero"`
}

var configReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "includeLayers", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v ConfigReadParams) MarshalJSON() ([]byte, error) {
	type wire ConfigReadParams
	w := wire(v)
	w.IncludeLayers = nonNullWithoutFalse(w.IncludeLayers)
	return json.Marshal(w)
}

func (v *ConfigReadParams) UnmarshalJSON(data []byte) error {
	type wire ConfigReadParams
	return decodeObject(data, &configReadParamsSpec, (*wire)(v))
}

type ConfigReadResponse struct {
	Config  Config                         `json:"config"`
	Layers  Optional[[]ConfigLayer]        `json:"layers,omitzero"`
	Origins map[string]ConfigLayerMetadata `json:"origins"`
}

var configReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "config", flags: fieldRequired | fieldRejectNull},
		{name: "layers"},
		{name: "origins", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConfigReadResponse) MarshalJSON() ([]byte, error) {
	type wire ConfigReadResponse
	w := wire(v)
	w.Layers = w.Layers.withoutNull()
	return json.Marshal(w)
}

func (v *ConfigReadResponse) UnmarshalJSON(data []byte) error {
	type wire ConfigReadResponse
	return decodeObject(data, &configReadResponseSpec, (*wire)(v))
}

type ConfigRequirements struct {
	AdditionalDeveloperInstructions      Optional[string]                         `json:"additionalDeveloperInstructions,omitzero"`
	AllowAppshots                        Optional[bool]                           `json:"allowAppshots,omitzero"`
	AllowBrowserAndComputerUse           Optional[bool]                           `json:"allowBrowserAndComputerUse,omitzero"`
	AllowLoginShell                      Optional[bool]                           `json:"allowLoginShell,omitzero"`
	AllowManagedHooksOnly                Optional[bool]                           `json:"allowManagedHooksOnly,omitzero"`
	AllowRemoteControl                   Optional[bool]                           `json:"allowRemoteControl,omitzero"`
	AllowedApprovalPolicies              Optional[[]AskForApproval]               `json:"allowedApprovalPolicies,omitzero"`
	AllowedApprovalsReviewers            Optional[[]ApprovalsReviewer]            `json:"allowedApprovalsReviewers,omitzero"`
	AllowedLoginMethods                  Optional[[]ForcedLoginMethod]            `json:"allowedLoginMethods,omitzero"`
	AllowedPermissionProfiles            Optional[map[string]bool]                `json:"allowedPermissionProfiles,omitzero"`
	AllowedSandboxModes                  Optional[[]SandboxMode]                  `json:"allowedSandboxModes,omitzero"`
	AllowedWebSearchModes                Optional[[]WebSearchMode]                `json:"allowedWebSearchModes,omitzero"`
	AllowedWindowsSandboxImplementations Optional[[]WindowsSandboxImplementation] `json:"allowedWindowsSandboxImplementations,omitzero"`
	Application                          Optional[ApplicationRequirements]        `json:"application,omitzero"`
	AutoReview                           Optional[AutoReviewRequirements]         `json:"autoReview,omitzero"`
	BrowserUse                           Optional[BrowserUseRequirements]         `json:"browserUse,omitzero"`
	ChatgptBaseURL                       Optional[string]                         `json:"chatgptBaseUrl,omitzero"`
	CheckForUpdateOnStartup              Optional[bool]                           `json:"checkForUpdateOnStartup,omitzero"`
	CliAuthCredentialsStore              Optional[CliAuthCredentialsStoreMode]    `json:"cliAuthCredentialsStore,omitzero"`
	ComputerUse                          Optional[ComputerUseRequirements]        `json:"computerUse,omitzero"`
	DefaultPermissions                   Optional[string]                         `json:"defaultPermissions,omitzero"`
	EnforceResidency                     Optional[ResidencyRequirement]           `json:"enforceResidency,omitzero"`
	FeatureRequirements                  Optional[map[string]bool]                `json:"featureRequirements,omitzero"`
	Feedback                             Optional[FeedbackRequirements]           `json:"feedback,omitzero"`
	Hooks                                Optional[ManagedHooksRequirements]       `json:"hooks,omitzero"`
	InAppBrowser                         Optional[InAppBrowserRequirements]       `json:"inAppBrowser,omitzero"`
	LogDir                               Optional[string]                         `json:"logDir,omitzero"`
	ModelCatalogJSON                     Optional[string]                         `json:"modelCatalogJson,omitzero"`
	ModelProvider                        Optional[string]                         `json:"modelProvider,omitzero"`
	ModelProviders                       Optional[map[string]json.RawMessage]     `json:"modelProviders,omitzero"`
	Models                               Optional[ModelsRequirements]             `json:"models,omitzero"`
	Network                              Optional[NetworkRequirements]            `json:"network,omitzero"`
	SqliteHome                           Optional[string]                         `json:"sqliteHome,omitzero"`
}

var configRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "additionalDeveloperInstructions"},
		{name: "allowAppshots"},
		{name: "allowBrowserAndComputerUse"},
		{name: "allowLoginShell"},
		{name: "allowManagedHooksOnly"},
		{name: "allowRemoteControl"},
		{name: "allowedApprovalPolicies"},
		{name: "allowedApprovalsReviewers"},
		{name: "allowedLoginMethods"},
		{name: "allowedPermissionProfiles"},
		{name: "allowedSandboxModes"},
		{name: "allowedWebSearchModes"},
		{name: "allowedWindowsSandboxImplementations"},
		{name: "application"},
		{name: "autoReview"},
		{name: "browserUse"},
		{name: "chatgptBaseUrl"},
		{name: "checkForUpdateOnStartup"},
		{name: "cliAuthCredentialsStore"},
		{name: "computerUse"},
		{name: "defaultPermissions"},
		{name: "enforceResidency"},
		{name: "featureRequirements"},
		{name: "feedback"},
		{name: "hooks"},
		{name: "inAppBrowser"},
		{name: "logDir"},
		{name: "modelCatalogJson"},
		{name: "modelProvider"},
		{name: "modelProviders"},
		{name: "models"},
		{name: "network"},
		{name: "sqliteHome"},
	},
}

func (v ConfigRequirements) MarshalJSON() ([]byte, error) {
	type wire ConfigRequirements
	return json.Marshal(wire(v))
}

func (v *ConfigRequirements) UnmarshalJSON(data []byte) error {
	type wire ConfigRequirements
	return decodeObject(data, &configRequirementsSpec, (*wire)(v))
}

type ConfigRequirementsReadResponse struct {
	Requirements                  Optional[ConfigRequirements] `json:"requirements,omitzero"`
	SupportsIndependentSpeedModes Optional[bool]               `json:"supportsIndependentSpeedModes,omitzero"`
}

var configRequirementsReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "requirements"},
		{name: "supportsIndependentSpeedModes"},
	},
}

func (v ConfigRequirementsReadResponse) MarshalJSON() ([]byte, error) {
	type wire ConfigRequirementsReadResponse
	return json.Marshal(wire(v))
}

func (v *ConfigRequirementsReadResponse) UnmarshalJSON(data []byte) error {
	type wire ConfigRequirementsReadResponse
	return decodeObject(data, &configRequirementsReadResponseSpec, (*wire)(v))
}

type ConfigValueWriteParams struct {
	ExpectedVersion Optional[string] `json:"expectedVersion,omitzero"`
	FilePath        Optional[string] `json:"filePath,omitzero"`
	KeyPath         string           `json:"keyPath"`
	MergeStrategy   MergeStrategy    `json:"mergeStrategy"`
	Value           json.RawMessage  `json:"value"`
}

var configValueWriteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "expectedVersion"},
		{name: "filePath"},
		{name: "keyPath", flags: fieldRequired | fieldRejectNull},
		{name: "mergeStrategy", flags: fieldRequired | fieldRejectNull},
		{name: "value", flags: fieldRequired},
	},
}

func (v ConfigValueWriteParams) MarshalJSON() ([]byte, error) {
	type wire ConfigValueWriteParams
	return json.Marshal(wire(v))
}

func (v *ConfigValueWriteParams) UnmarshalJSON(data []byte) error {
	type wire ConfigValueWriteParams
	return decodeObject(data, &configValueWriteParamsSpec, (*wire)(v))
}

type ConfigWarningNotification struct {
	Details    Optional[string]    `json:"details,omitzero"`
	Path       Optional[string]    `json:"path,omitzero"`
	RangeValue Optional[TextRange] `json:"range,omitzero"`
	Summary    string              `json:"summary"`
}

var configWarningNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "details"},
		{name: "path"},
		{name: "range"},
		{name: "summary", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConfigWarningNotification) MarshalJSON() ([]byte, error) {
	type wire ConfigWarningNotification
	w := wire(v)
	w.Path = w.Path.withoutNull()
	w.RangeValue = w.RangeValue.withoutNull()
	return json.Marshal(w)
}

func (v *ConfigWarningNotification) UnmarshalJSON(data []byte) error {
	type wire ConfigWarningNotification
	return decodeObject(data, &configWarningNotificationSpec, (*wire)(v))
}

type ConfigWriteResponse struct {
	FilePath           AbsolutePathBuf              `json:"filePath"`
	OverriddenMetadata Optional[OverriddenMetadata] `json:"overriddenMetadata,omitzero"`
	Status             WriteStatus                  `json:"status"`
	Version            string                       `json:"version"`
}

var configWriteResponseSpec = objectSpec{
	fields: []objectField{
		{name: "filePath", flags: fieldRequired | fieldRejectNull},
		{name: "overriddenMetadata"},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "version", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConfigWriteResponse) MarshalJSON() ([]byte, error) {
	type wire ConfigWriteResponse
	return json.Marshal(wire(v))
}

func (v *ConfigWriteResponse) UnmarshalJSON(data []byte) error {
	type wire ConfigWriteResponse
	return decodeObject(data, &configWriteResponseSpec, (*wire)(v))
}

type ConfigurationReasoning struct {
	Effort ReasoningEffort `json:"effort"`
}

var configurationReasoningSpec = objectSpec{
	fields: []objectField{
		{name: "effort", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConfigurationReasoning) MarshalJSON() ([]byte, error) {
	type wire ConfigurationReasoning
	return json.Marshal(wire(v))
}

func (v *ConfigurationReasoning) UnmarshalJSON(data []byte) error {
	type wire ConfigurationReasoning
	return decodeObject(data, &configurationReasoningSpec, (*wire)(v))
}

type ConfiguredHookHandler struct {
	AdditionalContextLimit Optional[uint64]                            `json:"additionalContextLimit,omitzero"`
	Async                  OptionalNonNull[bool]                       `json:"async,omitzero"`
	Command                OptionalNonNull[string]                     `json:"command,omitzero"`
	CommandWindows         Optional[string]                            `json:"commandWindows,omitzero"`
	Input                  OptionalNonNull[map[string]json.RawMessage] `json:"input,omitzero"`
	Server                 OptionalNonNull[string]                     `json:"server,omitzero"`
	StatusMessage          Optional[string]                            `json:"statusMessage,omitzero"`
	TimeoutSec             Optional[uint64]                            `json:"timeoutSec,omitzero"`
	Tool                   OptionalNonNull[string]                     `json:"tool,omitzero"`
	TypeValue              string                                      `json:"type"`
	RawJSON                json.RawMessage                             `json:"-"`
}

var configuredHookHandlerSpec = objectSpec{
	fields: []objectField{
		{name: "additionalContextLimit"},
		{name: "async", flags: fieldNonNullOptional},
		{name: "command", flags: fieldNonNullOptional},
		{name: "commandWindows"},
		{name: "input", flags: fieldNonNullOptional},
		{name: "server", flags: fieldNonNullOptional},
		{name: "statusMessage"},
		{name: "timeoutSec"},
		{name: "tool", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  9,
	tags: []string{"command", "mcp_tool", "prompt", "agent"},
}

func (v ConfiguredHookHandler) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "command":
		if !v.Async.IsSet() {
			return nil, DecodeError{Field: "async", Reason: "missing required field for type command"}
		}
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type command"}
		}
	case "mcp_tool":
		if !v.Input.IsSet() {
			return nil, DecodeError{Field: "input", Reason: "missing required field for type mcp_tool"}
		}
		if !v.Server.IsSet() {
			return nil, DecodeError{Field: "server", Reason: "missing required field for type mcp_tool"}
		}
		if !v.Tool.IsSet() {
			return nil, DecodeError{Field: "tool", Reason: "missing required field for type mcp_tool"}
		}
	case "prompt":
	case "agent":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ConfiguredHookHandler
	return json.Marshal(wire(v))
}

func (v *ConfiguredHookHandler) UnmarshalJSON(data []byte) error {
	type wire ConfiguredHookHandler
	var s objectScan
	known, err := s.begin(data, &configuredHookHandlerSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueAdditionalContextLimit, ok := v.AdditionalContextLimit.Value(); ok {
		if valueAdditionalContextLimit < 0 {
			return DecodeError{Field: "additionalContextLimit", Reason: "below minimum 0"}
		}
	}
	if valueTimeoutSec, ok := v.TimeoutSec.Value(); ok {
		if valueTimeoutSec < 0 {
			return DecodeError{Field: "timeoutSec", Reason: "below minimum 0"}
		}
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "command":
		if !s.has("async") {
			return DecodeError{Field: "async", Reason: "missing required field for type command"}
		}
		if s.isNull("async") {
			return DecodeError{Field: "async", Reason: "cannot be null"}
		}
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type command"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
	case "mcp_tool":
		if !s.has("input") {
			return DecodeError{Field: "input", Reason: "missing required field for type mcp_tool"}
		}
		if s.isNull("input") {
			return DecodeError{Field: "input", Reason: "cannot be null"}
		}
		if !s.has("server") {
			return DecodeError{Field: "server", Reason: "missing required field for type mcp_tool"}
		}
		if s.isNull("server") {
			return DecodeError{Field: "server", Reason: "cannot be null"}
		}
		if !s.has("tool") {
			return DecodeError{Field: "tool", Reason: "missing required field for type mcp_tool"}
		}
		if s.isNull("tool") {
			return DecodeError{Field: "tool", Reason: "cannot be null"}
		}
	case "prompt":
	case "agent":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ConfiguredHookMatcherGroup struct {
	Hooks   []ConfiguredHookHandler `json:"hooks"`
	Matcher Optional[string]        `json:"matcher,omitzero"`
}

var configuredHookMatcherGroupSpec = objectSpec{
	fields: []objectField{
		{name: "hooks", flags: fieldRequired | fieldRejectNull},
		{name: "matcher"},
	},
}

func (v ConfiguredHookMatcherGroup) MarshalJSON() ([]byte, error) {
	type wire ConfiguredHookMatcherGroup
	return json.Marshal(wire(v))
}

func (v *ConfiguredHookMatcherGroup) UnmarshalJSON(data []byte) error {
	type wire ConfiguredHookMatcherGroup
	return decodeObject(data, &configuredHookMatcherGroupSpec, (*wire)(v))
}

type ConnectorMetadata struct {
	Description         Optional[string]           `json:"description,omitzero"`
	DistributionChannel Optional[string]           `json:"distributionChannel,omitzero"`
	IconURL             Optional[string]           `json:"iconUrl,omitzero"`
	IconURLDark         Optional[string]           `json:"iconUrlDark,omitzero"`
	ID                  string                     `json:"id"`
	InstallURL          Optional[string]           `json:"installUrl,omitzero"`
	Name                string                     `json:"name"`
	PluginDisplayNames  OptionalNonNull[[]string]  `json:"pluginDisplayNames,omitzero"`
	ToolSummaries       Optional[[]AppToolSummary] `json:"toolSummaries,omitzero"`
}

var connectorMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "description"},
		{name: "distributionChannel"},
		{name: "iconUrl"},
		{name: "iconUrlDark"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "installUrl"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "pluginDisplayNames", flags: fieldNonNullOptional},
		{name: "toolSummaries"},
	},
}

func (v ConnectorMetadata) MarshalJSON() ([]byte, error) {
	type wire ConnectorMetadata
	return json.Marshal(wire(v))
}

func (v *ConnectorMetadata) UnmarshalJSON(data []byte) error {
	type wire ConnectorMetadata
	return decodeObject(data, &connectorMetadataSpec, (*wire)(v))
}

type ConsumeAccountRateLimitResetCreditOutcome string

const (
	ConsumeAccountRateLimitResetCreditOutcomeReset           ConsumeAccountRateLimitResetCreditOutcome = "reset"
	ConsumeAccountRateLimitResetCreditOutcomeNothingToReset  ConsumeAccountRateLimitResetCreditOutcome = "nothingToReset"
	ConsumeAccountRateLimitResetCreditOutcomeNoCredit        ConsumeAccountRateLimitResetCreditOutcome = "noCredit"
	ConsumeAccountRateLimitResetCreditOutcomeAlreadyRedeemed ConsumeAccountRateLimitResetCreditOutcome = "alreadyRedeemed"
)

type ConsumeAccountRateLimitResetCreditParams struct {
	CreditID       Optional[string] `json:"creditId,omitzero"`
	IDempotencyKey string           `json:"idempotencyKey"`
}

var consumeAccountRateLimitResetCreditParamsSpec = objectSpec{
	fields: []objectField{
		{name: "creditId"},
		{name: "idempotencyKey", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConsumeAccountRateLimitResetCreditParams) MarshalJSON() ([]byte, error) {
	type wire ConsumeAccountRateLimitResetCreditParams
	return json.Marshal(wire(v))
}

func (v *ConsumeAccountRateLimitResetCreditParams) UnmarshalJSON(data []byte) error {
	type wire ConsumeAccountRateLimitResetCreditParams
	return decodeObject(data, &consumeAccountRateLimitResetCreditParamsSpec, (*wire)(v))
}

type ConsumeAccountRateLimitResetCreditResponse struct {
	Outcome ConsumeAccountRateLimitResetCreditOutcome `json:"outcome"`
}

var consumeAccountRateLimitResetCreditResponseSpec = objectSpec{
	fields: []objectField{
		{name: "outcome", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ConsumeAccountRateLimitResetCreditResponse) MarshalJSON() ([]byte, error) {
	type wire ConsumeAccountRateLimitResetCreditResponse
	return json.Marshal(wire(v))
}

func (v *ConsumeAccountRateLimitResetCreditResponse) UnmarshalJSON(data []byte) error {
	type wire ConsumeAccountRateLimitResetCreditResponse
	return decodeObject(data, &consumeAccountRateLimitResetCreditResponseSpec, (*wire)(v))
}

type ContentItem struct {
	Annotations Optional[[]json.RawMessage] `json:"annotations,omitzero"`
	AudioURL    OptionalNonNull[string]     `json:"audio_url,omitzero"`
	Detail      Optional[ImageDetail]       `json:"detail,omitzero"`
	FileID      OptionalNonNull[string]     `json:"file_id,omitzero"`
	ImageURL    OptionalNonNull[string]     `json:"image_url,omitzero"`
	Logprobs    Optional[[]json.RawMessage] `json:"logprobs,omitzero"`
	Text        OptionalNonNull[string]     `json:"text,omitzero"`
	TypeValue   string                      `json:"type"`
	RawJSON     json.RawMessage             `json:"-"`
}

var contentItemSpec = objectSpec{
	fields: []objectField{
		{name: "annotations"},
		{name: "audio_url", flags: fieldNonNullOptional},
		{name: "detail"},
		{name: "file_id", flags: fieldNonNullOptional, anyOfTags: []string{"\"input_image\""}},
		{name: "image_url", flags: fieldNonNullOptional, anyOfTags: []string{"\"input_image\""}},
		{name: "logprobs"},
		{name: "text", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  7,
	tags: []string{"input_text", "input_image", "input_audio", "output_text"},
}

func (v ContentItem) MarshalJSON() ([]byte, error) {
	type wire ContentItem
	w := wire(v)
	switch v.TypeValue {
	case "input_text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type input_text"}
		}
	case "input_image":
		taggedAnyOfVariant1Matches := (w.ImageURL.IsSet() && marshalsNonNull(w.ImageURL)) || (w.FileID.IsSet() && marshalsNonNull(w.FileID))
		if !taggedAnyOfVariant1Matches {
			return nil, DecodeError{Field: "", Reason: "does not match any anyOf variant for type input_image"}
		}
	case "input_audio":
		if !v.AudioURL.IsSet() {
			return nil, DecodeError{Field: "audio_url", Reason: "missing required field for type input_audio"}
		}
	case "output_text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type output_text"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	return json.Marshal(w)
}

func (v *ContentItem) UnmarshalJSON(data []byte) error {
	type wire ContentItem
	var s objectScan
	known, err := s.begin(data, &contentItemSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "input_text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type input_text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "input_image":
		taggedAnyOfVariant1Matches := (v.ImageURL.IsSet()) || (v.FileID.IsSet())
		if !taggedAnyOfVariant1Matches {
			return DecodeError{Field: "", Reason: "does not match any anyOf variant for type input_image"}
		}
	case "input_audio":
		if !s.has("audio_url") {
			return DecodeError{Field: "audio_url", Reason: "missing required field for type input_audio"}
		}
		if s.isNull("audio_url") {
			return DecodeError{Field: "audio_url", Reason: "cannot be null"}
		}
	case "output_text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type output_text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ContextCompactedNotification struct {
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var contextCompactedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ContextCompactedNotification) MarshalJSON() ([]byte, error) {
	type wire ContextCompactedNotification
	return json.Marshal(wire(v))
}

func (v *ContextCompactedNotification) UnmarshalJSON(data []byte) error {
	type wire ContextCompactedNotification
	return decodeObject(data, &contextCompactedNotificationSpec, (*wire)(v))
}

type ConversationTextRole string

const (
	ConversationTextRoleUser      ConversationTextRole = "user"
	ConversationTextRoleDeveloper ConversationTextRole = "developer"
	ConversationTextRoleAssistant ConversationTextRole = "assistant"
)

type CreditsSnapshot struct {
	Balance    Optional[string] `json:"balance,omitzero"`
	HasCredits bool             `json:"hasCredits"`
	Unlimited  bool             `json:"unlimited"`
}

var creditsSnapshotSpec = objectSpec{
	fields: []objectField{
		{name: "balance"},
		{name: "hasCredits", flags: fieldRequired | fieldRejectNull},
		{name: "unlimited", flags: fieldRequired | fieldRejectNull},
	},
}

func (v CreditsSnapshot) MarshalJSON() ([]byte, error) {
	type wire CreditsSnapshot
	return json.Marshal(wire(v))
}

func (v *CreditsSnapshot) UnmarshalJSON(data []byte) error {
	type wire CreditsSnapshot
	return decodeObject(data, &creditsSnapshotSpec, (*wire)(v))
}

type CyberAccessProgram string

const (
	CyberAccessProgramStandard     CyberAccessProgram = "standard"
	CyberAccessProgramDaybreakBlue CyberAccessProgram = "daybreakBlue"
	CyberAccessProgramDaybreakRed  CyberAccessProgram = "daybreakRed"
)

type DeprecationNoticeNotification struct {
	Details Optional[string] `json:"details,omitzero"`
	Summary string           `json:"summary"`
}

var deprecationNoticeNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "details"},
		{name: "summary", flags: fieldRequired | fieldRejectNull},
	},
}

func (v DeprecationNoticeNotification) MarshalJSON() ([]byte, error) {
	type wire DeprecationNoticeNotification
	return json.Marshal(wire(v))
}

func (v *DeprecationNoticeNotification) UnmarshalJSON(data []byte) error {
	type wire DeprecationNoticeNotification
	return decodeObject(data, &deprecationNoticeNotificationSpec, (*wire)(v))
}

type DesktopOnboardingEntrypoint string

const (
	DesktopOnboardingEntrypointLifeSciences DesktopOnboardingEntrypoint = "life_sciences"
)

type DynamicToolCallOutputContentItem struct {
	AudioURL  OptionalNonNull[string] `json:"audioUrl,omitzero"`
	ImageURL  OptionalNonNull[string] `json:"imageUrl,omitzero"`
	Text      OptionalNonNull[string] `json:"text,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var dynamicToolCallOutputContentItemSpec = objectSpec{
	fields: []objectField{
		{name: "audioUrl", flags: fieldNonNullOptional},
		{name: "imageUrl", flags: fieldNonNullOptional},
		{name: "text", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  3,
	tags: []string{"inputText", "inputImage", "inputAudio"},
}

func (v DynamicToolCallOutputContentItem) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "inputText":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type inputText"}
		}
	case "inputImage":
		if !v.ImageURL.IsSet() {
			return nil, DecodeError{Field: "imageUrl", Reason: "missing required field for type inputImage"}
		}
	case "inputAudio":
		if !v.AudioURL.IsSet() {
			return nil, DecodeError{Field: "audioUrl", Reason: "missing required field for type inputAudio"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire DynamicToolCallOutputContentItem
	return json.Marshal(wire(v))
}

func (v *DynamicToolCallOutputContentItem) UnmarshalJSON(data []byte) error {
	type wire DynamicToolCallOutputContentItem
	var s objectScan
	known, err := s.begin(data, &dynamicToolCallOutputContentItemSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "inputText":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type inputText"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "inputImage":
		if !s.has("imageUrl") {
			return DecodeError{Field: "imageUrl", Reason: "missing required field for type inputImage"}
		}
		if s.isNull("imageUrl") {
			return DecodeError{Field: "imageUrl", Reason: "cannot be null"}
		}
	case "inputAudio":
		if !s.has("audioUrl") {
			return DecodeError{Field: "audioUrl", Reason: "missing required field for type inputAudio"}
		}
		if s.isNull("audioUrl") {
			return DecodeError{Field: "audioUrl", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type DynamicToolCallStatus string

const (
	DynamicToolCallStatusInProgress DynamicToolCallStatus = "inProgress"
	DynamicToolCallStatusCompleted  DynamicToolCallStatus = "completed"
	DynamicToolCallStatusFailed     DynamicToolCallStatus = "failed"
)

type DynamicToolNamespaceTool struct {
	DeferLoading OptionalNonNull[bool]   `json:"deferLoading,omitzero"`
	Description  OptionalNonNull[string] `json:"description,omitzero"`
	InputSchema  json.RawMessage         `json:"inputSchema,omitempty"`
	Name         OptionalNonNull[string] `json:"name,omitzero"`
	TypeValue    string                  `json:"type"`
	RawJSON      json.RawMessage         `json:"-"`
}

var dynamicToolNamespaceToolSpec = objectSpec{
	fields: []objectField{
		{name: "deferLoading", flags: fieldNonNullOptional},
		{name: "description", flags: fieldNonNullOptional},
		{name: "inputSchema"},
		{name: "name", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  4,
	tags: []string{"function"},
}

func (v DynamicToolNamespaceTool) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "function":
		if !v.Description.IsSet() {
			return nil, DecodeError{Field: "description", Reason: "missing required field for type function"}
		}
		if len(v.InputSchema) == 0 {
			return nil, DecodeError{Field: "inputSchema", Reason: "missing required field for type function"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type function"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire DynamicToolNamespaceTool
	return json.Marshal(wire(v))
}

func (v *DynamicToolNamespaceTool) UnmarshalJSON(data []byte) error {
	type wire DynamicToolNamespaceTool
	var s objectScan
	known, err := s.begin(data, &dynamicToolNamespaceToolSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "function":
		if !s.has("description") {
			return DecodeError{Field: "description", Reason: "missing required field for type function"}
		}
		if s.isNull("description") {
			return DecodeError{Field: "description", Reason: "cannot be null"}
		}
		if !s.has("inputSchema") {
			return DecodeError{Field: "inputSchema", Reason: "missing required field for type function"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type function"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type DynamicToolSpec struct {
	DeferLoading OptionalNonNull[bool]                       `json:"deferLoading,omitzero"`
	Description  OptionalNonNull[string]                     `json:"description,omitzero"`
	InputSchema  json.RawMessage                             `json:"inputSchema,omitempty"`
	Name         OptionalNonNull[string]                     `json:"name,omitzero"`
	Tools        OptionalNonNull[[]DynamicToolNamespaceTool] `json:"tools,omitzero"`
	TypeValue    string                                      `json:"type"`
	RawJSON      json.RawMessage                             `json:"-"`
}

var dynamicToolSpecSpec = objectSpec{
	fields: []objectField{
		{name: "deferLoading", flags: fieldNonNullOptional},
		{name: "description", flags: fieldNonNullOptional},
		{name: "inputSchema"},
		{name: "name", flags: fieldNonNullOptional},
		{name: "tools", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  5,
	tags: []string{"function", "namespace"},
}

func (v DynamicToolSpec) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "function":
		if !v.Description.IsSet() {
			return nil, DecodeError{Field: "description", Reason: "missing required field for type function"}
		}
		if len(v.InputSchema) == 0 {
			return nil, DecodeError{Field: "inputSchema", Reason: "missing required field for type function"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type function"}
		}
	case "namespace":
		if !v.Description.IsSet() {
			return nil, DecodeError{Field: "description", Reason: "missing required field for type namespace"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type namespace"}
		}
		if !v.Tools.IsSet() {
			return nil, DecodeError{Field: "tools", Reason: "missing required field for type namespace"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire DynamicToolSpec
	return json.Marshal(wire(v))
}

func (v *DynamicToolSpec) UnmarshalJSON(data []byte) error {
	type wire DynamicToolSpec
	var s objectScan
	known, err := s.begin(data, &dynamicToolSpecSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "function":
		if !s.has("description") {
			return DecodeError{Field: "description", Reason: "missing required field for type function"}
		}
		if s.isNull("description") {
			return DecodeError{Field: "description", Reason: "cannot be null"}
		}
		if !s.has("inputSchema") {
			return DecodeError{Field: "inputSchema", Reason: "missing required field for type function"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type function"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
	case "namespace":
		if !s.has("description") {
			return DecodeError{Field: "description", Reason: "missing required field for type namespace"}
		}
		if s.isNull("description") {
			return DecodeError{Field: "description", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type namespace"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("tools") {
			return DecodeError{Field: "tools", Reason: "missing required field for type namespace"}
		}
		if s.isNull("tools") {
			return DecodeError{Field: "tools", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type EnvironmentAddParams struct {
	AuthBearerToken    Optional[string]                  `json:"authBearerToken,omitzero"`
	ConnectTimeoutMs   Optional[uint64]                  `json:"connectTimeoutMs,omitzero"`
	EnvironmentID      string                            `json:"environmentId"`
	ExecServerURL      string                            `json:"execServerUrl"`
	Skills             Optional[EnvironmentSkillsParams] `json:"skills,omitzero"`
	WebsocketRequestID Optional[string]                  `json:"websocketRequestId,omitzero"`
}

var environmentAddParamsSpec = objectSpec{
	fields: []objectField{
		{name: "authBearerToken"},
		{name: "connectTimeoutMs"},
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
		{name: "execServerUrl", flags: fieldRequired | fieldRejectNull},
		{name: "skills"},
		{name: "websocketRequestId"},
	},
}

func (v EnvironmentAddParams) MarshalJSON() ([]byte, error) {
	type wire EnvironmentAddParams
	return json.Marshal(wire(v))
}

func (v *EnvironmentAddParams) UnmarshalJSON(data []byte) error {
	type wire EnvironmentAddParams
	var s objectScan
	if _, err := s.begin(data, &environmentAddParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueConnectTimeoutMs, ok := v.ConnectTimeoutMs.Value(); ok {
		if valueConnectTimeoutMs < 0 {
			return DecodeError{Field: "connectTimeoutMs", Reason: "below minimum 0"}
		}
	}
	return nil
}

type EnvironmentAddResponse map[string]json.RawMessage

func (v EnvironmentAddResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *EnvironmentAddResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = EnvironmentAddResponse(raw)
	return nil
}

type EnvironmentConnectionNotification struct {
	EnvironmentID string `json:"environmentId"`
	ThreadID      string `json:"threadId"`
}

var environmentConnectionNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v EnvironmentConnectionNotification) MarshalJSON() ([]byte, error) {
	type wire EnvironmentConnectionNotification
	return json.Marshal(wire(v))
}

func (v *EnvironmentConnectionNotification) UnmarshalJSON(data []byte) error {
	type wire EnvironmentConnectionNotification
	return decodeObject(data, &environmentConnectionNotificationSpec, (*wire)(v))
}

type EnvironmentInfoParams struct {
	EnvironmentID string `json:"environmentId"`
}

var environmentInfoParamsSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v EnvironmentInfoParams) MarshalJSON() ([]byte, error) {
	type wire EnvironmentInfoParams
	return json.Marshal(wire(v))
}

func (v *EnvironmentInfoParams) UnmarshalJSON(data []byte) error {
	type wire EnvironmentInfoParams
	return decodeObject(data, &environmentInfoParamsSpec, (*wire)(v))
}

type EnvironmentInfoResponse struct {
	Cwd   Optional[PathUri]    `json:"cwd,omitzero"`
	Shell EnvironmentShellInfo `json:"shell"`
}

var environmentInfoResponseSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "shell", flags: fieldRequired | fieldRejectNull},
	},
}

func (v EnvironmentInfoResponse) MarshalJSON() ([]byte, error) {
	type wire EnvironmentInfoResponse
	return json.Marshal(wire(v))
}

func (v *EnvironmentInfoResponse) UnmarshalJSON(data []byte) error {
	type wire EnvironmentInfoResponse
	return decodeObject(data, &environmentInfoResponseSpec, (*wire)(v))
}

type EnvironmentShellInfo struct {
	Name string `json:"name"`
	Path string `json:"path"`
}

var environmentShellInfoSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v EnvironmentShellInfo) MarshalJSON() ([]byte, error) {
	type wire EnvironmentShellInfo
	return json.Marshal(wire(v))
}

func (v *EnvironmentShellInfo) UnmarshalJSON(data []byte) error {
	type wire EnvironmentShellInfo
	return decodeObject(data, &environmentShellInfoSpec, (*wire)(v))
}

type EnvironmentSkillsParams struct {
	Required Optional[[]string] `json:"required,omitzero"`
}

var environmentSkillsParamsSpec = objectSpec{
	fields: []objectField{
		{name: "required"},
	},
}

func (v EnvironmentSkillsParams) MarshalJSON() ([]byte, error) {
	type wire EnvironmentSkillsParams
	return json.Marshal(wire(v))
}

func (v *EnvironmentSkillsParams) UnmarshalJSON(data []byte) error {
	type wire EnvironmentSkillsParams
	return decodeObject(data, &environmentSkillsParamsSpec, (*wire)(v))
}

type EnvironmentStatusKind string

const (
	EnvironmentStatusKindReady        EnvironmentStatusKind = "ready"
	EnvironmentStatusKindPending      EnvironmentStatusKind = "pending"
	EnvironmentStatusKindDisconnected EnvironmentStatusKind = "disconnected"
	EnvironmentStatusKindUnknown      EnvironmentStatusKind = "unknown"
)

type EnvironmentStatusParams struct {
	EnvironmentID string `json:"environmentId"`
}

var environmentStatusParamsSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v EnvironmentStatusParams) MarshalJSON() ([]byte, error) {
	type wire EnvironmentStatusParams
	return json.Marshal(wire(v))
}

func (v *EnvironmentStatusParams) UnmarshalJSON(data []byte) error {
	type wire EnvironmentStatusParams
	return decodeObject(data, &environmentStatusParamsSpec, (*wire)(v))
}

type EnvironmentStatusResponse struct {
	Error  Optional[string]      `json:"error,omitzero"`
	Status EnvironmentStatusKind `json:"status"`
}

var environmentStatusResponseSpec = objectSpec{
	fields: []objectField{
		{name: "error"},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v EnvironmentStatusResponse) MarshalJSON() ([]byte, error) {
	type wire EnvironmentStatusResponse
	return json.Marshal(wire(v))
}

func (v *EnvironmentStatusResponse) UnmarshalJSON(data []byte) error {
	type wire EnvironmentStatusResponse
	return decodeObject(data, &environmentStatusResponseSpec, (*wire)(v))
}

type ErrorNotification struct {
	Error     TurnError `json:"error"`
	ThreadID  string    `json:"threadId"`
	TurnID    string    `json:"turnId"`
	WillRetry bool      `json:"willRetry"`
}

var errorNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "error", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
		{name: "willRetry", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ErrorNotification) MarshalJSON() ([]byte, error) {
	type wire ErrorNotification
	return json.Marshal(wire(v))
}

func (v *ErrorNotification) UnmarshalJSON(data []byte) error {
	type wire ErrorNotification
	return decodeObject(data, &errorNotificationSpec, (*wire)(v))
}

type ExperimentalFeature struct {
	Announcement   Optional[string]         `json:"announcement,omitzero"`
	DefaultEnabled bool                     `json:"defaultEnabled"`
	Description    Optional[string]         `json:"description,omitzero"`
	DisplayName    Optional[string]         `json:"displayName,omitzero"`
	Enabled        bool                     `json:"enabled"`
	Name           string                   `json:"name"`
	Stage          ExperimentalFeatureStage `json:"stage"`
}

var experimentalFeatureSpec = objectSpec{
	fields: []objectField{
		{name: "announcement"},
		{name: "defaultEnabled", flags: fieldRequired | fieldRejectNull},
		{name: "description"},
		{name: "displayName"},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "stage", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExperimentalFeature) MarshalJSON() ([]byte, error) {
	type wire ExperimentalFeature
	return json.Marshal(wire(v))
}

func (v *ExperimentalFeature) UnmarshalJSON(data []byte) error {
	type wire ExperimentalFeature
	return decodeObject(data, &experimentalFeatureSpec, (*wire)(v))
}

type ExperimentalFeatureEnablementSetParams struct {
	Enablement map[string]bool `json:"enablement"`
}

var experimentalFeatureEnablementSetParamsSpec = objectSpec{
	fields: []objectField{
		{name: "enablement", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExperimentalFeatureEnablementSetParams) MarshalJSON() ([]byte, error) {
	type wire ExperimentalFeatureEnablementSetParams
	return json.Marshal(wire(v))
}

func (v *ExperimentalFeatureEnablementSetParams) UnmarshalJSON(data []byte) error {
	type wire ExperimentalFeatureEnablementSetParams
	return decodeObject(data, &experimentalFeatureEnablementSetParamsSpec, (*wire)(v))
}

type ExperimentalFeatureEnablementSetResponse struct {
	Enablement map[string]bool `json:"enablement"`
}

var experimentalFeatureEnablementSetResponseSpec = objectSpec{
	fields: []objectField{
		{name: "enablement", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExperimentalFeatureEnablementSetResponse) MarshalJSON() ([]byte, error) {
	type wire ExperimentalFeatureEnablementSetResponse
	return json.Marshal(wire(v))
}

func (v *ExperimentalFeatureEnablementSetResponse) UnmarshalJSON(data []byte) error {
	type wire ExperimentalFeatureEnablementSetResponse
	return decodeObject(data, &experimentalFeatureEnablementSetResponseSpec, (*wire)(v))
}

type ExperimentalFeatureListParams struct {
	Cursor   Optional[string] `json:"cursor,omitzero"`
	Limit    Optional[uint32] `json:"limit,omitzero"`
	ThreadID Optional[string] `json:"threadId,omitzero"`
}

var experimentalFeatureListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "threadId"},
	},
}

func (v ExperimentalFeatureListParams) MarshalJSON() ([]byte, error) {
	type wire ExperimentalFeatureListParams
	return json.Marshal(wire(v))
}

func (v *ExperimentalFeatureListParams) UnmarshalJSON(data []byte) error {
	type wire ExperimentalFeatureListParams
	var s objectScan
	if _, err := s.begin(data, &experimentalFeatureListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ExperimentalFeatureListResponse struct {
	Data       []ExperimentalFeature `json:"data"`
	NextCursor Optional[string]      `json:"nextCursor,omitzero"`
}

var experimentalFeatureListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ExperimentalFeatureListResponse) MarshalJSON() ([]byte, error) {
	type wire ExperimentalFeatureListResponse
	return json.Marshal(wire(v))
}

func (v *ExperimentalFeatureListResponse) UnmarshalJSON(data []byte) error {
	type wire ExperimentalFeatureListResponse
	return decodeObject(data, &experimentalFeatureListResponseSpec, (*wire)(v))
}

type ExperimentalFeatureStage string

const (
	ExperimentalFeatureStageBeta             ExperimentalFeatureStage = "beta"
	ExperimentalFeatureStageUnderDevelopment ExperimentalFeatureStage = "underDevelopment"
	ExperimentalFeatureStageStable           ExperimentalFeatureStage = "stable"
	ExperimentalFeatureStageDeprecated       ExperimentalFeatureStage = "deprecated"
	ExperimentalFeatureStageRemoved          ExperimentalFeatureStage = "removed"
)

type ExternalAgentConfigDetectParams struct {
	Cwds              Optional[[]string]    `json:"cwds,omitzero"`
	IncludeHome       OptionalNonNull[bool] `json:"includeHome,omitzero"`
	MaxSessionAgeDays Optional[uint32]      `json:"maxSessionAgeDays,omitzero"`
	MaxSessions       Optional[uint32]      `json:"maxSessions,omitzero"`
	MigrationSource   Optional[string]      `json:"migrationSource,omitzero"`
	Source            Optional[string]      `json:"source,omitzero"`
}

var externalAgentConfigDetectParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwds"},
		{name: "includeHome", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "maxSessionAgeDays"},
		{name: "maxSessions"},
		{name: "migrationSource"},
		{name: "source"},
	},
}

func (v ExternalAgentConfigDetectParams) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigDetectParams
	w := wire(v)
	w.IncludeHome = nonNullWithoutFalse(w.IncludeHome)
	return json.Marshal(w)
}

func (v *ExternalAgentConfigDetectParams) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigDetectParams
	var s objectScan
	if _, err := s.begin(data, &externalAgentConfigDetectParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueMaxSessionAgeDays, ok := v.MaxSessionAgeDays.Value(); ok {
		if valueMaxSessionAgeDays < 0 {
			return DecodeError{Field: "maxSessionAgeDays", Reason: "below minimum 0"}
		}
	}
	if valueMaxSessions, ok := v.MaxSessions.Value(); ok {
		if valueMaxSessions < 0 {
			return DecodeError{Field: "maxSessions", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ExternalAgentConfigDetectResponse struct {
	Connectors OptionalNonNull[[]ExternalAgentDetectedConnectorCandidate] `json:"connectors,omitzero"`
	Items      []ExternalAgentConfigMigrationItem                         `json:"items"`
}

var externalAgentConfigDetectResponseSpec = objectSpec{
	fields: []objectField{
		{name: "connectors", flags: fieldNonNullOptional},
		{name: "items", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigDetectResponse) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigDetectResponse
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigDetectResponse) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigDetectResponse
	return decodeObject(data, &externalAgentConfigDetectResponseSpec, (*wire)(v))
}

type ExternalAgentConfigImportCompletedNotification struct {
	ImportID        string                                `json:"importId"`
	ItemTypeResults []ExternalAgentConfigImportTypeResult `json:"itemTypeResults"`
}

var externalAgentConfigImportCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "importId", flags: fieldRequired | fieldRejectNull},
		{name: "itemTypeResults", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportCompletedNotification
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportCompletedNotification
	return decodeObject(data, &externalAgentConfigImportCompletedNotificationSpec, (*wire)(v))
}

type ExternalAgentConfigImportHistoriesReadResponse struct {
	Connectors []ExternalAgentImportedConnectorCandidate `json:"connectors"`
	Data       []ExternalAgentConfigImportHistory        `json:"data"`
}

var externalAgentConfigImportHistoriesReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "connectors", flags: fieldRequired | fieldRejectNull},
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportHistoriesReadResponse) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportHistoriesReadResponse
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportHistoriesReadResponse) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportHistoriesReadResponse
	return decodeObject(data, &externalAgentConfigImportHistoriesReadResponseSpec, (*wire)(v))
}

type ExternalAgentConfigImportHistory struct {
	CompletedAtMs int64                                      `json:"completedAtMs"`
	Failures      []ExternalAgentConfigImportItemTypeFailure `json:"failures"`
	ImportID      string                                     `json:"importId"`
	ProviderID    Optional[string]                           `json:"providerId,omitzero"`
	Successes     []ExternalAgentConfigImportItemTypeSuccess `json:"successes"`
}

var externalAgentConfigImportHistorySpec = objectSpec{
	fields: []objectField{
		{name: "completedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "failures", flags: fieldRequired | fieldRejectNull},
		{name: "importId", flags: fieldRequired | fieldRejectNull},
		{name: "providerId"},
		{name: "successes", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportHistory) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportHistory
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportHistory) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportHistory
	return decodeObject(data, &externalAgentConfigImportHistorySpec, (*wire)(v))
}

type ExternalAgentConfigImportHistoryRecordParams struct {
	ItemTypeResults []ExternalAgentConfigImportHistoryRecordTypeResultParams `json:"itemTypeResults"`
	ProviderID      string                                                   `json:"providerId"`
}

var externalAgentConfigImportHistoryRecordParamsSpec = objectSpec{
	fields: []objectField{
		{name: "itemTypeResults", flags: fieldRequired | fieldRejectNull},
		{name: "providerId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportHistoryRecordParams) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportHistoryRecordParams
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportHistoryRecordParams) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportHistoryRecordParams
	return decodeObject(data, &externalAgentConfigImportHistoryRecordParamsSpec, (*wire)(v))
}

type ExternalAgentConfigImportHistoryRecordResponse struct {
	ImportID string `json:"importId"`
}

var externalAgentConfigImportHistoryRecordResponseSpec = objectSpec{
	fields: []objectField{
		{name: "importId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportHistoryRecordResponse) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportHistoryRecordResponse
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportHistoryRecordResponse) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportHistoryRecordResponse
	return decodeObject(data, &externalAgentConfigImportHistoryRecordResponseSpec, (*wire)(v))
}

type ExternalAgentConfigImportHistoryRecordSuccessParams struct {
	Cwd      Optional[string]                     `json:"cwd,omitzero"`
	ItemType ExternalAgentConfigMigrationItemType `json:"itemType"`
	Source   Optional[string]                     `json:"source,omitzero"`
	Target   Optional[string]                     `json:"target,omitzero"`
	Title    Optional[string]                     `json:"title,omitzero"`
}

var externalAgentConfigImportHistoryRecordSuccessParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "itemType", flags: fieldRequired | fieldRejectNull},
		{name: "source"},
		{name: "target"},
		{name: "title"},
	},
}

func (v ExternalAgentConfigImportHistoryRecordSuccessParams) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportHistoryRecordSuccessParams
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportHistoryRecordSuccessParams) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportHistoryRecordSuccessParams
	return decodeObject(data, &externalAgentConfigImportHistoryRecordSuccessParamsSpec, (*wire)(v))
}

type ExternalAgentConfigImportHistoryRecordTypeResultParams struct {
	Failures  []ExternalAgentConfigImportItemTypeFailure            `json:"failures"`
	ItemType  ExternalAgentConfigMigrationItemType                  `json:"itemType"`
	Successes []ExternalAgentConfigImportHistoryRecordSuccessParams `json:"successes"`
}

var externalAgentConfigImportHistoryRecordTypeResultParamsSpec = objectSpec{
	fields: []objectField{
		{name: "failures", flags: fieldRequired | fieldRejectNull},
		{name: "itemType", flags: fieldRequired | fieldRejectNull},
		{name: "successes", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportHistoryRecordTypeResultParams) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportHistoryRecordTypeResultParams
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportHistoryRecordTypeResultParams) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportHistoryRecordTypeResultParams
	return decodeObject(data, &externalAgentConfigImportHistoryRecordTypeResultParamsSpec, (*wire)(v))
}

type ExternalAgentConfigImportItemTypeFailure struct {
	Cwd          Optional[string]                     `json:"cwd,omitzero"`
	ErrorType    Optional[string]                     `json:"errorType,omitzero"`
	FailureStage string                               `json:"failureStage"`
	ItemType     ExternalAgentConfigMigrationItemType `json:"itemType"`
	Message      string                               `json:"message"`
	Source       Optional[string]                     `json:"source,omitzero"`
	SubErrorType Optional[string]                     `json:"subErrorType,omitzero"`
}

var externalAgentConfigImportItemTypeFailureSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "errorType"},
		{name: "failureStage", flags: fieldRequired | fieldRejectNull},
		{name: "itemType", flags: fieldRequired | fieldRejectNull},
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "source"},
		{name: "subErrorType"},
	},
}

func (v ExternalAgentConfigImportItemTypeFailure) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportItemTypeFailure
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportItemTypeFailure) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportItemTypeFailure
	return decodeObject(data, &externalAgentConfigImportItemTypeFailureSpec, (*wire)(v))
}

type ExternalAgentConfigImportItemTypeSuccess struct {
	Cwd      Optional[string]                     `json:"cwd,omitzero"`
	ItemType ExternalAgentConfigMigrationItemType `json:"itemType"`
	Source   Optional[string]                     `json:"source,omitzero"`
	Target   Optional[string]                     `json:"target,omitzero"`
	Title    Optional[string]                     `json:"title,omitzero"`
}

var externalAgentConfigImportItemTypeSuccessSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "itemType", flags: fieldRequired | fieldRejectNull},
		{name: "source"},
		{name: "target"},
		{name: "title"},
	},
}

func (v ExternalAgentConfigImportItemTypeSuccess) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportItemTypeSuccess
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportItemTypeSuccess) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportItemTypeSuccess
	return decodeObject(data, &externalAgentConfigImportItemTypeSuccessSpec, (*wire)(v))
}

type ExternalAgentConfigImportParams struct {
	MigrationItems  []ExternalAgentConfigMigrationItem `json:"migrationItems"`
	MigrationSource Optional[string]                   `json:"migrationSource,omitzero"`
	ProviderID      Optional[string]                   `json:"providerId,omitzero"`
	Source          Optional[string]                   `json:"source,omitzero"`
}

var externalAgentConfigImportParamsSpec = objectSpec{
	fields: []objectField{
		{name: "migrationItems", flags: fieldRequired | fieldRejectNull},
		{name: "migrationSource"},
		{name: "providerId"},
		{name: "source"},
	},
}

func (v ExternalAgentConfigImportParams) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportParams
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportParams) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportParams
	return decodeObject(data, &externalAgentConfigImportParamsSpec, (*wire)(v))
}

type ExternalAgentConfigImportProgressNotification struct {
	ImportID        string                                `json:"importId"`
	ItemTypeResults []ExternalAgentConfigImportTypeResult `json:"itemTypeResults"`
}

var externalAgentConfigImportProgressNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "importId", flags: fieldRequired | fieldRejectNull},
		{name: "itemTypeResults", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportProgressNotification) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportProgressNotification
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportProgressNotification) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportProgressNotification
	return decodeObject(data, &externalAgentConfigImportProgressNotificationSpec, (*wire)(v))
}

type ExternalAgentConfigImportResponse struct {
	ImportID string `json:"importId"`
}

var externalAgentConfigImportResponseSpec = objectSpec{
	fields: []objectField{
		{name: "importId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportResponse) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportResponse
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportResponse) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportResponse
	return decodeObject(data, &externalAgentConfigImportResponseSpec, (*wire)(v))
}

type ExternalAgentConfigImportTypeResult struct {
	Failures  []ExternalAgentConfigImportItemTypeFailure `json:"failures"`
	ItemType  ExternalAgentConfigMigrationItemType       `json:"itemType"`
	Successes []ExternalAgentConfigImportItemTypeSuccess `json:"successes"`
}

var externalAgentConfigImportTypeResultSpec = objectSpec{
	fields: []objectField{
		{name: "failures", flags: fieldRequired | fieldRejectNull},
		{name: "itemType", flags: fieldRequired | fieldRejectNull},
		{name: "successes", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigImportTypeResult) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigImportTypeResult
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigImportTypeResult) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigImportTypeResult
	return decodeObject(data, &externalAgentConfigImportTypeResultSpec, (*wire)(v))
}

type ExternalAgentConfigMigrationItem struct {
	Cwd         Optional[string]                     `json:"cwd,omitzero"`
	Description string                               `json:"description"`
	Details     Optional[MigrationDetails]           `json:"details,omitzero"`
	ItemType    ExternalAgentConfigMigrationItemType `json:"itemType"`
}

var externalAgentConfigMigrationItemSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "details"},
		{name: "itemType", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentConfigMigrationItem) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentConfigMigrationItem
	return json.Marshal(wire(v))
}

func (v *ExternalAgentConfigMigrationItem) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentConfigMigrationItem
	return decodeObject(data, &externalAgentConfigMigrationItemSpec, (*wire)(v))
}

type ExternalAgentConfigMigrationItemType string

const (
	ExternalAgentConfigMigrationItemTypeAgentsMd        ExternalAgentConfigMigrationItemType = "AGENTS_MD"
	ExternalAgentConfigMigrationItemTypeConfig          ExternalAgentConfigMigrationItemType = "CONFIG"
	ExternalAgentConfigMigrationItemTypeSkills          ExternalAgentConfigMigrationItemType = "SKILLS"
	ExternalAgentConfigMigrationItemTypePlugins         ExternalAgentConfigMigrationItemType = "PLUGINS"
	ExternalAgentConfigMigrationItemTypeMcpServerConfig ExternalAgentConfigMigrationItemType = "MCP_SERVER_CONFIG"
	ExternalAgentConfigMigrationItemTypeSubagents       ExternalAgentConfigMigrationItemType = "SUBAGENTS"
	ExternalAgentConfigMigrationItemTypeHooks           ExternalAgentConfigMigrationItemType = "HOOKS"
	ExternalAgentConfigMigrationItemTypeCommands        ExternalAgentConfigMigrationItemType = "COMMANDS"
	ExternalAgentConfigMigrationItemTypeMemory          ExternalAgentConfigMigrationItemType = "MEMORY"
	ExternalAgentConfigMigrationItemTypeSessions        ExternalAgentConfigMigrationItemType = "SESSIONS"
)

type ExternalAgentDetectedConnectorCandidate struct {
	Name         string                               `json:"name"`
	SessionCount uint32                               `json:"sessionCount"`
	Source       ExternalAgentDetectedConnectorSource `json:"source"`
}

var externalAgentDetectedConnectorCandidateSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "sessionCount", flags: fieldRequired | fieldRejectNull},
		{name: "source", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentDetectedConnectorCandidate) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentDetectedConnectorCandidate
	return json.Marshal(wire(v))
}

func (v *ExternalAgentDetectedConnectorCandidate) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentDetectedConnectorCandidate
	var s objectScan
	if _, err := s.begin(data, &externalAgentDetectedConnectorCandidateSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.SessionCount < 0 {
		return DecodeError{Field: "sessionCount", Reason: "below minimum 0"}
	}
	return nil
}

type ExternalAgentDetectedConnectorSource string

const (
	ExternalAgentDetectedConnectorSourceRemoteMcpServersConfig ExternalAgentDetectedConnectorSource = "remoteMcpServersConfig"
	ExternalAgentDetectedConnectorSourceSessionToolUse         ExternalAgentDetectedConnectorSource = "sessionToolUse"
)

type ExternalAgentImportedConnectorCandidate struct {
	Name         string                               `json:"name"`
	SessionCount uint32                               `json:"sessionCount"`
	Source       ExternalAgentImportedConnectorSource `json:"source"`
}

var externalAgentImportedConnectorCandidateSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "sessionCount", flags: fieldRequired | fieldRejectNull},
		{name: "source", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ExternalAgentImportedConnectorCandidate) MarshalJSON() ([]byte, error) {
	type wire ExternalAgentImportedConnectorCandidate
	return json.Marshal(wire(v))
}

func (v *ExternalAgentImportedConnectorCandidate) UnmarshalJSON(data []byte) error {
	type wire ExternalAgentImportedConnectorCandidate
	var s objectScan
	if _, err := s.begin(data, &externalAgentImportedConnectorCandidateSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.SessionCount < 0 {
		return DecodeError{Field: "sessionCount", Reason: "below minimum 0"}
	}
	return nil
}

type ExternalAgentImportedConnectorSource string

const (
	ExternalAgentImportedConnectorSourceRemoteMcpServersConfig ExternalAgentImportedConnectorSource = "remoteMcpServersConfig"
)

type FeedbackRequirements struct {
	Enabled Optional[bool] `json:"enabled,omitzero"`
}

var feedbackRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "enabled"},
	},
}

func (v FeedbackRequirements) MarshalJSON() ([]byte, error) {
	type wire FeedbackRequirements
	return json.Marshal(wire(v))
}

func (v *FeedbackRequirements) UnmarshalJSON(data []byte) error {
	type wire FeedbackRequirements
	return decodeObject(data, &feedbackRequirementsSpec, (*wire)(v))
}

type FeedbackSaveParams struct {
	Classification string                      `json:"classification"`
	ExtraLogFiles  Optional[[]string]          `json:"extraLogFiles,omitzero"`
	IncludeLogs    OptionalNonNull[bool]       `json:"includeLogs,omitzero"`
	Reason         Optional[string]            `json:"reason,omitzero"`
	SavePath       LegacyAppPathString         `json:"savePath"`
	Tags           Optional[map[string]string] `json:"tags,omitzero"`
	ThreadID       Optional[string]            `json:"threadId,omitzero"`
}

var feedbackSaveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "classification", flags: fieldRequired | fieldRejectNull},
		{name: "extraLogFiles"},
		{name: "includeLogs", flags: fieldNonNullOptional},
		{name: "reason"},
		{name: "savePath", flags: fieldRequired | fieldRejectNull},
		{name: "tags"},
		{name: "threadId"},
	},
}

func (v FeedbackSaveParams) MarshalJSON() ([]byte, error) {
	type wire FeedbackSaveParams
	return json.Marshal(wire(v))
}

func (v *FeedbackSaveParams) UnmarshalJSON(data []byte) error {
	type wire FeedbackSaveParams
	return decodeObject(data, &feedbackSaveParamsSpec, (*wire)(v))
}

type FeedbackSaveResponse struct {
	PromptHash Optional[string]    `json:"promptHash,omitzero"`
	SavedPath  LegacyAppPathString `json:"savedPath"`
	ThreadID   string              `json:"threadId"`
}

var feedbackSaveResponseSpec = objectSpec{
	fields: []objectField{
		{name: "promptHash"},
		{name: "savedPath", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FeedbackSaveResponse) MarshalJSON() ([]byte, error) {
	type wire FeedbackSaveResponse
	return json.Marshal(wire(v))
}

func (v *FeedbackSaveResponse) UnmarshalJSON(data []byte) error {
	type wire FeedbackSaveResponse
	return decodeObject(data, &feedbackSaveResponseSpec, (*wire)(v))
}

type FeedbackUploadParams struct {
	Classification string                      `json:"classification"`
	ExtraLogFiles  Optional[[]string]          `json:"extraLogFiles,omitzero"`
	IncludeLogs    OptionalNonNull[bool]       `json:"includeLogs,omitzero"`
	Reason         Optional[string]            `json:"reason,omitzero"`
	Tags           Optional[map[string]string] `json:"tags,omitzero"`
	ThreadID       Optional[string]            `json:"threadId,omitzero"`
}

var feedbackUploadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "classification", flags: fieldRequired | fieldRejectNull},
		{name: "extraLogFiles"},
		{name: "includeLogs", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "reason"},
		{name: "tags"},
		{name: "threadId"},
	},
}

func (v FeedbackUploadParams) MarshalJSON() ([]byte, error) {
	type wire FeedbackUploadParams
	w := wire(v)
	w.IncludeLogs = nonNullWithoutFalse(w.IncludeLogs)
	return json.Marshal(w)
}

func (v *FeedbackUploadParams) UnmarshalJSON(data []byte) error {
	type wire FeedbackUploadParams
	return decodeObject(data, &feedbackUploadParamsSpec, (*wire)(v))
}

type FeedbackUploadResponse struct {
	PromptHash Optional[string] `json:"promptHash,omitzero"`
	ThreadID   string           `json:"threadId"`
}

var feedbackUploadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "promptHash", defaultJSON: "null"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FeedbackUploadResponse) MarshalJSON() ([]byte, error) {
	type wire FeedbackUploadResponse
	return json.Marshal(wire(v))
}

func (v *FeedbackUploadResponse) UnmarshalJSON(data []byte) error {
	type wire FeedbackUploadResponse
	return decodeObject(data, &feedbackUploadResponseSpec, (*wire)(v))
}

type FileChangeOutputDeltaNotification struct {
	Delta    string `json:"delta"`
	ItemID   string `json:"itemId"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var fileChangeOutputDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FileChangeOutputDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire FileChangeOutputDeltaNotification
	return json.Marshal(wire(v))
}

func (v *FileChangeOutputDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire FileChangeOutputDeltaNotification
	return decodeObject(data, &fileChangeOutputDeltaNotificationSpec, (*wire)(v))
}

type FileChangePatchUpdatedNotification struct {
	Changes  []FileUpdateChange `json:"changes"`
	ItemID   string             `json:"itemId"`
	ThreadID string             `json:"threadId"`
	TurnID   string             `json:"turnId"`
}

var fileChangePatchUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "changes", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FileChangePatchUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire FileChangePatchUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *FileChangePatchUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire FileChangePatchUpdatedNotification
	return decodeObject(data, &fileChangePatchUpdatedNotificationSpec, (*wire)(v))
}

type FileSystemAccessMode string

const (
	FileSystemAccessModeRead  FileSystemAccessMode = "read"
	FileSystemAccessModeWrite FileSystemAccessMode = "write"
	FileSystemAccessModeDeny  FileSystemAccessMode = "deny"
)

type FileSystemPath struct {
	Path      OptionalNonNull[LegacyAppPathString]   `json:"path,omitzero"`
	Pattern   OptionalNonNull[string]                `json:"pattern,omitzero"`
	TypeValue string                                 `json:"type"`
	Value     OptionalNonNull[FileSystemSpecialPath] `json:"value,omitzero"`
	RawJSON   json.RawMessage                        `json:"-"`
}

var fileSystemPathSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldNonNullOptional},
		{name: "pattern", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "value", flags: fieldNonNullOptional},
	},
	tag:  2,
	tags: []string{"path", "glob_pattern", "special"},
}

func (v FileSystemPath) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "path":
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type path"}
		}
	case "glob_pattern":
		if !v.Pattern.IsSet() {
			return nil, DecodeError{Field: "pattern", Reason: "missing required field for type glob_pattern"}
		}
	case "special":
		if !v.Value.IsSet() {
			return nil, DecodeError{Field: "value", Reason: "missing required field for type special"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire FileSystemPath
	return json.Marshal(wire(v))
}

func (v *FileSystemPath) UnmarshalJSON(data []byte) error {
	type wire FileSystemPath
	var s objectScan
	known, err := s.begin(data, &fileSystemPathSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "path":
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type path"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "glob_pattern":
		if !s.has("pattern") {
			return DecodeError{Field: "pattern", Reason: "missing required field for type glob_pattern"}
		}
		if s.isNull("pattern") {
			return DecodeError{Field: "pattern", Reason: "cannot be null"}
		}
	case "special":
		if !s.has("value") {
			return DecodeError{Field: "value", Reason: "missing required field for type special"}
		}
		if s.isNull("value") {
			return DecodeError{Field: "value", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type FileSystemSandboxEntry struct {
	Access FileSystemAccessMode `json:"access"`
	Path   FileSystemPath       `json:"path"`
}

var fileSystemSandboxEntrySpec = objectSpec{
	fields: []objectField{
		{name: "access", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FileSystemSandboxEntry) MarshalJSON() ([]byte, error) {
	type wire FileSystemSandboxEntry
	return json.Marshal(wire(v))
}

func (v *FileSystemSandboxEntry) UnmarshalJSON(data []byte) error {
	type wire FileSystemSandboxEntry
	return decodeObject(data, &fileSystemSandboxEntrySpec, (*wire)(v))
}

type FileSystemSpecialPath struct {
	Kind    string                        `json:"kind"`
	Path    OptionalNonNull[string]       `json:"path,omitzero"`
	Subpath Optional[LegacyAppPathString] `json:"subpath,omitzero"`
	RawJSON json.RawMessage               `json:"-"`
}

var fileSystemSpecialPathSpec = objectSpec{
	fields: []objectField{
		{name: "kind", flags: fieldRequired | fieldRejectNull, variantAliases: []variantAlias{{alias: "\"current_working_directory\"", canonical: "\"project_roots\""}}},
		{name: "path", flags: fieldNonNullOptional},
		{name: "subpath"},
	},
	tag:  0,
	tags: []string{"root", "minimal", "project_roots", "tmpdir", "slash_tmp", "unknown"},
}

func (v FileSystemSpecialPath) MarshalJSON() ([]byte, error) {
	switch v.Kind {
	case "root":
	case "minimal":
	case "project_roots":
	case "tmpdir":
	case "slash_tmp":
	case "unknown":
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for kind unknown"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "kind", Reason: fmt.Sprintf("unsupported discriminator value %q", v.Kind)}
	}
	type wire FileSystemSpecialPath
	return json.Marshal(wire(v))
}

func (v *FileSystemSpecialPath) UnmarshalJSON(data []byte) error {
	type wire FileSystemSpecialPath
	var s objectScan
	known, err := s.begin(data, &fileSystemSpecialPathSpec)
	if err != nil {
		return err
	}
	if !known {
		v.Kind = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.Kind {
	case "root":
	case "minimal":
	case "project_roots":
	case "tmpdir":
	case "slash_tmp":
	case "unknown":
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for kind unknown"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type FileUpdateChange struct {
	Diff string          `json:"diff"`
	Kind PatchChangeKind `json:"kind"`
	Path string          `json:"path"`
}

var fileUpdateChangeSpec = objectSpec{
	fields: []objectField{
		{name: "diff", flags: fieldRequired | fieldRejectNull},
		{name: "kind", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FileUpdateChange) MarshalJSON() ([]byte, error) {
	type wire FileUpdateChange
	return json.Marshal(wire(v))
}

func (v *FileUpdateChange) UnmarshalJSON(data []byte) error {
	type wire FileUpdateChange
	return decodeObject(data, &fileUpdateChangeSpec, (*wire)(v))
}

type ForcedChatgptWorkspaceIDs = json.RawMessage

type ForcedLoginMethod string

const (
	ForcedLoginMethodChatgpt ForcedLoginMethod = "chatgpt"
	ForcedLoginMethodApi     ForcedLoginMethod = "api"
)

type FsChangedNotification struct {
	ChangedPaths []AbsolutePathBuf `json:"changedPaths"`
	WatchID      string            `json:"watchId"`
}

var fsChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "changedPaths", flags: fieldRequired | fieldRejectNull},
		{name: "watchId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsChangedNotification) MarshalJSON() ([]byte, error) {
	type wire FsChangedNotification
	return json.Marshal(wire(v))
}

func (v *FsChangedNotification) UnmarshalJSON(data []byte) error {
	type wire FsChangedNotification
	return decodeObject(data, &fsChangedNotificationSpec, (*wire)(v))
}

type FsCopyParams struct {
	DestinationPath AbsolutePathBuf       `json:"destinationPath"`
	Recursive       OptionalNonNull[bool] `json:"recursive,omitzero"`
	SourcePath      AbsolutePathBuf       `json:"sourcePath"`
}

var fsCopyParamsSpec = objectSpec{
	fields: []objectField{
		{name: "destinationPath", flags: fieldRequired | fieldRejectNull},
		{name: "recursive", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "sourcePath", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsCopyParams) MarshalJSON() ([]byte, error) {
	type wire FsCopyParams
	w := wire(v)
	w.Recursive = nonNullWithoutFalse(w.Recursive)
	return json.Marshal(w)
}

func (v *FsCopyParams) UnmarshalJSON(data []byte) error {
	type wire FsCopyParams
	return decodeObject(data, &fsCopyParamsSpec, (*wire)(v))
}

type FsCopyResponse map[string]json.RawMessage

func (v FsCopyResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FsCopyResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FsCopyResponse(raw)
	return nil
}

type FsCreateDirectoryParams struct {
	Path      AbsolutePathBuf `json:"path"`
	Recursive Optional[bool]  `json:"recursive,omitzero"`
}

var fsCreateDirectoryParamsSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "recursive"},
	},
}

func (v FsCreateDirectoryParams) MarshalJSON() ([]byte, error) {
	type wire FsCreateDirectoryParams
	return json.Marshal(wire(v))
}

func (v *FsCreateDirectoryParams) UnmarshalJSON(data []byte) error {
	type wire FsCreateDirectoryParams
	return decodeObject(data, &fsCreateDirectoryParamsSpec, (*wire)(v))
}

type FsCreateDirectoryResponse map[string]json.RawMessage

func (v FsCreateDirectoryResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FsCreateDirectoryResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FsCreateDirectoryResponse(raw)
	return nil
}

type FsGetMetadataParams struct {
	Path AbsolutePathBuf `json:"path"`
}

var fsGetMetadataParamsSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsGetMetadataParams) MarshalJSON() ([]byte, error) {
	type wire FsGetMetadataParams
	return json.Marshal(wire(v))
}

func (v *FsGetMetadataParams) UnmarshalJSON(data []byte) error {
	type wire FsGetMetadataParams
	return decodeObject(data, &fsGetMetadataParamsSpec, (*wire)(v))
}

type FsGetMetadataResponse struct {
	CreatedAtMs  int64 `json:"createdAtMs"`
	IsDirectory  bool  `json:"isDirectory"`
	IsFile       bool  `json:"isFile"`
	IsSymlink    bool  `json:"isSymlink"`
	ModifiedAtMs int64 `json:"modifiedAtMs"`
}

var fsGetMetadataResponseSpec = objectSpec{
	fields: []objectField{
		{name: "createdAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "isDirectory", flags: fieldRequired | fieldRejectNull},
		{name: "isFile", flags: fieldRequired | fieldRejectNull},
		{name: "isSymlink", flags: fieldRequired | fieldRejectNull},
		{name: "modifiedAtMs", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsGetMetadataResponse) MarshalJSON() ([]byte, error) {
	type wire FsGetMetadataResponse
	return json.Marshal(wire(v))
}

func (v *FsGetMetadataResponse) UnmarshalJSON(data []byte) error {
	type wire FsGetMetadataResponse
	return decodeObject(data, &fsGetMetadataResponseSpec, (*wire)(v))
}

type FsReadDirectoryEntry struct {
	FileName    string `json:"fileName"`
	IsDirectory bool   `json:"isDirectory"`
	IsFile      bool   `json:"isFile"`
}

var fsReadDirectoryEntrySpec = objectSpec{
	fields: []objectField{
		{name: "fileName", flags: fieldRequired | fieldRejectNull},
		{name: "isDirectory", flags: fieldRequired | fieldRejectNull},
		{name: "isFile", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsReadDirectoryEntry) MarshalJSON() ([]byte, error) {
	type wire FsReadDirectoryEntry
	return json.Marshal(wire(v))
}

func (v *FsReadDirectoryEntry) UnmarshalJSON(data []byte) error {
	type wire FsReadDirectoryEntry
	return decodeObject(data, &fsReadDirectoryEntrySpec, (*wire)(v))
}

type FsReadDirectoryParams struct {
	Path AbsolutePathBuf `json:"path"`
}

var fsReadDirectoryParamsSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsReadDirectoryParams) MarshalJSON() ([]byte, error) {
	type wire FsReadDirectoryParams
	return json.Marshal(wire(v))
}

func (v *FsReadDirectoryParams) UnmarshalJSON(data []byte) error {
	type wire FsReadDirectoryParams
	return decodeObject(data, &fsReadDirectoryParamsSpec, (*wire)(v))
}

type FsReadDirectoryResponse struct {
	Entries []FsReadDirectoryEntry `json:"entries"`
}

var fsReadDirectoryResponseSpec = objectSpec{
	fields: []objectField{
		{name: "entries", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsReadDirectoryResponse) MarshalJSON() ([]byte, error) {
	type wire FsReadDirectoryResponse
	return json.Marshal(wire(v))
}

func (v *FsReadDirectoryResponse) UnmarshalJSON(data []byte) error {
	type wire FsReadDirectoryResponse
	return decodeObject(data, &fsReadDirectoryResponseSpec, (*wire)(v))
}

type FsReadFileParams struct {
	Path AbsolutePathBuf `json:"path"`
}

var fsReadFileParamsSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsReadFileParams) MarshalJSON() ([]byte, error) {
	type wire FsReadFileParams
	return json.Marshal(wire(v))
}

func (v *FsReadFileParams) UnmarshalJSON(data []byte) error {
	type wire FsReadFileParams
	return decodeObject(data, &fsReadFileParamsSpec, (*wire)(v))
}

type FsReadFileResponse struct {
	DataBase64 string `json:"dataBase64"`
}

var fsReadFileResponseSpec = objectSpec{
	fields: []objectField{
		{name: "dataBase64", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsReadFileResponse) MarshalJSON() ([]byte, error) {
	type wire FsReadFileResponse
	return json.Marshal(wire(v))
}

func (v *FsReadFileResponse) UnmarshalJSON(data []byte) error {
	type wire FsReadFileResponse
	return decodeObject(data, &fsReadFileResponseSpec, (*wire)(v))
}

type FsRemoveParams struct {
	Force     Optional[bool]  `json:"force,omitzero"`
	Path      AbsolutePathBuf `json:"path"`
	Recursive Optional[bool]  `json:"recursive,omitzero"`
}

var fsRemoveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "force"},
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "recursive"},
	},
}

func (v FsRemoveParams) MarshalJSON() ([]byte, error) {
	type wire FsRemoveParams
	return json.Marshal(wire(v))
}

func (v *FsRemoveParams) UnmarshalJSON(data []byte) error {
	type wire FsRemoveParams
	return decodeObject(data, &fsRemoveParamsSpec, (*wire)(v))
}

type FsRemoveResponse map[string]json.RawMessage

func (v FsRemoveResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FsRemoveResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FsRemoveResponse(raw)
	return nil
}

type FsUnwatchParams struct {
	WatchID string `json:"watchId"`
}

var fsUnwatchParamsSpec = objectSpec{
	fields: []objectField{
		{name: "watchId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsUnwatchParams) MarshalJSON() ([]byte, error) {
	type wire FsUnwatchParams
	return json.Marshal(wire(v))
}

func (v *FsUnwatchParams) UnmarshalJSON(data []byte) error {
	type wire FsUnwatchParams
	return decodeObject(data, &fsUnwatchParamsSpec, (*wire)(v))
}

type FsUnwatchResponse map[string]json.RawMessage

func (v FsUnwatchResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FsUnwatchResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FsUnwatchResponse(raw)
	return nil
}

type FsWatchParams struct {
	Path    AbsolutePathBuf `json:"path"`
	WatchID string          `json:"watchId"`
}

var fsWatchParamsSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "watchId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsWatchParams) MarshalJSON() ([]byte, error) {
	type wire FsWatchParams
	return json.Marshal(wire(v))
}

func (v *FsWatchParams) UnmarshalJSON(data []byte) error {
	type wire FsWatchParams
	return decodeObject(data, &fsWatchParamsSpec, (*wire)(v))
}

type FsWatchResponse struct {
	Path AbsolutePathBuf `json:"path"`
}

var fsWatchResponseSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsWatchResponse) MarshalJSON() ([]byte, error) {
	type wire FsWatchResponse
	return json.Marshal(wire(v))
}

func (v *FsWatchResponse) UnmarshalJSON(data []byte) error {
	type wire FsWatchResponse
	return decodeObject(data, &fsWatchResponseSpec, (*wire)(v))
}

type FsWriteFileParams struct {
	DataBase64 string          `json:"dataBase64"`
	Path       AbsolutePathBuf `json:"path"`
}

var fsWriteFileParamsSpec = objectSpec{
	fields: []objectField{
		{name: "dataBase64", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v FsWriteFileParams) MarshalJSON() ([]byte, error) {
	type wire FsWriteFileParams
	return json.Marshal(wire(v))
}

func (v *FsWriteFileParams) UnmarshalJSON(data []byte) error {
	type wire FsWriteFileParams
	return decodeObject(data, &fsWriteFileParamsSpec, (*wire)(v))
}

type FsWriteFileResponse map[string]json.RawMessage

func (v FsWriteFileResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *FsWriteFileResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = FsWriteFileResponse(raw)
	return nil
}

type FunctionCallOutputBody = json.RawMessage

type FunctionCallOutputContentItem struct {
	AudioURL         OptionalNonNull[string] `json:"audio_url,omitzero"`
	Detail           Optional[ImageDetail]   `json:"detail,omitzero"`
	EncryptedContent OptionalNonNull[string] `json:"encrypted_content,omitzero"`
	FileID           OptionalNonNull[string] `json:"file_id,omitzero"`
	ImageURL         OptionalNonNull[string] `json:"image_url,omitzero"`
	Text             OptionalNonNull[string] `json:"text,omitzero"`
	TypeValue        string                  `json:"type"`
	RawJSON          json.RawMessage         `json:"-"`
}

var functionCallOutputContentItemSpec = objectSpec{
	fields: []objectField{
		{name: "audio_url", flags: fieldNonNullOptional},
		{name: "detail"},
		{name: "encrypted_content", flags: fieldNonNullOptional},
		{name: "file_id", flags: fieldNonNullOptional, anyOfTags: []string{"\"input_image\""}},
		{name: "image_url", flags: fieldNonNullOptional, anyOfTags: []string{"\"input_image\""}},
		{name: "text", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  6,
	tags: []string{"input_text", "input_image", "input_audio", "encrypted_content"},
}

func (v FunctionCallOutputContentItem) MarshalJSON() ([]byte, error) {
	type wire FunctionCallOutputContentItem
	w := wire(v)
	switch v.TypeValue {
	case "input_text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type input_text"}
		}
	case "input_image":
		taggedAnyOfVariant1Matches := (w.ImageURL.IsSet() && marshalsNonNull(w.ImageURL)) || (w.FileID.IsSet() && marshalsNonNull(w.FileID))
		if !taggedAnyOfVariant1Matches {
			return nil, DecodeError{Field: "", Reason: "does not match any anyOf variant for type input_image"}
		}
	case "input_audio":
		if !v.AudioURL.IsSet() {
			return nil, DecodeError{Field: "audio_url", Reason: "missing required field for type input_audio"}
		}
	case "encrypted_content":
		if !v.EncryptedContent.IsSet() {
			return nil, DecodeError{Field: "encrypted_content", Reason: "missing required field for type encrypted_content"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	return json.Marshal(w)
}

func (v *FunctionCallOutputContentItem) UnmarshalJSON(data []byte) error {
	type wire FunctionCallOutputContentItem
	var s objectScan
	known, err := s.begin(data, &functionCallOutputContentItemSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "input_text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type input_text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "input_image":
		taggedAnyOfVariant1Matches := (v.ImageURL.IsSet()) || (v.FileID.IsSet())
		if !taggedAnyOfVariant1Matches {
			return DecodeError{Field: "", Reason: "does not match any anyOf variant for type input_image"}
		}
	case "input_audio":
		if !s.has("audio_url") {
			return DecodeError{Field: "audio_url", Reason: "missing required field for type input_audio"}
		}
		if s.isNull("audio_url") {
			return DecodeError{Field: "audio_url", Reason: "cannot be null"}
		}
	case "encrypted_content":
		if !s.has("encrypted_content") {
			return DecodeError{Field: "encrypted_content", Reason: "missing required field for type encrypted_content"}
		}
		if s.isNull("encrypted_content") {
			return DecodeError{Field: "encrypted_content", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type GatewayOAuthCancelResponse map[string]json.RawMessage

func (v GatewayOAuthCancelResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *GatewayOAuthCancelResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = GatewayOAuthCancelResponse(raw)
	return nil
}

type GatewayOAuthChangedNotification struct {
	AuthURL    Optional[string]   `json:"authUrl,omitzero"`
	Error      Optional[string]   `json:"error,omitzero"`
	ProviderID string             `json:"providerId"`
	Status     GatewayOAuthStatus `json:"status"`
}

var gatewayOAuthChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "authUrl"},
		{name: "error"},
		{name: "providerId", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v GatewayOAuthChangedNotification) MarshalJSON() ([]byte, error) {
	type wire GatewayOAuthChangedNotification
	return json.Marshal(wire(v))
}

func (v *GatewayOAuthChangedNotification) UnmarshalJSON(data []byte) error {
	type wire GatewayOAuthChangedNotification
	return decodeObject(data, &gatewayOAuthChangedNotificationSpec, (*wire)(v))
}

type GatewayOAuthLoginResponse map[string]json.RawMessage

func (v GatewayOAuthLoginResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *GatewayOAuthLoginResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = GatewayOAuthLoginResponse(raw)
	return nil
}

type GatewayOAuthReadResponse struct {
	Error        Optional[string]             `json:"error,omitzero"`
	ProviderID   string                       `json:"providerId"`
	ProviderName string                       `json:"providerName"`
	Required     bool                         `json:"required"`
	Status       Optional[GatewayOAuthStatus] `json:"status,omitzero"`
}

var gatewayOAuthReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "error"},
		{name: "providerId", flags: fieldRequired | fieldRejectNull},
		{name: "providerName", flags: fieldRequired | fieldRejectNull},
		{name: "required", flags: fieldRequired | fieldRejectNull},
		{name: "status"},
	},
}

func (v GatewayOAuthReadResponse) MarshalJSON() ([]byte, error) {
	type wire GatewayOAuthReadResponse
	return json.Marshal(wire(v))
}

func (v *GatewayOAuthReadResponse) UnmarshalJSON(data []byte) error {
	type wire GatewayOAuthReadResponse
	return decodeObject(data, &gatewayOAuthReadResponseSpec, (*wire)(v))
}

type GatewayOAuthStatus string

const (
	GatewayOAuthStatusNotReady  GatewayOAuthStatus = "notReady"
	GatewayOAuthStatusStarted   GatewayOAuthStatus = "started"
	GatewayOAuthStatusSucceeded GatewayOAuthStatus = "succeeded"
	GatewayOAuthStatusFailed    GatewayOAuthStatus = "failed"
)

type GcpSetupParams struct {
	CredentialFile Optional[string] `json:"credentialFile,omitzero"`
	Location       Optional[string] `json:"location,omitzero"`
	ProjectID      string           `json:"projectId"`
}

var gcpSetupParamsSpec = objectSpec{
	fields: []objectField{
		{name: "credentialFile"},
		{name: "location"},
		{name: "projectId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v GcpSetupParams) MarshalJSON() ([]byte, error) {
	type wire GcpSetupParams
	return json.Marshal(wire(v))
}

func (v *GcpSetupParams) UnmarshalJSON(data []byte) error {
	type wire GcpSetupParams
	return decodeObject(data, &gcpSetupParamsSpec, (*wire)(v))
}

type GcpSetupResponse map[string]json.RawMessage

func (v GcpSetupResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *GcpSetupResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = GcpSetupResponse(raw)
	return nil
}

type GetAccountParams struct {
	RefreshToken OptionalNonNull[bool] `json:"refreshToken,omitzero"`
}

var getAccountParamsSpec = objectSpec{
	fields: []objectField{
		{name: "refreshToken", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v GetAccountParams) MarshalJSON() ([]byte, error) {
	type wire GetAccountParams
	w := wire(v)
	w.RefreshToken = nonNullWithoutFalse(w.RefreshToken)
	return json.Marshal(w)
}

func (v *GetAccountParams) UnmarshalJSON(data []byte) error {
	type wire GetAccountParams
	return decodeObject(data, &getAccountParamsSpec, (*wire)(v))
}

type GetAccountRateLimitsParams struct {
	ExcludeResetCreditDetails OptionalNonNull[bool] `json:"excludeResetCreditDetails,omitzero"`
	SupportsLunaReserve       OptionalNonNull[bool] `json:"supportsLunaReserve,omitzero"`
}

var getAccountRateLimitsParamsSpec = objectSpec{
	fields: []objectField{
		{name: "excludeResetCreditDetails", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "supportsLunaReserve", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v GetAccountRateLimitsParams) MarshalJSON() ([]byte, error) {
	type wire GetAccountRateLimitsParams
	w := wire(v)
	w.ExcludeResetCreditDetails = nonNullWithoutFalse(w.ExcludeResetCreditDetails)
	w.SupportsLunaReserve = nonNullWithoutFalse(w.SupportsLunaReserve)
	return json.Marshal(w)
}

func (v *GetAccountRateLimitsParams) UnmarshalJSON(data []byte) error {
	type wire GetAccountRateLimitsParams
	return decodeObject(data, &getAccountRateLimitsParamsSpec, (*wire)(v))
}

type GetAccountRateLimitsResponse struct {
	AccountID             Optional[string]                       `json:"accountId,omitzero"`
	OrdinaryUsageAllowed  Optional[bool]                         `json:"ordinaryUsageAllowed,omitzero"`
	RateLimitResetCredits Optional[RateLimitResetCreditsSummary] `json:"rateLimitResetCredits,omitzero"`
	RateLimitUpsell       json.RawMessage                        `json:"rateLimitUpsell,omitempty"`
	RateLimits            RateLimitSnapshot                      `json:"rateLimits"`
	RateLimitsByLimitID   Optional[map[string]RateLimitSnapshot] `json:"rateLimitsByLimitId,omitzero"`
}

var getAccountRateLimitsResponseSpec = objectSpec{
	fields: []objectField{
		{name: "accountId"},
		{name: "ordinaryUsageAllowed"},
		{name: "rateLimitResetCredits"},
		{name: "rateLimitUpsell"},
		{name: "rateLimits", flags: fieldRequired | fieldRejectNull},
		{name: "rateLimitsByLimitId"},
	},
}

func (v GetAccountRateLimitsResponse) MarshalJSON() ([]byte, error) {
	type wire GetAccountRateLimitsResponse
	return json.Marshal(wire(v))
}

func (v *GetAccountRateLimitsResponse) UnmarshalJSON(data []byte) error {
	type wire GetAccountRateLimitsResponse
	return decodeObject(data, &getAccountRateLimitsResponseSpec, (*wire)(v))
}

type GetAccountResponse struct {
	Account            Optional[Account]          `json:"account,omitzero"`
	RequiresOpenaiAuth bool                       `json:"requiresOpenaiAuth"`
	WorkspaceRouting   Optional[WorkspaceRouting] `json:"workspaceRouting,omitzero"`
}

var getAccountResponseSpec = objectSpec{
	fields: []objectField{
		{name: "account"},
		{name: "requiresOpenaiAuth", flags: fieldRequired | fieldRejectNull},
		{name: "workspaceRouting"},
	},
}

func (v GetAccountResponse) MarshalJSON() ([]byte, error) {
	type wire GetAccountResponse
	return json.Marshal(wire(v))
}

func (v *GetAccountResponse) UnmarshalJSON(data []byte) error {
	type wire GetAccountResponse
	return decodeObject(data, &getAccountResponseSpec, (*wire)(v))
}

type GetAccountTokenUsageParams struct {
	ThreadID Optional[string] `json:"threadId,omitzero"`
}

var getAccountTokenUsageParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId"},
	},
}

func (v GetAccountTokenUsageParams) MarshalJSON() ([]byte, error) {
	type wire GetAccountTokenUsageParams
	return json.Marshal(wire(v))
}

func (v *GetAccountTokenUsageParams) UnmarshalJSON(data []byte) error {
	type wire GetAccountTokenUsageParams
	return decodeObject(data, &getAccountTokenUsageParamsSpec, (*wire)(v))
}

type GetAccountTokenUsageResponse struct {
	DailyUsageBuckets Optional[[]AccountTokenUsageDailyBucket] `json:"dailyUsageBuckets,omitzero"`
	Summary           AccountTokenUsageSummary                 `json:"summary"`
	ThreadUsage       Optional[ThreadUsage]                    `json:"threadUsage,omitzero"`
}

var getAccountTokenUsageResponseSpec = objectSpec{
	fields: []objectField{
		{name: "dailyUsageBuckets"},
		{name: "summary", flags: fieldRequired | fieldRejectNull},
		{name: "threadUsage"},
	},
}

func (v GetAccountTokenUsageResponse) MarshalJSON() ([]byte, error) {
	type wire GetAccountTokenUsageResponse
	return json.Marshal(wire(v))
}

func (v *GetAccountTokenUsageResponse) UnmarshalJSON(data []byte) error {
	type wire GetAccountTokenUsageResponse
	return decodeObject(data, &getAccountTokenUsageResponseSpec, (*wire)(v))
}

type GetWorkspaceMessagesResponse struct {
	FeatureEnabled bool               `json:"featureEnabled"`
	Messages       []WorkspaceMessage `json:"messages"`
}

var getWorkspaceMessagesResponseSpec = objectSpec{
	fields: []objectField{
		{name: "featureEnabled", flags: fieldRequired | fieldRejectNull},
		{name: "messages", flags: fieldRequired | fieldRejectNull},
	},
}

func (v GetWorkspaceMessagesResponse) MarshalJSON() ([]byte, error) {
	type wire GetWorkspaceMessagesResponse
	return json.Marshal(wire(v))
}

func (v *GetWorkspaceMessagesResponse) UnmarshalJSON(data []byte) error {
	type wire GetWorkspaceMessagesResponse
	return decodeObject(data, &getWorkspaceMessagesResponseSpec, (*wire)(v))
}

type GitInfo struct {
	Branch    Optional[string] `json:"branch,omitzero"`
	OriginURL Optional[string] `json:"originUrl,omitzero"`
	Sha       Optional[string] `json:"sha,omitzero"`
}

var gitInfoSpec = objectSpec{
	fields: []objectField{
		{name: "branch"},
		{name: "originUrl"},
		{name: "sha"},
	},
}

func (v GitInfo) MarshalJSON() ([]byte, error) {
	type wire GitInfo
	return json.Marshal(wire(v))
}

func (v *GitInfo) UnmarshalJSON(data []byte) error {
	type wire GitInfo
	return decodeObject(data, &gitInfoSpec, (*wire)(v))
}

type GuardianApprovalReview struct {
	Rationale         Optional[string]                    `json:"rationale,omitzero"`
	RiskLevel         Optional[GuardianRiskLevel]         `json:"riskLevel,omitzero"`
	Status            GuardianApprovalReviewStatus        `json:"status"`
	UserAuthorization Optional[GuardianUserAuthorization] `json:"userAuthorization,omitzero"`
}

var guardianApprovalReviewSpec = objectSpec{
	fields: []objectField{
		{name: "rationale"},
		{name: "riskLevel"},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "userAuthorization"},
	},
}

func (v GuardianApprovalReview) MarshalJSON() ([]byte, error) {
	type wire GuardianApprovalReview
	return json.Marshal(wire(v))
}

func (v *GuardianApprovalReview) UnmarshalJSON(data []byte) error {
	type wire GuardianApprovalReview
	return decodeObject(data, &guardianApprovalReviewSpec, (*wire)(v))
}

type GuardianApprovalReviewAction struct {
	ApprovalID    OptionalNonNull[string]                   `json:"approvalId,omitzero"`
	Argv          OptionalNonNull[[]string]                 `json:"argv,omitzero"`
	Command       OptionalNonNull[string]                   `json:"command,omitzero"`
	ConnectorID   Optional[string]                          `json:"connectorId,omitzero"`
	ConnectorName Optional[string]                          `json:"connectorName,omitzero"`
	Cwd           OptionalNonNull[LegacyAppPathString]      `json:"cwd,omitzero"`
	Files         OptionalNonNull[[]LegacyAppPathString]    `json:"files,omitzero"`
	Host          OptionalNonNull[string]                   `json:"host,omitzero"`
	Permissions   OptionalNonNull[RequestPermissionProfile] `json:"permissions,omitzero"`
	Port          OptionalNonNull[uint16]                   `json:"port,omitzero"`
	ProcessID     OptionalNonNull[string]                   `json:"processId,omitzero"`
	Program       OptionalNonNull[string]                   `json:"program,omitzero"`
	Protocol      OptionalNonNull[NetworkApprovalProtocol]  `json:"protocol,omitzero"`
	Reason        Optional[string]                          `json:"reason,omitzero"`
	Server        OptionalNonNull[string]                   `json:"server,omitzero"`
	Source        OptionalNonNull[GuardianCommandSource]    `json:"source,omitzero"`
	Stdin         OptionalNonNull[string]                   `json:"stdin,omitzero"`
	Target        OptionalNonNull[string]                   `json:"target,omitzero"`
	ToolName      OptionalNonNull[string]                   `json:"toolName,omitzero"`
	ToolTitle     Optional[string]                          `json:"toolTitle,omitzero"`
	TypeValue     string                                    `json:"type"`
	RawJSON       json.RawMessage                           `json:"-"`
}

var guardianApprovalReviewActionSpec = objectSpec{
	fields: []objectField{
		{name: "approvalId", flags: fieldNonNullOptional},
		{name: "argv", flags: fieldNonNullOptional},
		{name: "command", flags: fieldNonNullOptional},
		{name: "connectorId"},
		{name: "connectorName"},
		{name: "cwd", flags: fieldNonNullOptional},
		{name: "files", flags: fieldNonNullOptional},
		{name: "host", flags: fieldNonNullOptional},
		{name: "permissions", flags: fieldNonNullOptional},
		{name: "port", flags: fieldNonNullOptional},
		{name: "processId", flags: fieldNonNullOptional},
		{name: "program", flags: fieldNonNullOptional},
		{name: "protocol", flags: fieldNonNullOptional},
		{name: "reason"},
		{name: "server", flags: fieldNonNullOptional},
		{name: "source", flags: fieldNonNullOptional},
		{name: "stdin", flags: fieldNonNullOptional},
		{name: "target", flags: fieldNonNullOptional},
		{name: "toolName", flags: fieldNonNullOptional},
		{name: "toolTitle"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  20,
	tags: []string{"command", "execve", "writeStdin", "applyPatch", "networkAccess", "mcpToolCall", "requestPermissions"},
}

func (v GuardianApprovalReviewAction) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "command":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type command"}
		}
		if !v.Cwd.IsSet() {
			return nil, DecodeError{Field: "cwd", Reason: "missing required field for type command"}
		}
		if !v.Source.IsSet() {
			return nil, DecodeError{Field: "source", Reason: "missing required field for type command"}
		}
	case "execve":
		if !v.Argv.IsSet() {
			return nil, DecodeError{Field: "argv", Reason: "missing required field for type execve"}
		}
		if !v.Cwd.IsSet() {
			return nil, DecodeError{Field: "cwd", Reason: "missing required field for type execve"}
		}
		if !v.Program.IsSet() {
			return nil, DecodeError{Field: "program", Reason: "missing required field for type execve"}
		}
		if !v.Source.IsSet() {
			return nil, DecodeError{Field: "source", Reason: "missing required field for type execve"}
		}
	case "writeStdin":
		if !v.ApprovalID.IsSet() {
			return nil, DecodeError{Field: "approvalId", Reason: "missing required field for type writeStdin"}
		}
		if !v.Cwd.IsSet() {
			return nil, DecodeError{Field: "cwd", Reason: "missing required field for type writeStdin"}
		}
		if !v.ProcessID.IsSet() {
			return nil, DecodeError{Field: "processId", Reason: "missing required field for type writeStdin"}
		}
		if !v.Stdin.IsSet() {
			return nil, DecodeError{Field: "stdin", Reason: "missing required field for type writeStdin"}
		}
	case "applyPatch":
		if !v.Cwd.IsSet() {
			return nil, DecodeError{Field: "cwd", Reason: "missing required field for type applyPatch"}
		}
		if !v.Files.IsSet() {
			return nil, DecodeError{Field: "files", Reason: "missing required field for type applyPatch"}
		}
	case "networkAccess":
		if !v.Host.IsSet() {
			return nil, DecodeError{Field: "host", Reason: "missing required field for type networkAccess"}
		}
		if !v.Port.IsSet() {
			return nil, DecodeError{Field: "port", Reason: "missing required field for type networkAccess"}
		}
		if !v.Protocol.IsSet() {
			return nil, DecodeError{Field: "protocol", Reason: "missing required field for type networkAccess"}
		}
		if !v.Target.IsSet() {
			return nil, DecodeError{Field: "target", Reason: "missing required field for type networkAccess"}
		}
	case "mcpToolCall":
		if !v.Server.IsSet() {
			return nil, DecodeError{Field: "server", Reason: "missing required field for type mcpToolCall"}
		}
		if !v.ToolName.IsSet() {
			return nil, DecodeError{Field: "toolName", Reason: "missing required field for type mcpToolCall"}
		}
	case "requestPermissions":
		if !v.Permissions.IsSet() {
			return nil, DecodeError{Field: "permissions", Reason: "missing required field for type requestPermissions"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire GuardianApprovalReviewAction
	return json.Marshal(wire(v))
}

func (v *GuardianApprovalReviewAction) UnmarshalJSON(data []byte) error {
	type wire GuardianApprovalReviewAction
	var s objectScan
	known, err := s.begin(data, &guardianApprovalReviewActionSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valuePort, ok := v.Port.Value(); ok {
		if valuePort < 0 {
			return DecodeError{Field: "port", Reason: "below minimum 0"}
		}
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "command":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type command"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
		if !s.has("cwd") {
			return DecodeError{Field: "cwd", Reason: "missing required field for type command"}
		}
		if s.isNull("cwd") {
			return DecodeError{Field: "cwd", Reason: "cannot be null"}
		}
		if !s.has("source") {
			return DecodeError{Field: "source", Reason: "missing required field for type command"}
		}
		if s.isNull("source") {
			return DecodeError{Field: "source", Reason: "cannot be null"}
		}
	case "execve":
		if !s.has("argv") {
			return DecodeError{Field: "argv", Reason: "missing required field for type execve"}
		}
		if s.isNull("argv") {
			return DecodeError{Field: "argv", Reason: "cannot be null"}
		}
		if !s.has("cwd") {
			return DecodeError{Field: "cwd", Reason: "missing required field for type execve"}
		}
		if s.isNull("cwd") {
			return DecodeError{Field: "cwd", Reason: "cannot be null"}
		}
		if !s.has("program") {
			return DecodeError{Field: "program", Reason: "missing required field for type execve"}
		}
		if s.isNull("program") {
			return DecodeError{Field: "program", Reason: "cannot be null"}
		}
		if !s.has("source") {
			return DecodeError{Field: "source", Reason: "missing required field for type execve"}
		}
		if s.isNull("source") {
			return DecodeError{Field: "source", Reason: "cannot be null"}
		}
	case "writeStdin":
		if !s.has("approvalId") {
			return DecodeError{Field: "approvalId", Reason: "missing required field for type writeStdin"}
		}
		if s.isNull("approvalId") {
			return DecodeError{Field: "approvalId", Reason: "cannot be null"}
		}
		if !s.has("cwd") {
			return DecodeError{Field: "cwd", Reason: "missing required field for type writeStdin"}
		}
		if s.isNull("cwd") {
			return DecodeError{Field: "cwd", Reason: "cannot be null"}
		}
		if !s.has("processId") {
			return DecodeError{Field: "processId", Reason: "missing required field for type writeStdin"}
		}
		if s.isNull("processId") {
			return DecodeError{Field: "processId", Reason: "cannot be null"}
		}
		if !s.has("stdin") {
			return DecodeError{Field: "stdin", Reason: "missing required field for type writeStdin"}
		}
		if s.isNull("stdin") {
			return DecodeError{Field: "stdin", Reason: "cannot be null"}
		}
	case "applyPatch":
		if !s.has("cwd") {
			return DecodeError{Field: "cwd", Reason: "missing required field for type applyPatch"}
		}
		if s.isNull("cwd") {
			return DecodeError{Field: "cwd", Reason: "cannot be null"}
		}
		if !s.has("files") {
			return DecodeError{Field: "files", Reason: "missing required field for type applyPatch"}
		}
		if s.isNull("files") {
			return DecodeError{Field: "files", Reason: "cannot be null"}
		}
	case "networkAccess":
		if !s.has("host") {
			return DecodeError{Field: "host", Reason: "missing required field for type networkAccess"}
		}
		if s.isNull("host") {
			return DecodeError{Field: "host", Reason: "cannot be null"}
		}
		if !s.has("port") {
			return DecodeError{Field: "port", Reason: "missing required field for type networkAccess"}
		}
		if s.isNull("port") {
			return DecodeError{Field: "port", Reason: "cannot be null"}
		}
		if !s.has("protocol") {
			return DecodeError{Field: "protocol", Reason: "missing required field for type networkAccess"}
		}
		if s.isNull("protocol") {
			return DecodeError{Field: "protocol", Reason: "cannot be null"}
		}
		if !s.has("target") {
			return DecodeError{Field: "target", Reason: "missing required field for type networkAccess"}
		}
		if s.isNull("target") {
			return DecodeError{Field: "target", Reason: "cannot be null"}
		}
	case "mcpToolCall":
		if !s.has("server") {
			return DecodeError{Field: "server", Reason: "missing required field for type mcpToolCall"}
		}
		if s.isNull("server") {
			return DecodeError{Field: "server", Reason: "cannot be null"}
		}
		if !s.has("toolName") {
			return DecodeError{Field: "toolName", Reason: "missing required field for type mcpToolCall"}
		}
		if s.isNull("toolName") {
			return DecodeError{Field: "toolName", Reason: "cannot be null"}
		}
	case "requestPermissions":
		if !s.has("permissions") {
			return DecodeError{Field: "permissions", Reason: "missing required field for type requestPermissions"}
		}
		if s.isNull("permissions") {
			return DecodeError{Field: "permissions", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type GuardianApprovalReviewStatus string

const (
	GuardianApprovalReviewStatusInProgress GuardianApprovalReviewStatus = "inProgress"
	GuardianApprovalReviewStatusApproved   GuardianApprovalReviewStatus = "approved"
	GuardianApprovalReviewStatusDenied     GuardianApprovalReviewStatus = "denied"
	GuardianApprovalReviewStatusTimedOut   GuardianApprovalReviewStatus = "timedOut"
	GuardianApprovalReviewStatusAborted    GuardianApprovalReviewStatus = "aborted"
)

type GuardianCommandSource string

const (
	GuardianCommandSourceShell       GuardianCommandSource = "shell"
	GuardianCommandSourceUnifiedExec GuardianCommandSource = "unifiedExec"
)

type GuardianRiskLevel string

const (
	GuardianRiskLevelLow      GuardianRiskLevel = "low"
	GuardianRiskLevelMedium   GuardianRiskLevel = "medium"
	GuardianRiskLevelHigh     GuardianRiskLevel = "high"
	GuardianRiskLevelCritical GuardianRiskLevel = "critical"
)

type GuardianUserAuthorization string

const (
	GuardianUserAuthorizationUnknown GuardianUserAuthorization = "unknown"
	GuardianUserAuthorizationLow     GuardianUserAuthorization = "low"
	GuardianUserAuthorizationMedium  GuardianUserAuthorization = "medium"
	GuardianUserAuthorizationHigh    GuardianUserAuthorization = "high"
)

type GuardianWarningNotification struct {
	Message  string `json:"message"`
	ThreadID string `json:"threadId"`
}

var guardianWarningNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v GuardianWarningNotification) MarshalJSON() ([]byte, error) {
	type wire GuardianWarningNotification
	return json.Marshal(wire(v))
}

func (v *GuardianWarningNotification) UnmarshalJSON(data []byte) error {
	type wire GuardianWarningNotification
	return decodeObject(data, &guardianWarningNotificationSpec, (*wire)(v))
}

type HookCompletedNotification struct {
	Run      HookRunSummary   `json:"run"`
	ThreadID string           `json:"threadId"`
	TurnID   Optional[string] `json:"turnId,omitzero"`
}

var hookCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "run", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId"},
	},
}

func (v HookCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire HookCompletedNotification
	return json.Marshal(wire(v))
}

func (v *HookCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire HookCompletedNotification
	return decodeObject(data, &hookCompletedNotificationSpec, (*wire)(v))
}

type HookErrorInfo struct {
	Message string `json:"message"`
	Path    string `json:"path"`
}

var hookErrorInfoSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HookErrorInfo) MarshalJSON() ([]byte, error) {
	type wire HookErrorInfo
	return json.Marshal(wire(v))
}

func (v *HookErrorInfo) UnmarshalJSON(data []byte) error {
	type wire HookErrorInfo
	return decodeObject(data, &hookErrorInfoSpec, (*wire)(v))
}

type HookEventName string

const (
	HookEventNamePreModelRequest   HookEventName = "preModelRequest"
	HookEventNamePostModelRequest  HookEventName = "postModelRequest"
	HookEventNamePreToolUse        HookEventName = "preToolUse"
	HookEventNamePermissionRequest HookEventName = "permissionRequest"
	HookEventNamePostToolUse       HookEventName = "postToolUse"
	HookEventNamePreCompact        HookEventName = "preCompact"
	HookEventNamePostCompact       HookEventName = "postCompact"
	HookEventNameSessionStart      HookEventName = "sessionStart"
	HookEventNameSessionEnd        HookEventName = "sessionEnd"
	HookEventNameUserPromptSubmit  HookEventName = "userPromptSubmit"
	HookEventNameSubagentStart     HookEventName = "subagentStart"
	HookEventNameSubagentStop      HookEventName = "subagentStop"
	HookEventNameStop              HookEventName = "stop"
	HookEventNameInterrupt         HookEventName = "interrupt"
)

type HookExecutionMode string

const (
	HookExecutionModeSync  HookExecutionMode = "sync"
	HookExecutionModeAsync HookExecutionMode = "async"
)

type HookHandlerType string

const (
	HookHandlerTypeCommand HookHandlerType = "command"
	HookHandlerTypeMcpTool HookHandlerType = "mcpTool"
	HookHandlerTypePrompt  HookHandlerType = "prompt"
	HookHandlerTypeAgent   HookHandlerType = "agent"
)

type HookMetadata struct {
	AdditionalContextLimit Optional[uint64]        `json:"additionalContextLimit,omitzero"`
	Async                  OptionalNonNull[bool]   `json:"async,omitzero"`
	Command                OptionalNonNull[string] `json:"command,omitzero"`
	CurrentHash            string                  `json:"currentHash"`
	DisplayOrder           int64                   `json:"displayOrder"`
	Enabled                bool                    `json:"enabled"`
	EnvironmentID          Optional[string]        `json:"environmentId,omitzero"`
	EventName              HookEventName           `json:"eventName"`
	HandlerType            OptionalNonNull[string] `json:"handlerType,omitzero"`
	IsManaged              bool                    `json:"isManaged"`
	Key                    string                  `json:"key"`
	Matcher                Optional[string]        `json:"matcher,omitzero"`
	PluginID               Optional[string]        `json:"pluginId,omitzero"`
	Server                 OptionalNonNull[string] `json:"server,omitzero"`
	Source                 HookSource              `json:"source"`
	SourcePath             AbsolutePathBuf         `json:"sourcePath"`
	StatusMessage          Optional[string]        `json:"statusMessage,omitzero"`
	TimeoutSec             uint64                  `json:"timeoutSec"`
	Tool                   OptionalNonNull[string] `json:"tool,omitzero"`
	TrustStatus            HookTrustStatus         `json:"trustStatus"`
	RawJSON                json.RawMessage         `json:"-"`
}

var hookMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "additionalContextLimit"},
		{name: "async", flags: fieldNonNullOptional},
		{name: "command", flags: fieldNonNullOptional},
		{name: "currentHash", flags: fieldRequired | fieldRejectNull},
		{name: "displayOrder", flags: fieldRequired | fieldRejectNull},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "environmentId"},
		{name: "eventName", flags: fieldRequired | fieldRejectNull},
		{name: "handlerType", flags: fieldNonNullOptional},
		{name: "isManaged", flags: fieldRequired | fieldRejectNull},
		{name: "key", flags: fieldRequired | fieldRejectNull},
		{name: "matcher"},
		{name: "pluginId"},
		{name: "server", flags: fieldNonNullOptional},
		{name: "source", flags: fieldRequired | fieldRejectNull},
		{name: "sourcePath", flags: fieldRequired | fieldRejectNull},
		{name: "statusMessage"},
		{name: "timeoutSec", flags: fieldRequired | fieldRejectNull},
		{name: "tool", flags: fieldNonNullOptional},
		{name: "trustStatus", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HookMetadata) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.AdditionalContextLimit.IsSet() {
		out["additionalContextLimit"] = v.AdditionalContextLimit
	}
	if v.Async.IsSet() {
		out["async"] = v.Async
	}
	if v.Command.IsSet() {
		out["command"] = v.Command
	}
	out["currentHash"] = v.CurrentHash
	out["displayOrder"] = v.DisplayOrder
	out["enabled"] = v.Enabled
	if v.EnvironmentID.IsSet() {
		out["environmentId"] = v.EnvironmentID
	}
	out["eventName"] = v.EventName
	if v.HandlerType.IsSet() {
		out["handlerType"] = v.HandlerType
	}
	out["isManaged"] = v.IsManaged
	out["key"] = v.Key
	if v.Matcher.IsSet() {
		out["matcher"] = v.Matcher
	}
	if v.PluginID.IsSet() {
		out["pluginId"] = v.PluginID
	}
	if v.Server.IsSet() {
		out["server"] = v.Server
	}
	out["source"] = v.Source
	out["sourcePath"] = v.SourcePath
	if v.StatusMessage.IsSet() {
		out["statusMessage"] = v.StatusMessage
	}
	out["timeoutSec"] = v.TimeoutSec
	if v.Tool.IsSet() {
		out["tool"] = v.Tool
	}
	out["trustStatus"] = v.TrustStatus
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	untaggedOneOfVariant0ConstMatches := true
	if _, ok := out["command"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if _, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if rawValue, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant0Matches = false
		untaggedOneOfVariant0ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"command\"")) {
		untaggedOneOfVariant0Matches = false
		untaggedOneOfVariant0ConstMatches = false
	}
	if untaggedOneOfVariant0ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	untaggedOneOfVariant1ConstMatches := true
	if _, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if _, ok := out["server"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if _, ok := out["tool"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if rawValue, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant1Matches = false
		untaggedOneOfVariant1ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"mcpTool\"")) {
		untaggedOneOfVariant1Matches = false
		untaggedOneOfVariant1ConstMatches = false
	}
	if untaggedOneOfVariant1ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant2Matches := true
	untaggedOneOfVariant2ConstMatches := true
	if _, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant2Matches = false
	}
	if rawValue, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant2Matches = false
		untaggedOneOfVariant2ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"prompt\"")) {
		untaggedOneOfVariant2Matches = false
		untaggedOneOfVariant2ConstMatches = false
	}
	if untaggedOneOfVariant2ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant2Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant3Matches := true
	untaggedOneOfVariant3ConstMatches := true
	if _, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant3Matches = false
	}
	if rawValue, ok := out["handlerType"]; !ok {
		untaggedOneOfVariant3Matches = false
		untaggedOneOfVariant3ConstMatches = false
	} else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte("\"agent\"")) {
		untaggedOneOfVariant3Matches = false
		untaggedOneOfVariant3ConstMatches = false
	}
	if untaggedOneOfVariant3ConstMatches {
		untaggedOneOfKnownVariantMatches = true
	}
	if untaggedOneOfVariant3Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid HookMetadata raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *HookMetadata) UnmarshalJSON(data []byte) error {
	type wire HookMetadata
	var s objectScan
	if _, err := s.begin(data, &hookMetadataSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueAdditionalContextLimit, ok := v.AdditionalContextLimit.Value(); ok {
		if valueAdditionalContextLimit < 0 {
			return DecodeError{Field: "additionalContextLimit", Reason: "below minimum 0"}
		}
	}
	if v.TimeoutSec < 0 {
		return DecodeError{Field: "timeoutSec", Reason: "below minimum 0"}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.rawEquals("handlerType", "\"command\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("command") && s.nonNull("handlerType") && s.rawEquals("handlerType", "\"command\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("handlerType", "\"mcpTool\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("handlerType") && s.nonNull("server") && s.nonNull("tool") && s.rawEquals("handlerType", "\"mcpTool\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("handlerType", "\"prompt\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("handlerType") && s.rawEquals("handlerType", "\"prompt\"") {
		untaggedOneOfMatches++
	}
	if s.rawEquals("handlerType", "\"agent\"") {
		untaggedOneOfKnownVariantMatches = true
	}
	if s.nonNull("handlerType") && s.rawEquals("handlerType", "\"agent\"") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type HookMigration struct {
	Name string `json:"name"`
}

var hookMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HookMigration) MarshalJSON() ([]byte, error) {
	type wire HookMigration
	return json.Marshal(wire(v))
}

func (v *HookMigration) UnmarshalJSON(data []byte) error {
	type wire HookMigration
	return decodeObject(data, &hookMigrationSpec, (*wire)(v))
}

type HookOutputEntry struct {
	Kind HookOutputEntryKind `json:"kind"`
	Text string              `json:"text"`
}

var hookOutputEntrySpec = objectSpec{
	fields: []objectField{
		{name: "kind", flags: fieldRequired | fieldRejectNull},
		{name: "text", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HookOutputEntry) MarshalJSON() ([]byte, error) {
	type wire HookOutputEntry
	return json.Marshal(wire(v))
}

func (v *HookOutputEntry) UnmarshalJSON(data []byte) error {
	type wire HookOutputEntry
	return decodeObject(data, &hookOutputEntrySpec, (*wire)(v))
}

type HookOutputEntryKind string

const (
	HookOutputEntryKindWarning  HookOutputEntryKind = "warning"
	HookOutputEntryKindStop     HookOutputEntryKind = "stop"
	HookOutputEntryKindFeedback HookOutputEntryKind = "feedback"
	HookOutputEntryKindContext  HookOutputEntryKind = "context"
	HookOutputEntryKindError    HookOutputEntryKind = "error"
)

type HookPromptFragment struct {
	HookRunID string `json:"hookRunId"`
	Text      string `json:"text"`
}

var hookPromptFragmentSpec = objectSpec{
	fields: []objectField{
		{name: "hookRunId", flags: fieldRequired | fieldRejectNull},
		{name: "text", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HookPromptFragment) MarshalJSON() ([]byte, error) {
	type wire HookPromptFragment
	return json.Marshal(wire(v))
}

func (v *HookPromptFragment) UnmarshalJSON(data []byte) error {
	type wire HookPromptFragment
	return decodeObject(data, &hookPromptFragmentSpec, (*wire)(v))
}

type HookRunStatus string

const (
	HookRunStatusRunning   HookRunStatus = "running"
	HookRunStatusCompleted HookRunStatus = "completed"
	HookRunStatusFailed    HookRunStatus = "failed"
	HookRunStatusBlocked   HookRunStatus = "blocked"
	HookRunStatusStopped   HookRunStatus = "stopped"
)

type HookRunSummary struct {
	CompletedAt   Optional[int64]             `json:"completedAt,omitzero"`
	DisplayOrder  int64                       `json:"displayOrder"`
	DurationMs    Optional[int64]             `json:"durationMs,omitzero"`
	Entries       []HookOutputEntry           `json:"entries"`
	EventName     HookEventName               `json:"eventName"`
	ExecutionMode HookExecutionMode           `json:"executionMode"`
	HandlerType   HookHandlerType             `json:"handlerType"`
	ID            string                      `json:"id"`
	Scope         HookScope                   `json:"scope"`
	Source        OptionalNonNull[HookSource] `json:"source,omitzero"`
	SourcePath    LegacyAppPathString         `json:"sourcePath"`
	StartedAt     int64                       `json:"startedAt"`
	Status        HookRunStatus               `json:"status"`
	StatusMessage Optional[string]            `json:"statusMessage,omitzero"`
}

var hookRunSummarySpec = objectSpec{
	fields: []objectField{
		{name: "completedAt"},
		{name: "displayOrder", flags: fieldRequired | fieldRejectNull},
		{name: "durationMs"},
		{name: "entries", flags: fieldRequired | fieldRejectNull},
		{name: "eventName", flags: fieldRequired | fieldRejectNull},
		{name: "executionMode", flags: fieldRequired | fieldRejectNull},
		{name: "handlerType", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "scope", flags: fieldRequired | fieldRejectNull},
		{name: "source", flags: fieldNonNullOptional},
		{name: "sourcePath", flags: fieldRequired | fieldRejectNull},
		{name: "startedAt", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "statusMessage"},
	},
}

func (v HookRunSummary) MarshalJSON() ([]byte, error) {
	type wire HookRunSummary
	return json.Marshal(wire(v))
}

func (v *HookRunSummary) UnmarshalJSON(data []byte) error {
	type wire HookRunSummary
	return decodeObject(data, &hookRunSummarySpec, (*wire)(v))
}

type HookScope string

const (
	HookScopeThread HookScope = "thread"
	HookScopeTurn   HookScope = "turn"
)

type HookSource string

const (
	HookSourceSystem                  HookSource = "system"
	HookSourceUser                    HookSource = "user"
	HookSourceProject                 HookSource = "project"
	HookSourceMdm                     HookSource = "mdm"
	HookSourceSessionFlags            HookSource = "sessionFlags"
	HookSourcePlugin                  HookSource = "plugin"
	HookSourceCloudRequirements       HookSource = "cloudRequirements"
	HookSourceCloudManagedConfig      HookSource = "cloudManagedConfig"
	HookSourceLegacyManagedConfigFile HookSource = "legacyManagedConfigFile"
	HookSourceLegacyManagedConfigMdm  HookSource = "legacyManagedConfigMdm"
	HookSourceUnknown                 HookSource = "unknown"
)

type HookStartedNotification struct {
	Run      HookRunSummary   `json:"run"`
	ThreadID string           `json:"threadId"`
	TurnID   Optional[string] `json:"turnId,omitzero"`
}

var hookStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "run", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId"},
	},
}

func (v HookStartedNotification) MarshalJSON() ([]byte, error) {
	type wire HookStartedNotification
	return json.Marshal(wire(v))
}

func (v *HookStartedNotification) UnmarshalJSON(data []byte) error {
	type wire HookStartedNotification
	return decodeObject(data, &hookStartedNotificationSpec, (*wire)(v))
}

type HookTrustStatus string

const (
	HookTrustStatusManaged   HookTrustStatus = "managed"
	HookTrustStatusUntrusted HookTrustStatus = "untrusted"
	HookTrustStatusTrusted   HookTrustStatus = "trusted"
	HookTrustStatusModified  HookTrustStatus = "modified"
)

type HooksListEntry struct {
	Cwd      string          `json:"cwd"`
	Errors   []HookErrorInfo `json:"errors"`
	Hooks    []HookMetadata  `json:"hooks"`
	Warnings []string        `json:"warnings"`
}

var hooksListEntrySpec = objectSpec{
	fields: []objectField{
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "errors", flags: fieldRequired | fieldRejectNull},
		{name: "hooks", flags: fieldRequired | fieldRejectNull},
		{name: "warnings", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HooksListEntry) MarshalJSON() ([]byte, error) {
	type wire HooksListEntry
	return json.Marshal(wire(v))
}

func (v *HooksListEntry) UnmarshalJSON(data []byte) error {
	type wire HooksListEntry
	return decodeObject(data, &hooksListEntrySpec, (*wire)(v))
}

type HooksListParams struct {
	Cwds     OptionalNonNull[[]string] `json:"cwds,omitzero"`
	ThreadID Optional[string]          `json:"threadId,omitzero"`
}

var hooksListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "threadId"},
	},
}

func (v HooksListParams) MarshalJSON() ([]byte, error) {
	type wire HooksListParams
	w := wire(v)
	w.Cwds = nonNullWithoutEmpty(w.Cwds)
	return json.Marshal(w)
}

func (v *HooksListParams) UnmarshalJSON(data []byte) error {
	type wire HooksListParams
	return decodeObject(data, &hooksListParamsSpec, (*wire)(v))
}

type HooksListResponse struct {
	Data []HooksListEntry `json:"data"`
}

var hooksListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v HooksListResponse) MarshalJSON() ([]byte, error) {
	type wire HooksListResponse
	return json.Marshal(wire(v))
}

func (v *HooksListResponse) UnmarshalJSON(data []byte) error {
	type wire HooksListResponse
	return decodeObject(data, &hooksListResponseSpec, (*wire)(v))
}

type ImageDetail string

const (
	ImageDetailAuto     ImageDetail = "auto"
	ImageDetailLow      ImageDetail = "low"
	ImageDetailHigh     ImageDetail = "high"
	ImageDetailOriginal ImageDetail = "original"
)

type ImageGenerationFailure struct {
	LimitID   OptionalNonNull[string] `json:"limitId,omitzero"`
	ResetsAt  Optional[int64]         `json:"resetsAt,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var imageGenerationFailureSpec = objectSpec{
	fields: []objectField{
		{name: "limitId", flags: fieldNonNullOptional},
		{name: "resetsAt"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  2,
	tags: []string{"usageLimitExceeded"},
}

func (v ImageGenerationFailure) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "usageLimitExceeded":
		if !v.LimitID.IsSet() {
			return nil, DecodeError{Field: "limitId", Reason: "missing required field for type usageLimitExceeded"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ImageGenerationFailure
	return json.Marshal(wire(v))
}

func (v *ImageGenerationFailure) UnmarshalJSON(data []byte) error {
	type wire ImageGenerationFailure
	var s objectScan
	known, err := s.begin(data, &imageGenerationFailureSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "usageLimitExceeded":
		if !s.has("limitId") {
			return DecodeError{Field: "limitId", Reason: "missing required field for type usageLimitExceeded"}
		}
		if s.isNull("limitId") {
			return DecodeError{Field: "limitId", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ImageGenerationInputImage struct {
	Path   AbsolutePathBuf `json:"path"`
	Sha256 string          `json:"sha256"`
}

var imageGenerationInputImageSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "sha256", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ImageGenerationInputImage) MarshalJSON() ([]byte, error) {
	type wire ImageGenerationInputImage
	return json.Marshal(wire(v))
}

func (v *ImageGenerationInputImage) UnmarshalJSON(data []byte) error {
	type wire ImageGenerationInputImage
	return decodeObject(data, &imageGenerationInputImageSpec, (*wire)(v))
}

type InAppBrowserRequirements struct {
	AllowExternalBrowserSettingsImport Optional[bool] `json:"allowExternalBrowserSettingsImport,omitzero"`
}

var inAppBrowserRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "allowExternalBrowserSettingsImport"},
	},
}

func (v InAppBrowserRequirements) MarshalJSON() ([]byte, error) {
	type wire InAppBrowserRequirements
	return json.Marshal(wire(v))
}

func (v *InAppBrowserRequirements) UnmarshalJSON(data []byte) error {
	type wire InAppBrowserRequirements
	return decodeObject(data, &inAppBrowserRequirementsSpec, (*wire)(v))
}

type InputModality string

const (
	InputModalityText  InputModality = "text"
	InputModalityImage InputModality = "image"
	InputModalityAudio InputModality = "audio"
)

type InstalledApp struct {
	Callable    bool             `json:"callable"`
	Enabled     bool             `json:"enabled"`
	ID          string           `json:"id"`
	RuntimeName Optional[string] `json:"runtimeName,omitzero"`
}

var installedAppSpec = objectSpec{
	fields: []objectField{
		{name: "callable", flags: fieldRequired | fieldRejectNull},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "runtimeName"},
	},
}

func (v InstalledApp) MarshalJSON() ([]byte, error) {
	type wire InstalledApp
	return json.Marshal(wire(v))
}

func (v *InstalledApp) UnmarshalJSON(data []byte) error {
	type wire InstalledApp
	return decodeObject(data, &installedAppSpec, (*wire)(v))
}

type InternalChatMessageMetadataPassthrough struct {
	TurnID Optional[string] `json:"turn_id,omitzero"`
}

var internalChatMessageMetadataPassthroughSpec = objectSpec{
	fields: []objectField{
		{name: "turn_id"},
	},
}

func (v InternalChatMessageMetadataPassthrough) MarshalJSON() ([]byte, error) {
	type wire InternalChatMessageMetadataPassthrough
	return json.Marshal(wire(v))
}

func (v *InternalChatMessageMetadataPassthrough) UnmarshalJSON(data []byte) error {
	type wire InternalChatMessageMetadataPassthrough
	return decodeObject(data, &internalChatMessageMetadataPassthroughSpec, (*wire)(v))
}

type ItemCompletedNotification struct {
	CompletedAtMs int64      `json:"completedAtMs"`
	Item          ThreadItem `json:"item"`
	ThreadID      string     `json:"threadId"`
	TurnID        string     `json:"turnId"`
}

var itemCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "completedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "item", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ItemCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire ItemCompletedNotification
	return json.Marshal(wire(v))
}

func (v *ItemCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire ItemCompletedNotification
	return decodeObject(data, &itemCompletedNotificationSpec, (*wire)(v))
}

type ItemGuardianApprovalReviewCompletedNotification struct {
	Action         GuardianApprovalReviewAction `json:"action"`
	CompletedAtMs  int64                        `json:"completedAtMs"`
	DecisionSource AutoReviewDecisionSource     `json:"decisionSource"`
	Review         GuardianApprovalReview       `json:"review"`
	ReviewID       string                       `json:"reviewId"`
	StartedAtMs    int64                        `json:"startedAtMs"`
	TargetItemID   Optional[string]             `json:"targetItemId,omitzero"`
	ThreadID       string                       `json:"threadId"`
	TurnID         string                       `json:"turnId"`
}

var itemGuardianApprovalReviewCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "action", flags: fieldRequired | fieldRejectNull},
		{name: "completedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "decisionSource", flags: fieldRequired | fieldRejectNull},
		{name: "review", flags: fieldRequired | fieldRejectNull},
		{name: "reviewId", flags: fieldRequired | fieldRejectNull},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "targetItemId"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ItemGuardianApprovalReviewCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire ItemGuardianApprovalReviewCompletedNotification
	return json.Marshal(wire(v))
}

func (v *ItemGuardianApprovalReviewCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire ItemGuardianApprovalReviewCompletedNotification
	return decodeObject(data, &itemGuardianApprovalReviewCompletedNotificationSpec, (*wire)(v))
}

type ItemGuardianApprovalReviewStartedNotification struct {
	Action       GuardianApprovalReviewAction `json:"action"`
	Review       GuardianApprovalReview       `json:"review"`
	ReviewID     string                       `json:"reviewId"`
	StartedAtMs  int64                        `json:"startedAtMs"`
	TargetItemID Optional[string]             `json:"targetItemId,omitzero"`
	ThreadID     string                       `json:"threadId"`
	TurnID       string                       `json:"turnId"`
}

var itemGuardianApprovalReviewStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "action", flags: fieldRequired | fieldRejectNull},
		{name: "review", flags: fieldRequired | fieldRejectNull},
		{name: "reviewId", flags: fieldRequired | fieldRejectNull},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "targetItemId"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ItemGuardianApprovalReviewStartedNotification) MarshalJSON() ([]byte, error) {
	type wire ItemGuardianApprovalReviewStartedNotification
	return json.Marshal(wire(v))
}

func (v *ItemGuardianApprovalReviewStartedNotification) UnmarshalJSON(data []byte) error {
	type wire ItemGuardianApprovalReviewStartedNotification
	return decodeObject(data, &itemGuardianApprovalReviewStartedNotificationSpec, (*wire)(v))
}

type ItemStartedNotification struct {
	Item        ThreadItem `json:"item"`
	StartedAtMs int64      `json:"startedAtMs"`
	ThreadID    string     `json:"threadId"`
	TurnID      string     `json:"turnId"`
}

var itemStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "item", flags: fieldRequired | fieldRejectNull},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ItemStartedNotification) MarshalJSON() ([]byte, error) {
	type wire ItemStartedNotification
	return json.Marshal(wire(v))
}

func (v *ItemStartedNotification) UnmarshalJSON(data []byte) error {
	type wire ItemStartedNotification
	return decodeObject(data, &itemStartedNotificationSpec, (*wire)(v))
}

type LegacyAppPathString string

type ListMcpServerStatusParams struct {
	Cursor     Optional[string]                `json:"cursor,omitzero"`
	Detail     Optional[McpServerStatusDetail] `json:"detail,omitzero"`
	Limit      Optional[uint32]                `json:"limit,omitzero"`
	ServerName Optional[string]                `json:"serverName,omitzero"`
	ThreadID   Optional[string]                `json:"threadId,omitzero"`
}

var listMcpServerStatusParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "detail"},
		{name: "limit"},
		{name: "serverName"},
		{name: "threadId"},
	},
}

func (v ListMcpServerStatusParams) MarshalJSON() ([]byte, error) {
	type wire ListMcpServerStatusParams
	return json.Marshal(wire(v))
}

func (v *ListMcpServerStatusParams) UnmarshalJSON(data []byte) error {
	type wire ListMcpServerStatusParams
	var s objectScan
	if _, err := s.begin(data, &listMcpServerStatusParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ListMcpServerStatusResponse struct {
	Data       []McpServerStatus `json:"data"`
	NextCursor Optional[string]  `json:"nextCursor,omitzero"`
}

var listMcpServerStatusResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ListMcpServerStatusResponse) MarshalJSON() ([]byte, error) {
	type wire ListMcpServerStatusResponse
	return json.Marshal(wire(v))
}

func (v *ListMcpServerStatusResponse) UnmarshalJSON(data []byte) error {
	type wire ListMcpServerStatusResponse
	return decodeObject(data, &listMcpServerStatusResponseSpec, (*wire)(v))
}

type LocalShellAction struct {
	Command          OptionalNonNull[[]string]   `json:"command,omitzero"`
	Env              Optional[map[string]string] `json:"env,omitzero"`
	TimeoutMs        Optional[uint64]            `json:"timeout_ms,omitzero"`
	TypeValue        string                      `json:"type"`
	User             Optional[string]            `json:"user,omitzero"`
	WorkingDirectory Optional[string]            `json:"working_directory,omitzero"`
	RawJSON          json.RawMessage             `json:"-"`
}

var localShellActionSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldNonNullOptional},
		{name: "env"},
		{name: "timeout_ms"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "user"},
		{name: "working_directory"},
	},
	tag:  3,
	tags: []string{"exec"},
}

func (v LocalShellAction) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "exec":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type exec"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire LocalShellAction
	return json.Marshal(wire(v))
}

func (v *LocalShellAction) UnmarshalJSON(data []byte) error {
	type wire LocalShellAction
	var s objectScan
	known, err := s.begin(data, &localShellActionSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueTimeoutMs, ok := v.TimeoutMs.Value(); ok {
		if valueTimeoutMs < 0 {
			return DecodeError{Field: "timeout_ms", Reason: "below minimum 0"}
		}
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "exec":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type exec"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type LocalShellStatus string

const (
	LocalShellStatusCompleted  LocalShellStatus = "completed"
	LocalShellStatusInProgress LocalShellStatus = "in_progress"
	LocalShellStatusIncomplete LocalShellStatus = "incomplete"
)

type LoginAccountParams struct {
	AccessKeyID               OptionalNonNull[string] `json:"accessKeyId,omitzero"`
	AccessToken               OptionalNonNull[string] `json:"accessToken,omitzero"`
	APIKey                    OptionalNonNull[string] `json:"apiKey,omitzero"`
	AppBrand                  Optional[LoginAppBrand] `json:"appBrand,omitzero"`
	ChatgptAccountID          OptionalNonNull[string] `json:"chatgptAccountId,omitzero"`
	ChatgptPlanType           Optional[string]        `json:"chatgptPlanType,omitzero"`
	CodexStreamlinedLogin     OptionalNonNull[bool]   `json:"codexStreamlinedLogin,omitzero"`
	Region                    OptionalNonNull[string] `json:"region,omitzero"`
	SecretAccessKey           OptionalNonNull[string] `json:"secretAccessKey,omitzero"`
	SessionToken              Optional[string]        `json:"sessionToken,omitzero"`
	TypeValue                 string                  `json:"type"`
	UseHostedLoginSuccessPage OptionalNonNull[bool]   `json:"useHostedLoginSuccessPage,omitzero"`
	RawJSON                   json.RawMessage         `json:"-"`
}

var loginAccountParamsSpec = objectSpec{
	fields: []objectField{
		{name: "accessKeyId", flags: fieldNonNullOptional},
		{name: "accessToken", flags: fieldNonNullOptional},
		{name: "apiKey", flags: fieldNonNullOptional},
		{name: "appBrand"},
		{name: "chatgptAccountId", flags: fieldNonNullOptional},
		{name: "chatgptPlanType"},
		{name: "codexStreamlinedLogin", flags: fieldNonNullOptional, defaultJSON: "false", defaultTags: []string{"\"chatgpt\""}},
		{name: "region", flags: fieldNonNullOptional},
		{name: "secretAccessKey", flags: fieldNonNullOptional},
		{name: "sessionToken"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "useHostedLoginSuccessPage", flags: fieldNonNullOptional},
	},
	tag:  10,
	tags: []string{"apiKey", "chatgpt", "chatgptDeviceCode", "chatgptAuthTokens", "amazonBedrock", "amazonBedrockAccessKeys"},
}

func (v LoginAccountParams) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "apiKey":
		if !v.APIKey.IsSet() {
			return nil, DecodeError{Field: "apiKey", Reason: "missing required field for type apiKey"}
		}
	case "chatgpt":
	case "chatgptDeviceCode":
	case "chatgptAuthTokens":
		if !v.AccessToken.IsSet() {
			return nil, DecodeError{Field: "accessToken", Reason: "missing required field for type chatgptAuthTokens"}
		}
		if !v.ChatgptAccountID.IsSet() {
			return nil, DecodeError{Field: "chatgptAccountId", Reason: "missing required field for type chatgptAuthTokens"}
		}
	case "amazonBedrock":
		if !v.APIKey.IsSet() {
			return nil, DecodeError{Field: "apiKey", Reason: "missing required field for type amazonBedrock"}
		}
		if !v.Region.IsSet() {
			return nil, DecodeError{Field: "region", Reason: "missing required field for type amazonBedrock"}
		}
	case "amazonBedrockAccessKeys":
		if !v.AccessKeyID.IsSet() {
			return nil, DecodeError{Field: "accessKeyId", Reason: "missing required field for type amazonBedrockAccessKeys"}
		}
		if !v.Region.IsSet() {
			return nil, DecodeError{Field: "region", Reason: "missing required field for type amazonBedrockAccessKeys"}
		}
		if !v.SecretAccessKey.IsSet() {
			return nil, DecodeError{Field: "secretAccessKey", Reason: "missing required field for type amazonBedrockAccessKeys"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire LoginAccountParams
	w := wire(v)
	w.CodexStreamlinedLogin = nonNullWithoutFalse(w.CodexStreamlinedLogin)
	return json.Marshal(w)
}

func (v *LoginAccountParams) UnmarshalJSON(data []byte) error {
	type wire LoginAccountParams
	var s objectScan
	known, err := s.begin(data, &loginAccountParamsSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "apiKey":
		if !s.has("apiKey") {
			return DecodeError{Field: "apiKey", Reason: "missing required field for type apiKey"}
		}
		if s.isNull("apiKey") {
			return DecodeError{Field: "apiKey", Reason: "cannot be null"}
		}
	case "chatgpt":
	case "chatgptDeviceCode":
	case "chatgptAuthTokens":
		if !s.has("accessToken") {
			return DecodeError{Field: "accessToken", Reason: "missing required field for type chatgptAuthTokens"}
		}
		if s.isNull("accessToken") {
			return DecodeError{Field: "accessToken", Reason: "cannot be null"}
		}
		if !s.has("chatgptAccountId") {
			return DecodeError{Field: "chatgptAccountId", Reason: "missing required field for type chatgptAuthTokens"}
		}
		if s.isNull("chatgptAccountId") {
			return DecodeError{Field: "chatgptAccountId", Reason: "cannot be null"}
		}
	case "amazonBedrock":
		if !s.has("apiKey") {
			return DecodeError{Field: "apiKey", Reason: "missing required field for type amazonBedrock"}
		}
		if s.isNull("apiKey") {
			return DecodeError{Field: "apiKey", Reason: "cannot be null"}
		}
		if !s.has("region") {
			return DecodeError{Field: "region", Reason: "missing required field for type amazonBedrock"}
		}
		if s.isNull("region") {
			return DecodeError{Field: "region", Reason: "cannot be null"}
		}
	case "amazonBedrockAccessKeys":
		if !s.has("accessKeyId") {
			return DecodeError{Field: "accessKeyId", Reason: "missing required field for type amazonBedrockAccessKeys"}
		}
		if s.isNull("accessKeyId") {
			return DecodeError{Field: "accessKeyId", Reason: "cannot be null"}
		}
		if !s.has("region") {
			return DecodeError{Field: "region", Reason: "missing required field for type amazonBedrockAccessKeys"}
		}
		if s.isNull("region") {
			return DecodeError{Field: "region", Reason: "cannot be null"}
		}
		if !s.has("secretAccessKey") {
			return DecodeError{Field: "secretAccessKey", Reason: "missing required field for type amazonBedrockAccessKeys"}
		}
		if s.isNull("secretAccessKey") {
			return DecodeError{Field: "secretAccessKey", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type LoginAccountResponse struct {
	AuthURL         OptionalNonNull[string] `json:"authUrl,omitzero"`
	LoginID         OptionalNonNull[string] `json:"loginId,omitzero"`
	TypeValue       string                  `json:"type"`
	UserCode        OptionalNonNull[string] `json:"userCode,omitzero"`
	VerificationURL OptionalNonNull[string] `json:"verificationUrl,omitzero"`
	RawJSON         json.RawMessage         `json:"-"`
}

var loginAccountResponseSpec = objectSpec{
	fields: []objectField{
		{name: "authUrl", flags: fieldNonNullOptional},
		{name: "loginId", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "userCode", flags: fieldNonNullOptional},
		{name: "verificationUrl", flags: fieldNonNullOptional},
	},
	tag:  2,
	tags: []string{"apiKey", "chatgpt", "chatgptDeviceCode", "chatgptAuthTokens", "amazonBedrock"},
}

func (v LoginAccountResponse) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "apiKey":
	case "chatgpt":
		if !v.AuthURL.IsSet() {
			return nil, DecodeError{Field: "authUrl", Reason: "missing required field for type chatgpt"}
		}
		if !v.LoginID.IsSet() {
			return nil, DecodeError{Field: "loginId", Reason: "missing required field for type chatgpt"}
		}
	case "chatgptDeviceCode":
		if !v.LoginID.IsSet() {
			return nil, DecodeError{Field: "loginId", Reason: "missing required field for type chatgptDeviceCode"}
		}
		if !v.UserCode.IsSet() {
			return nil, DecodeError{Field: "userCode", Reason: "missing required field for type chatgptDeviceCode"}
		}
		if !v.VerificationURL.IsSet() {
			return nil, DecodeError{Field: "verificationUrl", Reason: "missing required field for type chatgptDeviceCode"}
		}
	case "chatgptAuthTokens":
	case "amazonBedrock":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire LoginAccountResponse
	return json.Marshal(wire(v))
}

func (v *LoginAccountResponse) UnmarshalJSON(data []byte) error {
	type wire LoginAccountResponse
	var s objectScan
	known, err := s.begin(data, &loginAccountResponseSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "apiKey":
	case "chatgpt":
		if !s.has("authUrl") {
			return DecodeError{Field: "authUrl", Reason: "missing required field for type chatgpt"}
		}
		if s.isNull("authUrl") {
			return DecodeError{Field: "authUrl", Reason: "cannot be null"}
		}
		if !s.has("loginId") {
			return DecodeError{Field: "loginId", Reason: "missing required field for type chatgpt"}
		}
		if s.isNull("loginId") {
			return DecodeError{Field: "loginId", Reason: "cannot be null"}
		}
	case "chatgptDeviceCode":
		if !s.has("loginId") {
			return DecodeError{Field: "loginId", Reason: "missing required field for type chatgptDeviceCode"}
		}
		if s.isNull("loginId") {
			return DecodeError{Field: "loginId", Reason: "cannot be null"}
		}
		if !s.has("userCode") {
			return DecodeError{Field: "userCode", Reason: "missing required field for type chatgptDeviceCode"}
		}
		if s.isNull("userCode") {
			return DecodeError{Field: "userCode", Reason: "cannot be null"}
		}
		if !s.has("verificationUrl") {
			return DecodeError{Field: "verificationUrl", Reason: "missing required field for type chatgptDeviceCode"}
		}
		if s.isNull("verificationUrl") {
			return DecodeError{Field: "verificationUrl", Reason: "cannot be null"}
		}
	case "chatgptAuthTokens":
	case "amazonBedrock":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type LoginAppBrand string

const (
	LoginAppBrandCodex   LoginAppBrand = "codex"
	LoginAppBrandChatgpt LoginAppBrand = "chatgpt"
)

type LogoutAccountResponse map[string]json.RawMessage

func (v LogoutAccountResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *LogoutAccountResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = LogoutAccountResponse(raw)
	return nil
}

type ManagedHooksRequirements struct {
	Interrupt         OptionalNonNull[[]ConfiguredHookMatcherGroup] `json:"Interrupt,omitzero"`
	PermissionRequest []ConfiguredHookMatcherGroup                  `json:"PermissionRequest"`
	PostCompact       []ConfiguredHookMatcherGroup                  `json:"PostCompact"`
	PostModelRequest  OptionalNonNull[[]ConfiguredHookMatcherGroup] `json:"PostModelRequest,omitzero"`
	PostToolUse       []ConfiguredHookMatcherGroup                  `json:"PostToolUse"`
	PreCompact        []ConfiguredHookMatcherGroup                  `json:"PreCompact"`
	PreModelRequest   OptionalNonNull[[]ConfiguredHookMatcherGroup] `json:"PreModelRequest,omitzero"`
	PreToolUse        []ConfiguredHookMatcherGroup                  `json:"PreToolUse"`
	SessionEnd        OptionalNonNull[[]ConfiguredHookMatcherGroup] `json:"SessionEnd,omitzero"`
	SessionStart      []ConfiguredHookMatcherGroup                  `json:"SessionStart"`
	Stop              []ConfiguredHookMatcherGroup                  `json:"Stop"`
	SubagentStart     []ConfiguredHookMatcherGroup                  `json:"SubagentStart"`
	SubagentStop      []ConfiguredHookMatcherGroup                  `json:"SubagentStop"`
	UserPromptSubmit  []ConfiguredHookMatcherGroup                  `json:"UserPromptSubmit"`
	ManagedDir        Optional[string]                              `json:"managedDir,omitzero"`
	WindowsManagedDir Optional[string]                              `json:"windowsManagedDir,omitzero"`
}

var managedHooksRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "Interrupt", flags: fieldNonNullOptional},
		{name: "PermissionRequest", flags: fieldRequired | fieldRejectNull},
		{name: "PostCompact", flags: fieldRequired | fieldRejectNull},
		{name: "PostModelRequest", flags: fieldNonNullOptional},
		{name: "PostToolUse", flags: fieldRequired | fieldRejectNull},
		{name: "PreCompact", flags: fieldRequired | fieldRejectNull},
		{name: "PreModelRequest", flags: fieldNonNullOptional},
		{name: "PreToolUse", flags: fieldRequired | fieldRejectNull},
		{name: "SessionEnd", flags: fieldNonNullOptional},
		{name: "SessionStart", flags: fieldRequired | fieldRejectNull},
		{name: "Stop", flags: fieldRequired | fieldRejectNull},
		{name: "SubagentStart", flags: fieldRequired | fieldRejectNull},
		{name: "SubagentStop", flags: fieldRequired | fieldRejectNull},
		{name: "UserPromptSubmit", flags: fieldRequired | fieldRejectNull},
		{name: "managedDir"},
		{name: "windowsManagedDir"},
	},
}

func (v ManagedHooksRequirements) MarshalJSON() ([]byte, error) {
	type wire ManagedHooksRequirements
	return json.Marshal(wire(v))
}

func (v *ManagedHooksRequirements) UnmarshalJSON(data []byte) error {
	type wire ManagedHooksRequirements
	return decodeObject(data, &managedHooksRequirementsSpec, (*wire)(v))
}

type MarketplaceAddParams struct {
	RefName     Optional[string]   `json:"refName,omitzero"`
	Source      string             `json:"source"`
	SparsePaths Optional[[]string] `json:"sparsePaths,omitzero"`
}

var marketplaceAddParamsSpec = objectSpec{
	fields: []objectField{
		{name: "refName"},
		{name: "source", flags: fieldRequired | fieldRejectNull},
		{name: "sparsePaths"},
	},
}

func (v MarketplaceAddParams) MarshalJSON() ([]byte, error) {
	type wire MarketplaceAddParams
	return json.Marshal(wire(v))
}

func (v *MarketplaceAddParams) UnmarshalJSON(data []byte) error {
	type wire MarketplaceAddParams
	return decodeObject(data, &marketplaceAddParamsSpec, (*wire)(v))
}

type MarketplaceAddResponse struct {
	AlreadyAdded    bool            `json:"alreadyAdded"`
	InstalledRoot   AbsolutePathBuf `json:"installedRoot"`
	MarketplaceName string          `json:"marketplaceName"`
}

var marketplaceAddResponseSpec = objectSpec{
	fields: []objectField{
		{name: "alreadyAdded", flags: fieldRequired | fieldRejectNull},
		{name: "installedRoot", flags: fieldRequired | fieldRejectNull},
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MarketplaceAddResponse) MarshalJSON() ([]byte, error) {
	type wire MarketplaceAddResponse
	return json.Marshal(wire(v))
}

func (v *MarketplaceAddResponse) UnmarshalJSON(data []byte) error {
	type wire MarketplaceAddResponse
	return decodeObject(data, &marketplaceAddResponseSpec, (*wire)(v))
}

type MarketplaceInterface struct {
	DisplayName Optional[string] `json:"displayName,omitzero"`
}

var marketplaceInterfaceSpec = objectSpec{
	fields: []objectField{
		{name: "displayName"},
	},
}

func (v MarketplaceInterface) MarshalJSON() ([]byte, error) {
	type wire MarketplaceInterface
	return json.Marshal(wire(v))
}

func (v *MarketplaceInterface) UnmarshalJSON(data []byte) error {
	type wire MarketplaceInterface
	return decodeObject(data, &marketplaceInterfaceSpec, (*wire)(v))
}

type MarketplaceLoadErrorInfo struct {
	MarketplacePath AbsolutePathBuf `json:"marketplacePath"`
	Message         string          `json:"message"`
}

var marketplaceLoadErrorInfoSpec = objectSpec{
	fields: []objectField{
		{name: "marketplacePath", flags: fieldRequired | fieldRejectNull},
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MarketplaceLoadErrorInfo) MarshalJSON() ([]byte, error) {
	type wire MarketplaceLoadErrorInfo
	return json.Marshal(wire(v))
}

func (v *MarketplaceLoadErrorInfo) UnmarshalJSON(data []byte) error {
	type wire MarketplaceLoadErrorInfo
	return decodeObject(data, &marketplaceLoadErrorInfoSpec, (*wire)(v))
}

type MarketplaceRemoveParams struct {
	MarketplaceName string `json:"marketplaceName"`
}

var marketplaceRemoveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MarketplaceRemoveParams) MarshalJSON() ([]byte, error) {
	type wire MarketplaceRemoveParams
	return json.Marshal(wire(v))
}

func (v *MarketplaceRemoveParams) UnmarshalJSON(data []byte) error {
	type wire MarketplaceRemoveParams
	return decodeObject(data, &marketplaceRemoveParamsSpec, (*wire)(v))
}

type MarketplaceRemoveResponse struct {
	InstalledRoot   Optional[AbsolutePathBuf] `json:"installedRoot,omitzero"`
	MarketplaceName string                    `json:"marketplaceName"`
}

var marketplaceRemoveResponseSpec = objectSpec{
	fields: []objectField{
		{name: "installedRoot"},
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MarketplaceRemoveResponse) MarshalJSON() ([]byte, error) {
	type wire MarketplaceRemoveResponse
	return json.Marshal(wire(v))
}

func (v *MarketplaceRemoveResponse) UnmarshalJSON(data []byte) error {
	type wire MarketplaceRemoveResponse
	return decodeObject(data, &marketplaceRemoveResponseSpec, (*wire)(v))
}

type MarketplaceUpgradeErrorInfo struct {
	MarketplaceName string `json:"marketplaceName"`
	Message         string `json:"message"`
}

var marketplaceUpgradeErrorInfoSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MarketplaceUpgradeErrorInfo) MarshalJSON() ([]byte, error) {
	type wire MarketplaceUpgradeErrorInfo
	return json.Marshal(wire(v))
}

func (v *MarketplaceUpgradeErrorInfo) UnmarshalJSON(data []byte) error {
	type wire MarketplaceUpgradeErrorInfo
	return decodeObject(data, &marketplaceUpgradeErrorInfoSpec, (*wire)(v))
}

type MarketplaceUpgradeParams struct {
	MarketplaceName Optional[string] `json:"marketplaceName,omitzero"`
}

var marketplaceUpgradeParamsSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceName"},
	},
}

func (v MarketplaceUpgradeParams) MarshalJSON() ([]byte, error) {
	type wire MarketplaceUpgradeParams
	return json.Marshal(wire(v))
}

func (v *MarketplaceUpgradeParams) UnmarshalJSON(data []byte) error {
	type wire MarketplaceUpgradeParams
	return decodeObject(data, &marketplaceUpgradeParamsSpec, (*wire)(v))
}

type MarketplaceUpgradeResponse struct {
	Errors               []MarketplaceUpgradeErrorInfo `json:"errors"`
	SelectedMarketplaces []string                      `json:"selectedMarketplaces"`
	UpgradedRoots        []AbsolutePathBuf             `json:"upgradedRoots"`
}

var marketplaceUpgradeResponseSpec = objectSpec{
	fields: []objectField{
		{name: "errors", flags: fieldRequired | fieldRejectNull},
		{name: "selectedMarketplaces", flags: fieldRequired | fieldRejectNull},
		{name: "upgradedRoots", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MarketplaceUpgradeResponse) MarshalJSON() ([]byte, error) {
	type wire MarketplaceUpgradeResponse
	return json.Marshal(wire(v))
}

func (v *MarketplaceUpgradeResponse) UnmarshalJSON(data []byte) error {
	type wire MarketplaceUpgradeResponse
	return decodeObject(data, &marketplaceUpgradeResponseSpec, (*wire)(v))
}

type McpAppDisplayMode string

const (
	McpAppDisplayModeInline     McpAppDisplayMode = "inline"
	McpAppDisplayModeFullscreen McpAppDisplayMode = "fullscreen"
)

type McpAppUi struct {
	PreferredModelDisplayMode McpAppDisplayMode `json:"preferredModelDisplayMode"`
	ResourceUri               string            `json:"resourceUri"`
}

var mcpAppUiSpec = objectSpec{
	fields: []objectField{
		{name: "preferredModelDisplayMode", flags: fieldRequired | fieldRejectNull},
		{name: "resourceUri", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpAppUi) MarshalJSON() ([]byte, error) {
	type wire McpAppUi
	return json.Marshal(wire(v))
}

func (v *McpAppUi) UnmarshalJSON(data []byte) error {
	type wire McpAppUi
	return decodeObject(data, &mcpAppUiSpec, (*wire)(v))
}

type McpAuthStatus string

const (
	McpAuthStatusUnknown     McpAuthStatus = "unknown"
	McpAuthStatusUnsupported McpAuthStatus = "unsupported"
	McpAuthStatusNotLoggedIn McpAuthStatus = "notLoggedIn"
	McpAuthStatusBearerToken McpAuthStatus = "bearerToken"
	McpAuthStatusOAuth       McpAuthStatus = "oAuth"
)

type McpResourceReadParams struct {
	ConnectorID  Optional[string]                `json:"connectorId,omitzero"`
	OriginCallID Optional[string]                `json:"originCallId,omitzero"`
	Server       string                          `json:"server"`
	Target       Optional[McpResourceReadTarget] `json:"target,omitzero"`
	ThreadID     Optional[string]                `json:"threadId,omitzero"`
	Uri          string                          `json:"uri"`
}

var mcpResourceReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "connectorId"},
		{name: "originCallId"},
		{name: "server", flags: fieldRequired | fieldRejectNull},
		{name: "target"},
		{name: "threadId"},
		{name: "uri", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpResourceReadParams) MarshalJSON() ([]byte, error) {
	type wire McpResourceReadParams
	return json.Marshal(wire(v))
}

func (v *McpResourceReadParams) UnmarshalJSON(data []byte) error {
	type wire McpResourceReadParams
	return decodeObject(data, &mcpResourceReadParamsSpec, (*wire)(v))
}

type McpResourceReadResponse struct {
	Contents     []ResourceContent `json:"contents"`
	OriginCallID Optional[string]  `json:"originCallId,omitzero"`
}

var mcpResourceReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "contents", flags: fieldRequired | fieldRejectNull},
		{name: "originCallId"},
	},
}

func (v McpResourceReadResponse) MarshalJSON() ([]byte, error) {
	type wire McpResourceReadResponse
	return json.Marshal(wire(v))
}

func (v *McpResourceReadResponse) UnmarshalJSON(data []byte) error {
	type wire McpResourceReadResponse
	return decodeObject(data, &mcpResourceReadResponseSpec, (*wire)(v))
}

type McpResourceReadTarget struct {
	ConnectorID string           `json:"connectorId"`
	LinkID      Optional[string] `json:"linkId,omitzero"`
}

var mcpResourceReadTargetSpec = objectSpec{
	fields: []objectField{
		{name: "connectorId", flags: fieldRequired | fieldRejectNull},
		{name: "linkId", flags: fieldRequired},
	},
}

func (v McpResourceReadTarget) MarshalJSON() ([]byte, error) {
	type wire McpResourceReadTarget
	return json.Marshal(wire(v))
}

func (v *McpResourceReadTarget) UnmarshalJSON(data []byte) error {
	type wire McpResourceReadTarget
	return decodeObject(data, &mcpResourceReadTargetSpec, (*wire)(v))
}

type McpServerConnectionStatus string

const (
	McpServerConnectionStatusNotStarted             McpServerConnectionStatus = "notStarted"
	McpServerConnectionStatusStarting               McpServerConnectionStatus = "starting"
	McpServerConnectionStatusConnected              McpServerConnectionStatus = "connected"
	McpServerConnectionStatusAuthenticationRequired McpServerConnectionStatus = "authenticationRequired"
	McpServerConnectionStatusFailed                 McpServerConnectionStatus = "failed"
	McpServerConnectionStatusCancelled              McpServerConnectionStatus = "cancelled"
	McpServerConnectionStatusDisabled               McpServerConnectionStatus = "disabled"
)

type McpServerEventNotification struct {
	Method string          `json:"method"`
	Params json.RawMessage `json:"params"`
}

var mcpServerEventNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "method", flags: fieldRequired | fieldRejectNull},
		{name: "params", flags: fieldRequired},
	},
}

func (v McpServerEventNotification) MarshalJSON() ([]byte, error) {
	type wire McpServerEventNotification
	return json.Marshal(wire(v))
}

func (v *McpServerEventNotification) UnmarshalJSON(data []byte) error {
	type wire McpServerEventNotification
	return decodeObject(data, &mcpServerEventNotificationSpec, (*wire)(v))
}

type McpServerEventStreamNotification struct {
	Notification   McpServerEventNotification `json:"notification"`
	SubscriptionID string                     `json:"subscriptionId"`
}

var mcpServerEventStreamNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "notification", flags: fieldRequired | fieldRejectNull},
		{name: "subscriptionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpServerEventStreamNotification) MarshalJSON() ([]byte, error) {
	type wire McpServerEventStreamNotification
	return json.Marshal(wire(v))
}

func (v *McpServerEventStreamNotification) UnmarshalJSON(data []byte) error {
	type wire McpServerEventStreamNotification
	return decodeObject(data, &mcpServerEventStreamNotificationSpec, (*wire)(v))
}

type McpServerEventStreamStartParams struct {
	Meta           json.RawMessage `json:"_meta,omitempty"`
	Arguments      json.RawMessage `json:"arguments"`
	Name           string          `json:"name"`
	Server         string          `json:"server"`
	SubscriptionID string          `json:"subscriptionId"`
	ThreadID       string          `json:"threadId"`
}

var mcpServerEventStreamStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "arguments", flags: fieldRequired},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "server", flags: fieldRequired | fieldRejectNull},
		{name: "subscriptionId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpServerEventStreamStartParams) MarshalJSON() ([]byte, error) {
	type wire McpServerEventStreamStartParams
	return json.Marshal(wire(v))
}

func (v *McpServerEventStreamStartParams) UnmarshalJSON(data []byte) error {
	type wire McpServerEventStreamStartParams
	return decodeObject(data, &mcpServerEventStreamStartParamsSpec, (*wire)(v))
}

type McpServerEventStreamStartResponse map[string]json.RawMessage

func (v McpServerEventStreamStartResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *McpServerEventStreamStartResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = McpServerEventStreamStartResponse(raw)
	return nil
}

type McpServerEventStreamStopParams struct {
	SubscriptionID string `json:"subscriptionId"`
}

var mcpServerEventStreamStopParamsSpec = objectSpec{
	fields: []objectField{
		{name: "subscriptionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpServerEventStreamStopParams) MarshalJSON() ([]byte, error) {
	type wire McpServerEventStreamStopParams
	return json.Marshal(wire(v))
}

func (v *McpServerEventStreamStopParams) UnmarshalJSON(data []byte) error {
	type wire McpServerEventStreamStopParams
	return decodeObject(data, &mcpServerEventStreamStopParamsSpec, (*wire)(v))
}

type McpServerEventStreamStopResponse map[string]json.RawMessage

func (v McpServerEventStreamStopResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *McpServerEventStreamStopResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = McpServerEventStreamStopResponse(raw)
	return nil
}

type McpServerInfo struct {
	Description Optional[string]            `json:"description,omitzero"`
	Icons       Optional[[]json.RawMessage] `json:"icons,omitzero"`
	Name        string                      `json:"name"`
	Title       Optional[string]            `json:"title,omitzero"`
	Version     string                      `json:"version"`
	WebsiteURL  Optional[string]            `json:"websiteUrl,omitzero"`
}

var mcpServerInfoSpec = objectSpec{
	fields: []objectField{
		{name: "description"},
		{name: "icons"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
		{name: "version", flags: fieldRequired | fieldRejectNull},
		{name: "websiteUrl"},
	},
}

func (v McpServerInfo) MarshalJSON() ([]byte, error) {
	type wire McpServerInfo
	return json.Marshal(wire(v))
}

func (v *McpServerInfo) UnmarshalJSON(data []byte) error {
	type wire McpServerInfo
	return decodeObject(data, &mcpServerInfoSpec, (*wire)(v))
}

type McpServerMigration struct {
	Name string `json:"name"`
}

var mcpServerMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpServerMigration) MarshalJSON() ([]byte, error) {
	type wire McpServerMigration
	return json.Marshal(wire(v))
}

func (v *McpServerMigration) UnmarshalJSON(data []byte) error {
	type wire McpServerMigration
	return decodeObject(data, &mcpServerMigrationSpec, (*wire)(v))
}

type McpServerOauthClientRegistration string

const (
	McpServerOauthClientRegistrationAuto McpServerOauthClientRegistration = "auto"
	McpServerOauthClientRegistrationCimd McpServerOauthClientRegistration = "cimd"
	McpServerOauthClientRegistrationDcr  McpServerOauthClientRegistration = "dcr"
)

type McpServerOauthLoginCompletedNotification struct {
	Error    Optional[string] `json:"error,omitzero"`
	LoginID  Optional[string] `json:"loginId,omitzero"`
	Name     string           `json:"name"`
	Success  bool             `json:"success"`
	ThreadID Optional[string] `json:"threadId,omitzero"`
}

var mcpServerOauthLoginCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "error"},
		{name: "loginId"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "success", flags: fieldRequired | fieldRejectNull},
		{name: "threadId"},
	},
}

func (v McpServerOauthLoginCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire McpServerOauthLoginCompletedNotification
	w := wire(v)
	w.Error = w.Error.withoutNull()
	return json.Marshal(w)
}

func (v *McpServerOauthLoginCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire McpServerOauthLoginCompletedNotification
	return decodeObject(data, &mcpServerOauthLoginCompletedNotificationSpec, (*wire)(v))
}

type McpServerOauthLoginParams struct {
	ClientRegistration Optional[McpServerOauthClientRegistration] `json:"clientRegistration,omitzero"`
	Name               string                                     `json:"name"`
	Scopes             Optional[[]string]                         `json:"scopes,omitzero"`
	ThreadID           Optional[string]                           `json:"threadId,omitzero"`
	TimeoutSecs        Optional[int64]                            `json:"timeoutSecs,omitzero"`
}

var mcpServerOauthLoginParamsSpec = objectSpec{
	fields: []objectField{
		{name: "clientRegistration"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "scopes"},
		{name: "threadId"},
		{name: "timeoutSecs"},
	},
}

func (v McpServerOauthLoginParams) MarshalJSON() ([]byte, error) {
	type wire McpServerOauthLoginParams
	w := wire(v)
	w.Scopes = w.Scopes.withoutNull()
	w.TimeoutSecs = w.TimeoutSecs.withoutNull()
	return json.Marshal(w)
}

func (v *McpServerOauthLoginParams) UnmarshalJSON(data []byte) error {
	type wire McpServerOauthLoginParams
	return decodeObject(data, &mcpServerOauthLoginParamsSpec, (*wire)(v))
}

type McpServerOauthLoginResponse struct {
	AuthorizationURL string           `json:"authorizationUrl"`
	LoginID          Optional[string] `json:"loginId,omitzero"`
}

var mcpServerOauthLoginResponseSpec = objectSpec{
	fields: []objectField{
		{name: "authorizationUrl", flags: fieldRequired | fieldRejectNull},
		{name: "loginId", defaultJSON: "null"},
	},
}

func (v McpServerOauthLoginResponse) MarshalJSON() ([]byte, error) {
	type wire McpServerOauthLoginResponse
	return json.Marshal(wire(v))
}

func (v *McpServerOauthLoginResponse) UnmarshalJSON(data []byte) error {
	type wire McpServerOauthLoginResponse
	return decodeObject(data, &mcpServerOauthLoginResponseSpec, (*wire)(v))
}

type McpServerRefreshResponse map[string]json.RawMessage

func (v McpServerRefreshResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *McpServerRefreshResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = McpServerRefreshResponse(raw)
	return nil
}

type McpServerStartupFailureReason string

const (
	McpServerStartupFailureReasonReauthenticationRequired McpServerStartupFailureReason = "reauthenticationRequired"
)

type McpServerStartupState string

const (
	McpServerStartupStateStarting  McpServerStartupState = "starting"
	McpServerStartupStateReady     McpServerStartupState = "ready"
	McpServerStartupStateFailed    McpServerStartupState = "failed"
	McpServerStartupStateCancelled McpServerStartupState = "cancelled"
)

type McpServerStatus struct {
	AuthStatus         McpAuthStatus                       `json:"authStatus"`
	HttpOrigin         Optional[string]                    `json:"httpOrigin,omitzero"`
	Name               string                              `json:"name"`
	PluginID           Optional[string]                    `json:"pluginId,omitzero"`
	ResourceTemplates  []ResourceTemplate                  `json:"resourceTemplates"`
	Resources          []Resource                          `json:"resources"`
	RuntimeStatus      Optional[McpServerConnectionStatus] `json:"runtimeStatus,omitzero"`
	ServerCapabilities json.RawMessage                     `json:"serverCapabilities,omitempty"`
	ServerInfo         Optional[McpServerInfo]             `json:"serverInfo,omitzero"`
	ToolListMeta       json.RawMessage                     `json:"toolListMeta,omitempty"`
	Tools              map[string]Tool                     `json:"tools"`
	ToolsError         Optional[string]                    `json:"toolsError,omitzero"`
}

var mcpServerStatusSpec = objectSpec{
	fields: []objectField{
		{name: "authStatus", flags: fieldRequired | fieldRejectNull},
		{name: "httpOrigin"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "pluginId"},
		{name: "resourceTemplates", flags: fieldRequired | fieldRejectNull},
		{name: "resources", flags: fieldRequired | fieldRejectNull},
		{name: "runtimeStatus"},
		{name: "serverCapabilities"},
		{name: "serverInfo"},
		{name: "toolListMeta"},
		{name: "tools", flags: fieldRequired | fieldRejectNull},
		{name: "toolsError"},
	},
}

func (v McpServerStatus) MarshalJSON() ([]byte, error) {
	type wire McpServerStatus
	return json.Marshal(wire(v))
}

func (v *McpServerStatus) UnmarshalJSON(data []byte) error {
	type wire McpServerStatus
	return decodeObject(data, &mcpServerStatusSpec, (*wire)(v))
}

type McpServerStatusDetail string

const (
	McpServerStatusDetailFull             McpServerStatusDetail = "full"
	McpServerStatusDetailToolsAndAuthOnly McpServerStatusDetail = "toolsAndAuthOnly"
)

type McpServerStatusUpdatedNotification struct {
	Error         Optional[string]                        `json:"error,omitzero"`
	FailureReason Optional[McpServerStartupFailureReason] `json:"failureReason,omitzero"`
	Name          string                                  `json:"name"`
	Status        McpServerStartupState                   `json:"status"`
	ThreadID      Optional[string]                        `json:"threadId,omitzero"`
}

var mcpServerStatusUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "error"},
		{name: "failureReason"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "threadId"},
	},
}

func (v McpServerStatusUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire McpServerStatusUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *McpServerStatusUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire McpServerStatusUpdatedNotification
	return decodeObject(data, &mcpServerStatusUpdatedNotificationSpec, (*wire)(v))
}

type McpServerToolCallParams struct {
	Meta      json.RawMessage `json:"_meta,omitempty"`
	Arguments json.RawMessage `json:"arguments,omitempty"`
	Server    string          `json:"server"`
	ThreadID  string          `json:"threadId"`
	Tool      string          `json:"tool"`
}

var mcpServerToolCallParamsSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "arguments"},
		{name: "server", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "tool", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpServerToolCallParams) MarshalJSON() ([]byte, error) {
	type wire McpServerToolCallParams
	return json.Marshal(wire(v))
}

func (v *McpServerToolCallParams) UnmarshalJSON(data []byte) error {
	type wire McpServerToolCallParams
	return decodeObject(data, &mcpServerToolCallParamsSpec, (*wire)(v))
}

type McpServerToolCallResponse struct {
	Meta              json.RawMessage   `json:"_meta,omitempty"`
	Content           []json.RawMessage `json:"content"`
	IsError           Optional[bool]    `json:"isError,omitzero"`
	StructuredContent json.RawMessage   `json:"structuredContent,omitempty"`
}

var mcpServerToolCallResponseSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "content", flags: fieldRequired | fieldRejectNull},
		{name: "isError"},
		{name: "structuredContent"},
	},
}

func (v McpServerToolCallResponse) MarshalJSON() ([]byte, error) {
	type wire McpServerToolCallResponse
	w := wire(v)
	w.IsError = w.IsError.withoutNull()
	return json.Marshal(w)
}

func (v *McpServerToolCallResponse) UnmarshalJSON(data []byte) error {
	type wire McpServerToolCallResponse
	return decodeObject(data, &mcpServerToolCallResponseSpec, (*wire)(v))
}

type McpToolCallAppContext struct {
	ActionName  Optional[string] `json:"actionName,omitzero"`
	AppName     Optional[string] `json:"appName,omitzero"`
	ConnectorID string           `json:"connectorId"`
	LinkID      Optional[string] `json:"linkId,omitzero"`
	ResourceUri Optional[string] `json:"resourceUri,omitzero"`
}

var mcpToolCallAppContextSpec = objectSpec{
	fields: []objectField{
		{name: "actionName"},
		{name: "appName"},
		{name: "connectorId", flags: fieldRequired | fieldRejectNull},
		{name: "linkId"},
		{name: "resourceUri"},
	},
}

func (v McpToolCallAppContext) MarshalJSON() ([]byte, error) {
	type wire McpToolCallAppContext
	return json.Marshal(wire(v))
}

func (v *McpToolCallAppContext) UnmarshalJSON(data []byte) error {
	type wire McpToolCallAppContext
	return decodeObject(data, &mcpToolCallAppContextSpec, (*wire)(v))
}

type McpToolCallError struct {
	Message string `json:"message"`
}

var mcpToolCallErrorSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpToolCallError) MarshalJSON() ([]byte, error) {
	type wire McpToolCallError
	return json.Marshal(wire(v))
}

func (v *McpToolCallError) UnmarshalJSON(data []byte) error {
	type wire McpToolCallError
	return decodeObject(data, &mcpToolCallErrorSpec, (*wire)(v))
}

type McpToolCallProgressNotification struct {
	ItemID   string `json:"itemId"`
	Message  string `json:"message"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var mcpToolCallProgressNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v McpToolCallProgressNotification) MarshalJSON() ([]byte, error) {
	type wire McpToolCallProgressNotification
	return json.Marshal(wire(v))
}

func (v *McpToolCallProgressNotification) UnmarshalJSON(data []byte) error {
	type wire McpToolCallProgressNotification
	return decodeObject(data, &mcpToolCallProgressNotificationSpec, (*wire)(v))
}

type McpToolCallResult struct {
	Meta              json.RawMessage   `json:"_meta,omitempty"`
	Content           []json.RawMessage `json:"content"`
	StructuredContent json.RawMessage   `json:"structuredContent,omitempty"`
}

var mcpToolCallResultSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "content", flags: fieldRequired | fieldRejectNull},
		{name: "structuredContent"},
	},
}

func (v McpToolCallResult) MarshalJSON() ([]byte, error) {
	type wire McpToolCallResult
	return json.Marshal(wire(v))
}

func (v *McpToolCallResult) UnmarshalJSON(data []byte) error {
	type wire McpToolCallResult
	return decodeObject(data, &mcpToolCallResultSpec, (*wire)(v))
}

type McpToolCallStatus string

const (
	McpToolCallStatusInProgress McpToolCallStatus = "inProgress"
	McpToolCallStatusCompleted  McpToolCallStatus = "completed"
	McpToolCallStatusFailed     McpToolCallStatus = "failed"
)

type MemoryCitation struct {
	Entries   []MemoryCitationEntry `json:"entries"`
	ThreadIDs []string              `json:"threadIds"`
}

var memoryCitationSpec = objectSpec{
	fields: []objectField{
		{name: "entries", flags: fieldRequired | fieldRejectNull},
		{name: "threadIds", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MemoryCitation) MarshalJSON() ([]byte, error) {
	type wire MemoryCitation
	return json.Marshal(wire(v))
}

func (v *MemoryCitation) UnmarshalJSON(data []byte) error {
	type wire MemoryCitation
	return decodeObject(data, &memoryCitationSpec, (*wire)(v))
}

type MemoryCitationEntry struct {
	LineEnd   uint32 `json:"lineEnd"`
	LineStart uint32 `json:"lineStart"`
	Note      string `json:"note"`
	Path      string `json:"path"`
}

var memoryCitationEntrySpec = objectSpec{
	fields: []objectField{
		{name: "lineEnd", flags: fieldRequired | fieldRejectNull},
		{name: "lineStart", flags: fieldRequired | fieldRejectNull},
		{name: "note", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MemoryCitationEntry) MarshalJSON() ([]byte, error) {
	type wire MemoryCitationEntry
	return json.Marshal(wire(v))
}

func (v *MemoryCitationEntry) UnmarshalJSON(data []byte) error {
	type wire MemoryCitationEntry
	var s objectScan
	if _, err := s.begin(data, &memoryCitationEntrySpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.LineEnd < 0 {
		return DecodeError{Field: "lineEnd", Reason: "below minimum 0"}
	}
	if v.LineStart < 0 {
		return DecodeError{Field: "lineStart", Reason: "below minimum 0"}
	}
	return nil
}

type MemoryResetResponse map[string]json.RawMessage

func (v MemoryResetResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *MemoryResetResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = MemoryResetResponse(raw)
	return nil
}

type MemoryStatusParams struct {
	MinConsolidatedThreads Optional[uint32] `json:"minConsolidatedThreads,omitzero"`
}

var memoryStatusParamsSpec = objectSpec{
	fields: []objectField{
		{name: "minConsolidatedThreads"},
	},
}

func (v MemoryStatusParams) MarshalJSON() ([]byte, error) {
	type wire MemoryStatusParams
	return json.Marshal(wire(v))
}

func (v *MemoryStatusParams) UnmarshalJSON(data []byte) error {
	type wire MemoryStatusParams
	var s objectScan
	if _, err := s.begin(data, &memoryStatusParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueMinConsolidatedThreads, ok := v.MinConsolidatedThreads.Value(); ok {
		if valueMinConsolidatedThreads < 0 {
			return DecodeError{Field: "minConsolidatedThreads", Reason: "below minimum 0"}
		}
	}
	return nil
}

type MemoryStatusResponse struct {
	V2ConsolidatedThreads uint32 `json:"v2ConsolidatedThreads"`
	V2Ready               bool   `json:"v2Ready"`
}

var memoryStatusResponseSpec = objectSpec{
	fields: []objectField{
		{name: "v2ConsolidatedThreads", flags: fieldRequired | fieldRejectNull},
		{name: "v2Ready", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MemoryStatusResponse) MarshalJSON() ([]byte, error) {
	type wire MemoryStatusResponse
	return json.Marshal(wire(v))
}

func (v *MemoryStatusResponse) UnmarshalJSON(data []byte) error {
	type wire MemoryStatusResponse
	var s objectScan
	if _, err := s.begin(data, &memoryStatusResponseSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.V2ConsolidatedThreads < 0 {
		return DecodeError{Field: "v2ConsolidatedThreads", Reason: "below minimum 0"}
	}
	return nil
}

type MergeStrategy string

const (
	MergeStrategyReplace MergeStrategy = "replace"
	MergeStrategyUpsert  MergeStrategy = "upsert"
)

type MessagePhase string

const (
	MessagePhaseCommentary    MessagePhase = "commentary"
	MessagePhasePartialAnswer MessagePhase = "partial_answer"
	MessagePhaseFinalAnswer   MessagePhase = "final_answer"
)

type MigrationDetails struct {
	Commands   OptionalNonNull[[]CommandMigration]   `json:"commands,omitzero"`
	Hooks      OptionalNonNull[[]HookMigration]      `json:"hooks,omitzero"`
	McpServers OptionalNonNull[[]McpServerMigration] `json:"mcpServers,omitzero"`
	Memory     OptionalNonNull[[]string]             `json:"memory,omitzero"`
	Plugins    OptionalNonNull[[]PluginsMigration]   `json:"plugins,omitzero"`
	Sessions   OptionalNonNull[[]SessionMigration]   `json:"sessions,omitzero"`
	Skills     OptionalNonNull[[]SkillMigration]     `json:"skills,omitzero"`
	Subagents  OptionalNonNull[[]SubagentMigration]  `json:"subagents,omitzero"`
}

var migrationDetailsSpec = objectSpec{
	fields: []objectField{
		{name: "commands", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "hooks", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "mcpServers", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "memory", flags: fieldNonNullOptional},
		{name: "plugins", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "sessions", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "skills", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "subagents", flags: fieldNonNullOptional, defaultJSON: "[]"},
	},
}

func (v MigrationDetails) MarshalJSON() ([]byte, error) {
	type wire MigrationDetails
	return json.Marshal(wire(v))
}

func (v *MigrationDetails) UnmarshalJSON(data []byte) error {
	type wire MigrationDetails
	return decodeObject(data, &migrationDetailsSpec, (*wire)(v))
}

type MisalignmentErrorDetails struct {
	DetailedExplanation Optional[string]            `json:"detailedExplanation,omitzero"`
	ErrorType           Optional[string]            `json:"errorType,omitzero"`
	ReviewTarget        Optional[string]            `json:"reviewTarget,omitzero"`
	Steer               Optional[MisalignmentSteer] `json:"steer,omitzero"`
}

var misalignmentErrorDetailsSpec = objectSpec{
	fields: []objectField{
		{name: "detailedExplanation"},
		{name: "errorType"},
		{name: "reviewTarget"},
		{name: "steer"},
	},
}

func (v MisalignmentErrorDetails) MarshalJSON() ([]byte, error) {
	type wire MisalignmentErrorDetails
	return json.Marshal(wire(v))
}

func (v *MisalignmentErrorDetails) UnmarshalJSON(data []byte) error {
	type wire MisalignmentErrorDetails
	return decodeObject(data, &misalignmentErrorDetailsSpec, (*wire)(v))
}

type MisalignmentSteer struct {
	Message string `json:"message"`
}

var misalignmentSteerSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v MisalignmentSteer) MarshalJSON() ([]byte, error) {
	type wire MisalignmentSteer
	return json.Marshal(wire(v))
}

func (v *MisalignmentSteer) UnmarshalJSON(data []byte) error {
	type wire MisalignmentSteer
	return decodeObject(data, &misalignmentSteerSpec, (*wire)(v))
}

type ModeKind string

const (
	ModeKindPlan         ModeKind = "plan"
	ModeKindDefaultValue ModeKind = "default"
)

type Model struct {
	AdditionalSpeedTiers      OptionalNonNull[[]string]           `json:"additionalSpeedTiers,omitzero"`
	AvailabilityNux           Optional[ModelAvailabilityNux]      `json:"availabilityNux,omitzero"`
	AvailableAccessPrograms   Optional[ModelAccessPrograms]       `json:"availableAccessPrograms,omitzero"`
	DefaultReasoningEffort    ReasoningEffort                     `json:"defaultReasoningEffort"`
	DefaultServiceTier        Optional[string]                    `json:"defaultServiceTier,omitzero"`
	Description               string                              `json:"description"`
	DisplayName               string                              `json:"displayName"`
	Hidden                    bool                                `json:"hidden"`
	ID                        string                              `json:"id"`
	InputModalities           OptionalNonNull[[]InputModality]    `json:"inputModalities,omitzero"`
	IsDefault                 bool                                `json:"isDefault"`
	Model                     string                              `json:"model"`
	ModelSpecialty            Optional[string]                    `json:"modelSpecialty,omitzero"`
	MultiAgentVersion         Optional[MultiAgentVersion]         `json:"multiAgentVersion,omitzero"`
	ServiceTiers              OptionalNonNull[[]ModelServiceTier] `json:"serviceTiers,omitzero"`
	SupportedReasoningEfforts []ReasoningEffortOption             `json:"supportedReasoningEfforts"`
	SupportsPersonality       OptionalNonNull[bool]               `json:"supportsPersonality,omitzero"`
	Upgrade                   Optional[string]                    `json:"upgrade,omitzero"`
	UpgradeInfo               Optional[ModelUpgradeInfo]          `json:"upgradeInfo,omitzero"`
}

var modelSpec = objectSpec{
	fields: []objectField{
		{name: "additionalSpeedTiers", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "availabilityNux"},
		{name: "availableAccessPrograms", defaultJSON: "null"},
		{name: "defaultReasoningEffort", flags: fieldRequired | fieldRejectNull},
		{name: "defaultServiceTier", defaultJSON: "null"},
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "displayName", flags: fieldRequired | fieldRejectNull},
		{name: "hidden", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "inputModalities", flags: fieldNonNullOptional, defaultJSON: "[\"text\"]"},
		{name: "isDefault", flags: fieldRequired | fieldRejectNull},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "modelSpecialty"},
		{name: "multiAgentVersion"},
		{name: "serviceTiers", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "supportedReasoningEfforts", flags: fieldRequired | fieldRejectNull},
		{name: "supportsPersonality", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "upgrade"},
		{name: "upgradeInfo"},
	},
}

func (v Model) MarshalJSON() ([]byte, error) {
	type wire Model
	return json.Marshal(wire(v))
}

func (v *Model) UnmarshalJSON(data []byte) error {
	type wire Model
	return decodeObject(data, &modelSpec, (*wire)(v))
}

type ModelAccessPrograms struct {
	Cyber []CyberAccessProgram `json:"cyber"`
}

var modelAccessProgramsSpec = objectSpec{
	fields: []objectField{
		{name: "cyber", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelAccessPrograms) MarshalJSON() ([]byte, error) {
	type wire ModelAccessPrograms
	return json.Marshal(wire(v))
}

func (v *ModelAccessPrograms) UnmarshalJSON(data []byte) error {
	type wire ModelAccessPrograms
	return decodeObject(data, &modelAccessProgramsSpec, (*wire)(v))
}

type ModelAvailabilityNux struct {
	Message string `json:"message"`
}

var modelAvailabilityNuxSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelAvailabilityNux) MarshalJSON() ([]byte, error) {
	type wire ModelAvailabilityNux
	return json.Marshal(wire(v))
}

func (v *ModelAvailabilityNux) UnmarshalJSON(data []byte) error {
	type wire ModelAvailabilityNux
	return decodeObject(data, &modelAvailabilityNuxSpec, (*wire)(v))
}

type ModelListParams struct {
	Cursor        Optional[string] `json:"cursor,omitzero"`
	IncludeHidden Optional[bool]   `json:"includeHidden,omitzero"`
	Limit         Optional[uint32] `json:"limit,omitzero"`
}

var modelListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "includeHidden"},
		{name: "limit"},
	},
}

func (v ModelListParams) MarshalJSON() ([]byte, error) {
	type wire ModelListParams
	return json.Marshal(wire(v))
}

func (v *ModelListParams) UnmarshalJSON(data []byte) error {
	type wire ModelListParams
	var s objectScan
	if _, err := s.begin(data, &modelListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ModelListResponse struct {
	Data       []Model          `json:"data"`
	NextCursor Optional[string] `json:"nextCursor,omitzero"`
}

var modelListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ModelListResponse) MarshalJSON() ([]byte, error) {
	type wire ModelListResponse
	return json.Marshal(wire(v))
}

func (v *ModelListResponse) UnmarshalJSON(data []byte) error {
	type wire ModelListResponse
	return decodeObject(data, &modelListResponseSpec, (*wire)(v))
}

type ModelProviderCapabilitiesReadParams map[string]json.RawMessage

func (v ModelProviderCapabilitiesReadParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ModelProviderCapabilitiesReadParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ModelProviderCapabilitiesReadParams(raw)
	return nil
}

type ModelProviderCapabilitiesReadResponse struct {
	ImageGeneration bool `json:"imageGeneration"`
	WebSearch       bool `json:"webSearch"`
}

var modelProviderCapabilitiesReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "imageGeneration", flags: fieldRequired | fieldRejectNull},
		{name: "webSearch", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelProviderCapabilitiesReadResponse) MarshalJSON() ([]byte, error) {
	type wire ModelProviderCapabilitiesReadResponse
	return json.Marshal(wire(v))
}

func (v *ModelProviderCapabilitiesReadResponse) UnmarshalJSON(data []byte) error {
	type wire ModelProviderCapabilitiesReadResponse
	return decodeObject(data, &modelProviderCapabilitiesReadResponseSpec, (*wire)(v))
}

type ModelRerouteReason string

const (
	ModelRerouteReasonHighRiskCyberActivity ModelRerouteReason = "highRiskCyberActivity"
)

type ModelReroutedNotification struct {
	FromModel string             `json:"fromModel"`
	Reason    ModelRerouteReason `json:"reason"`
	ThreadID  string             `json:"threadId"`
	ToModel   string             `json:"toModel"`
	TurnID    string             `json:"turnId"`
}

var modelReroutedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "fromModel", flags: fieldRequired | fieldRejectNull},
		{name: "reason", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "toModel", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelReroutedNotification) MarshalJSON() ([]byte, error) {
	type wire ModelReroutedNotification
	return json.Marshal(wire(v))
}

func (v *ModelReroutedNotification) UnmarshalJSON(data []byte) error {
	type wire ModelReroutedNotification
	return decodeObject(data, &modelReroutedNotificationSpec, (*wire)(v))
}

type ModelSafetyBufferingUpdatedNotification struct {
	FasterModel     Optional[string] `json:"fasterModel,omitzero"`
	Model           string           `json:"model"`
	Reasons         []string         `json:"reasons"`
	ShowBufferingUi bool             `json:"showBufferingUi"`
	ThreadID        string           `json:"threadId"`
	TurnID          string           `json:"turnId"`
	UseCases        []string         `json:"useCases"`
}

var modelSafetyBufferingUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "fasterModel"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "reasons", flags: fieldRequired | fieldRejectNull},
		{name: "showBufferingUi", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
		{name: "useCases", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelSafetyBufferingUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ModelSafetyBufferingUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ModelSafetyBufferingUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ModelSafetyBufferingUpdatedNotification
	return decodeObject(data, &modelSafetyBufferingUpdatedNotificationSpec, (*wire)(v))
}

type ModelServiceTier struct {
	Description string `json:"description"`
	ID          string `json:"id"`
	Name        string `json:"name"`
}

var modelServiceTierSpec = objectSpec{
	fields: []objectField{
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelServiceTier) MarshalJSON() ([]byte, error) {
	type wire ModelServiceTier
	return json.Marshal(wire(v))
}

func (v *ModelServiceTier) UnmarshalJSON(data []byte) error {
	type wire ModelServiceTier
	return decodeObject(data, &modelServiceTierSpec, (*wire)(v))
}

type ModelUpgradeInfo struct {
	MigrationMarkdown Optional[string] `json:"migrationMarkdown,omitzero"`
	Model             string           `json:"model"`
	ModelLink         Optional[string] `json:"modelLink,omitzero"`
	RetirementAt      Optional[int64]  `json:"retirementAt,omitzero"`
	UpgradeCopy       Optional[string] `json:"upgradeCopy,omitzero"`
}

var modelUpgradeInfoSpec = objectSpec{
	fields: []objectField{
		{name: "migrationMarkdown"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "modelLink"},
		{name: "retirementAt"},
		{name: "upgradeCopy"},
	},
}

func (v ModelUpgradeInfo) MarshalJSON() ([]byte, error) {
	type wire ModelUpgradeInfo
	return json.Marshal(wire(v))
}

func (v *ModelUpgradeInfo) UnmarshalJSON(data []byte) error {
	type wire ModelUpgradeInfo
	return decodeObject(data, &modelUpgradeInfoSpec, (*wire)(v))
}

type ModelVerification string

const (
	ModelVerificationTrustedAccessForCyber ModelVerification = "trustedAccessForCyber"
)

type ModelVerificationNotification struct {
	ThreadID      string              `json:"threadId"`
	TurnID        string              `json:"turnId"`
	Verifications []ModelVerification `json:"verifications"`
}

var modelVerificationNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
		{name: "verifications", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ModelVerificationNotification) MarshalJSON() ([]byte, error) {
	type wire ModelVerificationNotification
	return json.Marshal(wire(v))
}

func (v *ModelVerificationNotification) UnmarshalJSON(data []byte) error {
	type wire ModelVerificationNotification
	return decodeObject(data, &modelVerificationNotificationSpec, (*wire)(v))
}

type ModelsRequirements struct {
	NewThread Optional[NewThreadModelDefaults] `json:"newThread,omitzero"`
}

var modelsRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "newThread"},
	},
}

func (v ModelsRequirements) MarshalJSON() ([]byte, error) {
	type wire ModelsRequirements
	return json.Marshal(wire(v))
}

func (v *ModelsRequirements) UnmarshalJSON(data []byte) error {
	type wire ModelsRequirements
	return decodeObject(data, &modelsRequirementsSpec, (*wire)(v))
}

type MultiAgentMode struct {
	Mode    string                  `json:"-"`
	Custom  OptionalNonNull[string] `json:"custom,omitempty"`
	RawJSON json.RawMessage         `json:"-"`
}

var (
	MultiAgentModeExplicitRequestOnly = MultiAgentMode{Mode: "explicitRequestOnly"}
	MultiAgentModeProactive           = MultiAgentMode{Mode: "proactive"}
)

func CustomMultiAgentMode(custom string) MultiAgentMode {
	return MultiAgentMode{Custom: SomeNonNull(custom)}
}

func (v MultiAgentMode) IsSet() bool {
	return v.Mode != "" || v.Custom.IsSet() || len(bytes.TrimSpace(v.RawJSON)) > 0
}

func (v MultiAgentMode) MarshalJSON() ([]byte, error) {
	modeSet := v.Mode != ""
	customSet := v.Custom.IsSet()
	rawSet := len(bytes.TrimSpace(v.RawJSON)) > 0
	matches := 0
	if modeSet {
		matches++
	}
	if customSet {
		matches++
	}
	if rawSet {
		matches++
	}
	if matches == 0 {
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if matches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	if modeSet {
		switch v.Mode {
		case "explicitRequestOnly", "proactive":
			return json.Marshal(v.Mode)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported multiAgentMode value %q", v.Mode)}
		}
	}
	if customSet {
		return json.Marshal(map[string]any{"custom": v.Custom})
	}
	if !json.Valid(v.RawJSON) {
		return nil, fmt.Errorf("invalid MultiAgentMode raw fallback")
	}
	return append([]byte(nil), v.RawJSON...), nil
}

func (v *MultiAgentMode) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var mode string
	if err := json.Unmarshal(trimmed, &mode); err == nil {
		switch mode {
		case "explicitRequestOnly", "proactive":
			*v = MultiAgentMode{Mode: mode}
		default:
			v.Mode = ""
			v.Custom = OptionalNonNull[string]{}
			v.RawJSON = append(v.RawJSON[:0], data...)
		}
		return nil
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(trimmed, &raw); err != nil {
		return err
	}
	rawCustom, ok := raw["custom"]
	if !ok {
		v.Mode = ""
		v.Custom = OptionalNonNull[string]{}
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	var custom OptionalNonNull[string]
	if err := json.Unmarshal(rawCustom, &custom); err != nil {
		return fmt.Errorf("field custom: %w", err)
	}
	*v = MultiAgentMode{Custom: custom}
	return nil
}

type MultiAgentVersion string

const (
	MultiAgentVersionDisabled MultiAgentVersion = "disabled"
	MultiAgentVersionV1       MultiAgentVersion = "v1"
	MultiAgentVersionV2       MultiAgentVersion = "v2"
)

type NetworkAccess string

const (
	NetworkAccessRestricted NetworkAccess = "restricted"
	NetworkAccessEnabled    NetworkAccess = "enabled"
)

type NetworkApprovalProtocol string

const (
	NetworkApprovalProtocolHttp      NetworkApprovalProtocol = "http"
	NetworkApprovalProtocolHttps     NetworkApprovalProtocol = "https"
	NetworkApprovalProtocolSocks5tcp NetworkApprovalProtocol = "socks5Tcp"
	NetworkApprovalProtocolSocks5udp NetworkApprovalProtocol = "socks5Udp"
)

type NetworkDomainPermission string

const (
	NetworkDomainPermissionAllow NetworkDomainPermission = "allow"
	NetworkDomainPermissionDeny  NetworkDomainPermission = "deny"
)

type NetworkRequirements struct {
	AllowLocalBinding                Optional[bool]                                   `json:"allowLocalBinding,omitzero"`
	AllowUnixSockets                 Optional[[]string]                               `json:"allowUnixSockets,omitzero"`
	AllowUpstreamProxy               Optional[bool]                                   `json:"allowUpstreamProxy,omitzero"`
	AllowedDomains                   Optional[[]string]                               `json:"allowedDomains,omitzero"`
	DangerouslyAllowAllUnixSockets   Optional[bool]                                   `json:"dangerouslyAllowAllUnixSockets,omitzero"`
	DangerouslyAllowNonLoopbackProxy Optional[bool]                                   `json:"dangerouslyAllowNonLoopbackProxy,omitzero"`
	DeniedDomains                    Optional[[]string]                               `json:"deniedDomains,omitzero"`
	Domains                          Optional[map[string]NetworkDomainPermission]     `json:"domains,omitzero"`
	Enabled                          Optional[bool]                                   `json:"enabled,omitzero"`
	HttpPort                         Optional[uint16]                                 `json:"httpPort,omitzero"`
	ManagedAllowedDomainsOnly        Optional[bool]                                   `json:"managedAllowedDomainsOnly,omitzero"`
	ReviewDomains                    Optional[[]string]                               `json:"reviewDomains,omitzero"`
	SocksPort                        Optional[uint16]                                 `json:"socksPort,omitzero"`
	UnixSockets                      Optional[map[string]NetworkUnixSocketPermission] `json:"unixSockets,omitzero"`
}

var networkRequirementsSpec = objectSpec{
	fields: []objectField{
		{name: "allowLocalBinding"},
		{name: "allowUnixSockets"},
		{name: "allowUpstreamProxy"},
		{name: "allowedDomains"},
		{name: "dangerouslyAllowAllUnixSockets"},
		{name: "dangerouslyAllowNonLoopbackProxy"},
		{name: "deniedDomains"},
		{name: "domains"},
		{name: "enabled"},
		{name: "httpPort"},
		{name: "managedAllowedDomainsOnly"},
		{name: "reviewDomains"},
		{name: "socksPort"},
		{name: "unixSockets"},
	},
}

func (v NetworkRequirements) MarshalJSON() ([]byte, error) {
	type wire NetworkRequirements
	return json.Marshal(wire(v))
}

func (v *NetworkRequirements) UnmarshalJSON(data []byte) error {
	type wire NetworkRequirements
	var s objectScan
	if _, err := s.begin(data, &networkRequirementsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueHttpPort, ok := v.HttpPort.Value(); ok {
		if valueHttpPort < 0 {
			return DecodeError{Field: "httpPort", Reason: "below minimum 0"}
		}
	}
	if valueSocksPort, ok := v.SocksPort.Value(); ok {
		if valueSocksPort < 0 {
			return DecodeError{Field: "socksPort", Reason: "below minimum 0"}
		}
	}
	return nil
}

type NetworkUnixSocketPermission string

const (
	NetworkUnixSocketPermissionAllow NetworkUnixSocketPermission = "allow"
	NetworkUnixSocketPermissionDeny  NetworkUnixSocketPermission = "deny"
)

type NewThreadModelDefaults struct {
	Model                Optional[string]          `json:"model,omitzero"`
	ModelReasoningEffort Optional[ReasoningEffort] `json:"modelReasoningEffort,omitzero"`
	ServiceTier          Optional[string]          `json:"serviceTier,omitzero"`
}

var newThreadModelDefaultsSpec = objectSpec{
	fields: []objectField{
		{name: "model"},
		{name: "modelReasoningEffort"},
		{name: "serviceTier"},
	},
}

func (v NewThreadModelDefaults) MarshalJSON() ([]byte, error) {
	type wire NewThreadModelDefaults
	return json.Marshal(wire(v))
}

func (v *NewThreadModelDefaults) UnmarshalJSON(data []byte) error {
	type wire NewThreadModelDefaults
	return decodeObject(data, &newThreadModelDefaultsSpec, (*wire)(v))
}

type NonSteerableTurnKind string

const (
	NonSteerableTurnKindReview  NonSteerableTurnKind = "review"
	NonSteerableTurnKindCompact NonSteerableTurnKind = "compact"
)

type NullableGetAccountRateLimitsParams = Optional[GetAccountRateLimitsParams]

type NullableGetAccountTokenUsageParams = Optional[GetAccountTokenUsageParams]

type NullableRemoteControlDisableParams = Optional[RemoteControlDisableParams]

type NullableRemoteControlEnableParams = Optional[RemoteControlEnableParams]

type OverriddenMetadata struct {
	EffectiveValue  json.RawMessage     `json:"effectiveValue"`
	Message         string              `json:"message"`
	OverridingLayer ConfigLayerMetadata `json:"overridingLayer"`
}

var overriddenMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "effectiveValue", flags: fieldRequired},
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "overridingLayer", flags: fieldRequired | fieldRejectNull},
	},
}

func (v OverriddenMetadata) MarshalJSON() ([]byte, error) {
	type wire OverriddenMetadata
	return json.Marshal(wire(v))
}

func (v *OverriddenMetadata) UnmarshalJSON(data []byte) error {
	type wire OverriddenMetadata
	return decodeObject(data, &overriddenMetadataSpec, (*wire)(v))
}

type PatchApplyStatus string

const (
	PatchApplyStatusInProgress PatchApplyStatus = "inProgress"
	PatchApplyStatusCompleted  PatchApplyStatus = "completed"
	PatchApplyStatusFailed     PatchApplyStatus = "failed"
	PatchApplyStatusDeclined   PatchApplyStatus = "declined"
)

type PatchChangeKind struct {
	MovePath  Optional[string] `json:"move_path,omitzero"`
	TypeValue string           `json:"type"`
	RawJSON   json.RawMessage  `json:"-"`
}

var patchChangeKindSpec = objectSpec{
	fields: []objectField{
		{name: "move_path"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"add", "delete", "update"},
}

func (v PatchChangeKind) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "add":
	case "delete":
	case "update":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire PatchChangeKind
	return json.Marshal(wire(v))
}

func (v *PatchChangeKind) UnmarshalJSON(data []byte) error {
	type wire PatchChangeKind
	var s objectScan
	known, err := s.begin(data, &patchChangeKindSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "add":
	case "delete":
	case "update":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type PathUri string

type PendingBackgroundTerminal struct {
	Command            string                `json:"command"`
	NotifyOnExit       OptionalNonNull[bool] `json:"notifyOnExit,omitzero"`
	OutputWatch        Optional[string]      `json:"outputWatch,omitzero"`
	OutputWatchMatched OptionalNonNull[bool] `json:"outputWatchMatched,omitzero"`
	OutputWatchWaitMs  Optional[uint64]      `json:"outputWatchWaitMs,omitzero"`
	ProcessID          string                `json:"processId"`
	RunningForMs       uint64                `json:"runningForMs"`
}

var pendingBackgroundTerminalSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldRequired | fieldRejectNull},
		{name: "notifyOnExit", flags: fieldNonNullOptional},
		{name: "outputWatch"},
		{name: "outputWatchMatched", flags: fieldNonNullOptional},
		{name: "outputWatchWaitMs"},
		{name: "processId", flags: fieldRequired | fieldRejectNull},
		{name: "runningForMs", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PendingBackgroundTerminal) MarshalJSON() ([]byte, error) {
	type wire PendingBackgroundTerminal
	return json.Marshal(wire(v))
}

func (v *PendingBackgroundTerminal) UnmarshalJSON(data []byte) error {
	type wire PendingBackgroundTerminal
	var s objectScan
	if _, err := s.begin(data, &pendingBackgroundTerminalSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueOutputWatchWaitMs, ok := v.OutputWatchWaitMs.Value(); ok {
		if valueOutputWatchWaitMs < 0 {
			return DecodeError{Field: "outputWatchWaitMs", Reason: "below minimum 0"}
		}
	}
	if v.RunningForMs < 0 {
		return DecodeError{Field: "runningForMs", Reason: "below minimum 0"}
	}
	return nil
}

type PermissionProfileListParams struct {
	Cursor Optional[string] `json:"cursor,omitzero"`
	Cwd    Optional[string] `json:"cwd,omitzero"`
	Limit  Optional[uint32] `json:"limit,omitzero"`
}

var permissionProfileListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "cwd"},
		{name: "limit"},
	},
}

func (v PermissionProfileListParams) MarshalJSON() ([]byte, error) {
	type wire PermissionProfileListParams
	return json.Marshal(wire(v))
}

func (v *PermissionProfileListParams) UnmarshalJSON(data []byte) error {
	type wire PermissionProfileListParams
	var s objectScan
	if _, err := s.begin(data, &permissionProfileListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type PermissionProfileListResponse struct {
	Data       []PermissionProfileSummary `json:"data"`
	NextCursor Optional[string]           `json:"nextCursor,omitzero"`
}

var permissionProfileListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v PermissionProfileListResponse) MarshalJSON() ([]byte, error) {
	type wire PermissionProfileListResponse
	return json.Marshal(wire(v))
}

func (v *PermissionProfileListResponse) UnmarshalJSON(data []byte) error {
	type wire PermissionProfileListResponse
	return decodeObject(data, &permissionProfileListResponseSpec, (*wire)(v))
}

type PermissionProfileSummary struct {
	Allowed     bool             `json:"allowed"`
	Description Optional[string] `json:"description,omitzero"`
	ID          string           `json:"id"`
}

var permissionProfileSummarySpec = objectSpec{
	fields: []objectField{
		{name: "allowed", flags: fieldRequired | fieldRejectNull},
		{name: "description"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PermissionProfileSummary) MarshalJSON() ([]byte, error) {
	type wire PermissionProfileSummary
	return json.Marshal(wire(v))
}

func (v *PermissionProfileSummary) UnmarshalJSON(data []byte) error {
	type wire PermissionProfileSummary
	return decodeObject(data, &permissionProfileSummarySpec, (*wire)(v))
}

type Personality string

const (
	PersonalityNone      Personality = "none"
	PersonalityFriendly  Personality = "friendly"
	PersonalityPragmatic Personality = "pragmatic"
)

type PlanDeltaNotification struct {
	Delta    string `json:"delta"`
	ItemID   string `json:"itemId"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var planDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PlanDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire PlanDeltaNotification
	return json.Marshal(wire(v))
}

func (v *PlanDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire PlanDeltaNotification
	return decodeObject(data, &planDeltaNotificationSpec, (*wire)(v))
}

type PlanType string

const (
	PlanTypeFree                        PlanType = "free"
	PlanTypeGoValue                     PlanType = "go"
	PlanTypePlus                        PlanType = "plus"
	PlanTypePro                         PlanType = "pro"
	PlanTypeProlite                     PlanType = "prolite"
	PlanTypePromax                      PlanType = "promax"
	PlanTypeTeam                        PlanType = "team"
	PlanTypeSelfServeBusinessProlite    PlanType = "self_serve_business_prolite"
	PlanTypeSelfServeBusinessUsageBased PlanType = "self_serve_business_usage_based"
	PlanTypeBusiness                    PlanType = "business"
	PlanTypeEnt26                       PlanType = "ent26"
	PlanTypeEnterpriseCbpAutomation     PlanType = "enterprise_cbp_automation"
	PlanTypeEnterpriseCbpUsageBased     PlanType = "enterprise_cbp_usage_based"
	PlanTypeEnterprise                  PlanType = "enterprise"
	PlanTypeEdu                         PlanType = "edu"
	PlanTypeEduPlus                     PlanType = "edu_plus"
	PlanTypeEduPro                      PlanType = "edu_pro"
	PlanTypeSci                         PlanType = "sci"
	PlanTypeSciPlus                     PlanType = "sci_plus"
	PlanTypeUnknown                     PlanType = "unknown"
)

type PluginAuthPolicy string

const (
	PluginAuthPolicyOnInstall PluginAuthPolicy = "ON_INSTALL"
	PluginAuthPolicyOnUse     PluginAuthPolicy = "ON_USE"
)

type PluginAvailability string

const (
	PluginAvailabilityDisabledByAdmin PluginAvailability = "DISABLED_BY_ADMIN"
	PluginAvailabilityAvailable       PluginAvailability = "AVAILABLE"
)

func (v *PluginAvailability) UnmarshalJSON(data []byte) error {
	var value string
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	switch value {
	case "ENABLED":
		*v = PluginAvailabilityAvailable
	default:
		*v = PluginAvailability(value)
	}
	return nil
}

func (v PluginAvailability) MarshalJSON() ([]byte, error) {
	switch string(v) {
	case "ENABLED":
		return json.Marshal("AVAILABLE")
	default:
		return json.Marshal(string(v))
	}
}

type PluginDetail struct {
	AppTemplates    []AppTemplateSummary             `json:"appTemplates"`
	Apps            []AppSummary                     `json:"apps"`
	Description     Optional[string]                 `json:"description,omitzero"`
	Hooks           []PluginHookSummary              `json:"hooks"`
	MarketplaceName string                           `json:"marketplaceName"`
	MarketplacePath Optional[AbsolutePathBuf]        `json:"marketplacePath,omitzero"`
	McpServers      []string                         `json:"mcpServers"`
	OnboardingSkill Optional[SkillSummary]           `json:"onboardingSkill,omitzero"`
	ScheduledTasks  Optional[[]ScheduledTaskSummary] `json:"scheduledTasks,omitzero"`
	ShareURL        Optional[string]                 `json:"shareUrl,omitzero"`
	Skills          []SkillSummary                   `json:"skills"`
	Summary         PluginSummary                    `json:"summary"`
}

var pluginDetailSpec = objectSpec{
	fields: []objectField{
		{name: "appTemplates", flags: fieldRequired | fieldRejectNull},
		{name: "apps", flags: fieldRequired | fieldRejectNull},
		{name: "description"},
		{name: "hooks", flags: fieldRequired | fieldRejectNull},
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
		{name: "marketplacePath"},
		{name: "mcpServers", flags: fieldRequired | fieldRejectNull},
		{name: "onboardingSkill", defaultJSON: "null"},
		{name: "scheduledTasks"},
		{name: "shareUrl"},
		{name: "skills", flags: fieldRequired | fieldRejectNull},
		{name: "summary", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginDetail) MarshalJSON() ([]byte, error) {
	type wire PluginDetail
	return json.Marshal(wire(v))
}

func (v *PluginDetail) UnmarshalJSON(data []byte) error {
	type wire PluginDetail
	return decodeObject(data, &pluginDetailSpec, (*wire)(v))
}

type PluginDisabledReason string

const (
	PluginDisabledReasonDisabledByAdmin        PluginDisabledReason = "disabled_by_admin"
	PluginDisabledReasonPlanNotEligible        PluginDisabledReason = "plan_not_eligible"
	PluginDisabledReasonRequiredAppUnavailable PluginDisabledReason = "required_app_unavailable"
	PluginDisabledReasonUnknown                PluginDisabledReason = "unknown"
)

type PluginHookSummary struct {
	EventName HookEventName `json:"eventName"`
	Key       string        `json:"key"`
}

var pluginHookSummarySpec = objectSpec{
	fields: []objectField{
		{name: "eventName", flags: fieldRequired | fieldRejectNull},
		{name: "key", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginHookSummary) MarshalJSON() ([]byte, error) {
	type wire PluginHookSummary
	return json.Marshal(wire(v))
}

func (v *PluginHookSummary) UnmarshalJSON(data []byte) error {
	type wire PluginHookSummary
	return decodeObject(data, &pluginHookSummarySpec, (*wire)(v))
}

type PluginInstallParams struct {
	InstallAttemptID      Optional[string]          `json:"installAttemptId,omitzero"`
	MarketplacePath       Optional[AbsolutePathBuf] `json:"marketplacePath,omitzero"`
	PluginName            string                    `json:"pluginName"`
	RemoteMarketplaceName Optional[string]          `json:"remoteMarketplaceName,omitzero"`
}

var pluginInstallParamsSpec = objectSpec{
	fields: []objectField{
		{name: "installAttemptId"},
		{name: "marketplacePath"},
		{name: "pluginName", flags: fieldRequired | fieldRejectNull},
		{name: "remoteMarketplaceName"},
	},
}

func (v PluginInstallParams) MarshalJSON() ([]byte, error) {
	type wire PluginInstallParams
	return json.Marshal(wire(v))
}

func (v *PluginInstallParams) UnmarshalJSON(data []byte) error {
	type wire PluginInstallParams
	return decodeObject(data, &pluginInstallParamsSpec, (*wire)(v))
}

type PluginInstallPolicy string

const (
	PluginInstallPolicyNotAvailable       PluginInstallPolicy = "NOT_AVAILABLE"
	PluginInstallPolicyAvailable          PluginInstallPolicy = "AVAILABLE"
	PluginInstallPolicyInstalledByDefault PluginInstallPolicy = "INSTALLED_BY_DEFAULT"
)

type PluginInstallPolicySource string

const (
	PluginInstallPolicySourceWorkspaceSetting     PluginInstallPolicySource = "WORKSPACE_SETTING"
	PluginInstallPolicySourceImplicitCanonicalApp PluginInstallPolicySource = "IMPLICIT_CANONICAL_APP"
)

type PluginInstallResponse struct {
	AppsNeedingAuth []AppSummary     `json:"appsNeedingAuth"`
	AuthPolicy      PluginAuthPolicy `json:"authPolicy"`
}

var pluginInstallResponseSpec = objectSpec{
	fields: []objectField{
		{name: "appsNeedingAuth", flags: fieldRequired | fieldRejectNull},
		{name: "authPolicy", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginInstallResponse) MarshalJSON() ([]byte, error) {
	type wire PluginInstallResponse
	return json.Marshal(wire(v))
}

func (v *PluginInstallResponse) UnmarshalJSON(data []byte) error {
	type wire PluginInstallResponse
	return decodeObject(data, &pluginInstallResponseSpec, (*wire)(v))
}

type PluginInstalledParams struct {
	Cwds                         Optional[[]AbsolutePathBuf] `json:"cwds,omitzero"`
	InstallSuggestionPluginNames Optional[[]string]          `json:"installSuggestionPluginNames,omitzero"`
}

var pluginInstalledParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwds"},
		{name: "installSuggestionPluginNames"},
	},
}

func (v PluginInstalledParams) MarshalJSON() ([]byte, error) {
	type wire PluginInstalledParams
	return json.Marshal(wire(v))
}

func (v *PluginInstalledParams) UnmarshalJSON(data []byte) error {
	type wire PluginInstalledParams
	return decodeObject(data, &pluginInstalledParamsSpec, (*wire)(v))
}

type PluginInstalledResponse struct {
	MarketplaceLoadErrors OptionalNonNull[[]MarketplaceLoadErrorInfo] `json:"marketplaceLoadErrors,omitzero"`
	Marketplaces          []PluginMarketplaceEntry                    `json:"marketplaces"`
}

var pluginInstalledResponseSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceLoadErrors", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "marketplaces", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginInstalledResponse) MarshalJSON() ([]byte, error) {
	type wire PluginInstalledResponse
	return json.Marshal(wire(v))
}

func (v *PluginInstalledResponse) UnmarshalJSON(data []byte) error {
	type wire PluginInstalledResponse
	return decodeObject(data, &pluginInstalledResponseSpec, (*wire)(v))
}

type PluginInterface struct {
	BrandColor        Optional[string]          `json:"brandColor,omitzero"`
	Capabilities      []string                  `json:"capabilities"`
	Category          Optional[string]          `json:"category,omitzero"`
	ComposerIcon      Optional[AbsolutePathBuf] `json:"composerIcon,omitzero"`
	ComposerIconURL   Optional[string]          `json:"composerIconUrl,omitzero"`
	DefaultPrompt     Optional[[]string]        `json:"defaultPrompt,omitzero"`
	DeveloperName     Optional[string]          `json:"developerName,omitzero"`
	DisplayName       Optional[string]          `json:"displayName,omitzero"`
	Logo              Optional[AbsolutePathBuf] `json:"logo,omitzero"`
	LogoDark          Optional[AbsolutePathBuf] `json:"logoDark,omitzero"`
	LogoURL           Optional[string]          `json:"logoUrl,omitzero"`
	LogoURLDark       Optional[string]          `json:"logoUrlDark,omitzero"`
	LongDescription   Optional[string]          `json:"longDescription,omitzero"`
	PrivacyPolicyURL  Optional[string]          `json:"privacyPolicyUrl,omitzero"`
	ScreenshotURLs    []string                  `json:"screenshotUrls"`
	Screenshots       []AbsolutePathBuf         `json:"screenshots"`
	ShortDescription  Optional[string]          `json:"shortDescription,omitzero"`
	TermsOfServiceURL Optional[string]          `json:"termsOfServiceUrl,omitzero"`
	WebsiteURL        Optional[string]          `json:"websiteUrl,omitzero"`
}

var pluginInterfaceSpec = objectSpec{
	fields: []objectField{
		{name: "brandColor"},
		{name: "capabilities", flags: fieldRequired | fieldRejectNull},
		{name: "category"},
		{name: "composerIcon"},
		{name: "composerIconUrl"},
		{name: "defaultPrompt"},
		{name: "developerName"},
		{name: "displayName"},
		{name: "logo"},
		{name: "logoDark"},
		{name: "logoUrl"},
		{name: "logoUrlDark"},
		{name: "longDescription"},
		{name: "privacyPolicyUrl"},
		{name: "screenshotUrls", flags: fieldRequired | fieldRejectNull},
		{name: "screenshots", flags: fieldRequired | fieldRejectNull},
		{name: "shortDescription"},
		{name: "termsOfServiceUrl"},
		{name: "websiteUrl"},
	},
}

func (v PluginInterface) MarshalJSON() ([]byte, error) {
	type wire PluginInterface
	return json.Marshal(wire(v))
}

func (v *PluginInterface) UnmarshalJSON(data []byte) error {
	type wire PluginInterface
	return decodeObject(data, &pluginInterfaceSpec, (*wire)(v))
}

type PluginListMarketplaceKind string

const (
	PluginListMarketplaceKindLocal              PluginListMarketplaceKind = "local"
	PluginListMarketplaceKindVertical           PluginListMarketplaceKind = "vertical"
	PluginListMarketplaceKindWorkspaceDirectory PluginListMarketplaceKind = "workspace-directory"
	PluginListMarketplaceKindSharedWithMe       PluginListMarketplaceKind = "shared-with-me"
	PluginListMarketplaceKindCreatedByMeRemote  PluginListMarketplaceKind = "created-by-me-remote"
)

type PluginListParams struct {
	Cwds             Optional[[]AbsolutePathBuf]           `json:"cwds,omitzero"`
	ForceRefetch     OptionalNonNull[bool]                 `json:"forceRefetch,omitzero"`
	MarketplaceKinds Optional[[]PluginListMarketplaceKind] `json:"marketplaceKinds,omitzero"`
}

var pluginListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwds"},
		{name: "forceRefetch", flags: fieldNonNullOptional},
		{name: "marketplaceKinds"},
	},
}

func (v PluginListParams) MarshalJSON() ([]byte, error) {
	type wire PluginListParams
	return json.Marshal(wire(v))
}

func (v *PluginListParams) UnmarshalJSON(data []byte) error {
	type wire PluginListParams
	return decodeObject(data, &pluginListParamsSpec, (*wire)(v))
}

type PluginListResponse struct {
	FeaturedPluginIDs     OptionalNonNull[[]string]                   `json:"featuredPluginIds,omitzero"`
	MarketplaceLoadErrors OptionalNonNull[[]MarketplaceLoadErrorInfo] `json:"marketplaceLoadErrors,omitzero"`
	Marketplaces          []PluginMarketplaceEntry                    `json:"marketplaces"`
}

var pluginListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "featuredPluginIds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "marketplaceLoadErrors", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "marketplaces", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginListResponse) MarshalJSON() ([]byte, error) {
	type wire PluginListResponse
	return json.Marshal(wire(v))
}

func (v *PluginListResponse) UnmarshalJSON(data []byte) error {
	type wire PluginListResponse
	return decodeObject(data, &pluginListResponseSpec, (*wire)(v))
}

type PluginMarketplaceEntry struct {
	InterfaceValue Optional[MarketplaceInterface] `json:"interface,omitzero"`
	Name           string                         `json:"name"`
	Path           Optional[AbsolutePathBuf]      `json:"path,omitzero"`
	Plugins        []PluginSummary                `json:"plugins"`
}

var pluginMarketplaceEntrySpec = objectSpec{
	fields: []objectField{
		{name: "interface"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "path"},
		{name: "plugins", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginMarketplaceEntry) MarshalJSON() ([]byte, error) {
	type wire PluginMarketplaceEntry
	return json.Marshal(wire(v))
}

func (v *PluginMarketplaceEntry) UnmarshalJSON(data []byte) error {
	type wire PluginMarketplaceEntry
	return decodeObject(data, &pluginMarketplaceEntrySpec, (*wire)(v))
}

type PluginReadParams struct {
	MarketplacePath       Optional[AbsolutePathBuf] `json:"marketplacePath,omitzero"`
	PluginName            string                    `json:"pluginName"`
	RemoteMarketplaceName Optional[string]          `json:"remoteMarketplaceName,omitzero"`
}

var pluginReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "marketplacePath"},
		{name: "pluginName", flags: fieldRequired | fieldRejectNull},
		{name: "remoteMarketplaceName"},
	},
}

func (v PluginReadParams) MarshalJSON() ([]byte, error) {
	type wire PluginReadParams
	return json.Marshal(wire(v))
}

func (v *PluginReadParams) UnmarshalJSON(data []byte) error {
	type wire PluginReadParams
	return decodeObject(data, &pluginReadParamsSpec, (*wire)(v))
}

type PluginReadResponse struct {
	Plugin PluginDetail `json:"plugin"`
}

var pluginReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "plugin", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginReadResponse) MarshalJSON() ([]byte, error) {
	type wire PluginReadResponse
	return json.Marshal(wire(v))
}

func (v *PluginReadResponse) UnmarshalJSON(data []byte) error {
	type wire PluginReadResponse
	return decodeObject(data, &pluginReadResponseSpec, (*wire)(v))
}

type PluginReconcileChangedPlugin struct {
	HasApps   bool   `json:"hasApps"`
	HasHooks  bool   `json:"hasHooks"`
	HasMcps   bool   `json:"hasMcps"`
	HasSkills bool   `json:"hasSkills"`
	ID        string `json:"id"`
}

var pluginReconcileChangedPluginSpec = objectSpec{
	fields: []objectField{
		{name: "hasApps", flags: fieldRequired | fieldRejectNull},
		{name: "hasHooks", flags: fieldRequired | fieldRejectNull},
		{name: "hasMcps", flags: fieldRequired | fieldRejectNull},
		{name: "hasSkills", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginReconcileChangedPlugin) MarshalJSON() ([]byte, error) {
	type wire PluginReconcileChangedPlugin
	return json.Marshal(wire(v))
}

func (v *PluginReconcileChangedPlugin) UnmarshalJSON(data []byte) error {
	type wire PluginReconcileChangedPlugin
	return decodeObject(data, &pluginReconcileChangedPluginSpec, (*wire)(v))
}

type PluginReconcileParams struct {
	Reason Optional[string] `json:"reason,omitzero"`
}

var pluginReconcileParamsSpec = objectSpec{
	fields: []objectField{
		{name: "reason"},
	},
}

func (v PluginReconcileParams) MarshalJSON() ([]byte, error) {
	type wire PluginReconcileParams
	return json.Marshal(wire(v))
}

func (v *PluginReconcileParams) UnmarshalJSON(data []byte) error {
	type wire PluginReconcileParams
	return decodeObject(data, &pluginReconcileParamsSpec, (*wire)(v))
}

type PluginReconcileResponse struct {
	ChangedPlugins                       []PluginReconcileChangedPlugin `json:"changedPlugins"`
	FailedMaterializationRemotePluginIDs []string                       `json:"failedMaterializationRemotePluginIds"`
	FailedRemotePluginIDs                []string                       `json:"failedRemotePluginIds"`
}

var pluginReconcileResponseSpec = objectSpec{
	fields: []objectField{
		{name: "changedPlugins", flags: fieldRequired | fieldRejectNull},
		{name: "failedMaterializationRemotePluginIds", flags: fieldRequired | fieldRejectNull},
		{name: "failedRemotePluginIds", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginReconcileResponse) MarshalJSON() ([]byte, error) {
	type wire PluginReconcileResponse
	return json.Marshal(wire(v))
}

func (v *PluginReconcileResponse) UnmarshalJSON(data []byte) error {
	type wire PluginReconcileResponse
	return decodeObject(data, &pluginReconcileResponseSpec, (*wire)(v))
}

type PluginSearchParams struct {
	Cursor     Optional[string]            `json:"cursor,omitzero"`
	Cwds       Optional[[]AbsolutePathBuf] `json:"cwds,omitzero"`
	Limit      Optional[uint32]            `json:"limit,omitzero"`
	Scope      Optional[PluginSearchScope] `json:"scope,omitzero"`
	SearchTerm string                      `json:"searchTerm"`
}

var pluginSearchParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "cwds"},
		{name: "limit"},
		{name: "scope"},
		{name: "searchTerm", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginSearchParams) MarshalJSON() ([]byte, error) {
	type wire PluginSearchParams
	return json.Marshal(wire(v))
}

func (v *PluginSearchParams) UnmarshalJSON(data []byte) error {
	type wire PluginSearchParams
	var s objectScan
	if _, err := s.begin(data, &pluginSearchParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type PluginSearchResponse struct {
	Data       []PluginSearchResult `json:"data"`
	NextCursor Optional[string]     `json:"nextCursor,omitzero"`
}

var pluginSearchResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v PluginSearchResponse) MarshalJSON() ([]byte, error) {
	type wire PluginSearchResponse
	return json.Marshal(wire(v))
}

func (v *PluginSearchResponse) UnmarshalJSON(data []byte) error {
	type wire PluginSearchResponse
	return decodeObject(data, &pluginSearchResponseSpec, (*wire)(v))
}

type PluginSearchResult struct {
	MarketplaceName string                    `json:"marketplaceName"`
	MarketplacePath Optional[AbsolutePathBuf] `json:"marketplacePath,omitzero"`
	Plugin          PluginSummary             `json:"plugin"`
}

var pluginSearchResultSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
		{name: "marketplacePath"},
		{name: "plugin", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginSearchResult) MarshalJSON() ([]byte, error) {
	type wire PluginSearchResult
	return json.Marshal(wire(v))
}

func (v *PluginSearchResult) UnmarshalJSON(data []byte) error {
	type wire PluginSearchResult
	return decodeObject(data, &pluginSearchResultSpec, (*wire)(v))
}

type PluginSearchScope string

const (
	PluginSearchScopeGlobal    PluginSearchScope = "global"
	PluginSearchScopeWorkspace PluginSearchScope = "workspace"
	PluginSearchScopePersonal  PluginSearchScope = "personal"
)

type PluginShareCheckoutParams struct {
	RemotePluginID string `json:"remotePluginId"`
}

var pluginShareCheckoutParamsSpec = objectSpec{
	fields: []objectField{
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareCheckoutParams) MarshalJSON() ([]byte, error) {
	type wire PluginShareCheckoutParams
	return json.Marshal(wire(v))
}

func (v *PluginShareCheckoutParams) UnmarshalJSON(data []byte) error {
	type wire PluginShareCheckoutParams
	return decodeObject(data, &pluginShareCheckoutParamsSpec, (*wire)(v))
}

type PluginShareCheckoutResponse struct {
	MarketplaceName string           `json:"marketplaceName"`
	MarketplacePath AbsolutePathBuf  `json:"marketplacePath"`
	PluginID        string           `json:"pluginId"`
	PluginName      string           `json:"pluginName"`
	PluginPath      AbsolutePathBuf  `json:"pluginPath"`
	RemotePluginID  string           `json:"remotePluginId"`
	RemoteVersion   Optional[string] `json:"remoteVersion,omitzero"`
}

var pluginShareCheckoutResponseSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
		{name: "marketplacePath", flags: fieldRequired | fieldRejectNull},
		{name: "pluginId", flags: fieldRequired | fieldRejectNull},
		{name: "pluginName", flags: fieldRequired | fieldRejectNull},
		{name: "pluginPath", flags: fieldRequired | fieldRejectNull},
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
		{name: "remoteVersion"},
	},
}

func (v PluginShareCheckoutResponse) MarshalJSON() ([]byte, error) {
	type wire PluginShareCheckoutResponse
	return json.Marshal(wire(v))
}

func (v *PluginShareCheckoutResponse) UnmarshalJSON(data []byte) error {
	type wire PluginShareCheckoutResponse
	return decodeObject(data, &pluginShareCheckoutResponseSpec, (*wire)(v))
}

type PluginShareContext struct {
	CanPublishToWorkspace Optional[bool]                       `json:"canPublishToWorkspace,omitzero"`
	CreatorAccountUserID  Optional[string]                     `json:"creatorAccountUserId,omitzero"`
	CreatorName           Optional[string]                     `json:"creatorName,omitzero"`
	Discoverability       Optional[PluginShareDiscoverability] `json:"discoverability,omitzero"`
	RemotePluginID        string                               `json:"remotePluginId"`
	RemoteVersion         Optional[string]                     `json:"remoteVersion,omitzero"`
	SharePrincipals       Optional[[]PluginSharePrincipal]     `json:"sharePrincipals,omitzero"`
	ShareURL              Optional[string]                     `json:"shareUrl,omitzero"`
}

var pluginShareContextSpec = objectSpec{
	fields: []objectField{
		{name: "canPublishToWorkspace"},
		{name: "creatorAccountUserId"},
		{name: "creatorName"},
		{name: "discoverability"},
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
		{name: "remoteVersion", defaultJSON: "null"},
		{name: "sharePrincipals"},
		{name: "shareUrl"},
	},
}

func (v PluginShareContext) MarshalJSON() ([]byte, error) {
	type wire PluginShareContext
	return json.Marshal(wire(v))
}

func (v *PluginShareContext) UnmarshalJSON(data []byte) error {
	type wire PluginShareContext
	return decodeObject(data, &pluginShareContextSpec, (*wire)(v))
}

type PluginShareDeleteParams struct {
	RemotePluginID string `json:"remotePluginId"`
}

var pluginShareDeleteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareDeleteParams) MarshalJSON() ([]byte, error) {
	type wire PluginShareDeleteParams
	return json.Marshal(wire(v))
}

func (v *PluginShareDeleteParams) UnmarshalJSON(data []byte) error {
	type wire PluginShareDeleteParams
	return decodeObject(data, &pluginShareDeleteParamsSpec, (*wire)(v))
}

type PluginShareDeleteResponse map[string]json.RawMessage

func (v PluginShareDeleteResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *PluginShareDeleteResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = PluginShareDeleteResponse(raw)
	return nil
}

type PluginShareDiscoverability string

const (
	PluginShareDiscoverabilityListed   PluginShareDiscoverability = "LISTED"
	PluginShareDiscoverabilityUnlisted PluginShareDiscoverability = "UNLISTED"
	PluginShareDiscoverabilityPrivate  PluginShareDiscoverability = "PRIVATE"
)

type PluginShareListItem struct {
	LocalPluginPath Optional[AbsolutePathBuf] `json:"localPluginPath,omitzero"`
	Plugin          PluginSummary             `json:"plugin"`
}

var pluginShareListItemSpec = objectSpec{
	fields: []objectField{
		{name: "localPluginPath"},
		{name: "plugin", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareListItem) MarshalJSON() ([]byte, error) {
	type wire PluginShareListItem
	return json.Marshal(wire(v))
}

func (v *PluginShareListItem) UnmarshalJSON(data []byte) error {
	type wire PluginShareListItem
	return decodeObject(data, &pluginShareListItemSpec, (*wire)(v))
}

type PluginShareListParams map[string]json.RawMessage

func (v PluginShareListParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *PluginShareListParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = PluginShareListParams(raw)
	return nil
}

type PluginShareListResponse struct {
	Data []PluginShareListItem `json:"data"`
}

var pluginShareListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareListResponse) MarshalJSON() ([]byte, error) {
	type wire PluginShareListResponse
	return json.Marshal(wire(v))
}

func (v *PluginShareListResponse) UnmarshalJSON(data []byte) error {
	type wire PluginShareListResponse
	return decodeObject(data, &pluginShareListResponseSpec, (*wire)(v))
}

type PluginSharePrincipal struct {
	Name          string                   `json:"name"`
	PrincipalID   string                   `json:"principalId"`
	PrincipalType PluginSharePrincipalType `json:"principalType"`
	Role          PluginSharePrincipalRole `json:"role"`
}

var pluginSharePrincipalSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "principalId", flags: fieldRequired | fieldRejectNull},
		{name: "principalType", flags: fieldRequired | fieldRejectNull},
		{name: "role", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginSharePrincipal) MarshalJSON() ([]byte, error) {
	type wire PluginSharePrincipal
	return json.Marshal(wire(v))
}

func (v *PluginSharePrincipal) UnmarshalJSON(data []byte) error {
	type wire PluginSharePrincipal
	return decodeObject(data, &pluginSharePrincipalSpec, (*wire)(v))
}

type PluginSharePrincipalRole string

const (
	PluginSharePrincipalRoleReader PluginSharePrincipalRole = "reader"
	PluginSharePrincipalRoleEditor PluginSharePrincipalRole = "editor"
	PluginSharePrincipalRoleOwner  PluginSharePrincipalRole = "owner"
)

type PluginSharePrincipalType string

const (
	PluginSharePrincipalTypeUser      PluginSharePrincipalType = "user"
	PluginSharePrincipalTypeGroup     PluginSharePrincipalType = "group"
	PluginSharePrincipalTypeWorkspace PluginSharePrincipalType = "workspace"
)

type PluginShareSaveParams struct {
	Discoverability Optional[PluginShareDiscoverability] `json:"discoverability,omitzero"`
	PluginPath      AbsolutePathBuf                      `json:"pluginPath"`
	RemotePluginID  Optional[string]                     `json:"remotePluginId,omitzero"`
	ShareTargets    Optional[[]PluginShareTarget]        `json:"shareTargets,omitzero"`
}

var pluginShareSaveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "discoverability"},
		{name: "pluginPath", flags: fieldRequired | fieldRejectNull},
		{name: "remotePluginId"},
		{name: "shareTargets"},
	},
}

func (v PluginShareSaveParams) MarshalJSON() ([]byte, error) {
	type wire PluginShareSaveParams
	return json.Marshal(wire(v))
}

func (v *PluginShareSaveParams) UnmarshalJSON(data []byte) error {
	type wire PluginShareSaveParams
	return decodeObject(data, &pluginShareSaveParamsSpec, (*wire)(v))
}

type PluginShareSaveResponse struct {
	CanPublishToWorkspace Optional[bool] `json:"canPublishToWorkspace,omitzero"`
	RemotePluginID        string         `json:"remotePluginId"`
	ShareURL              string         `json:"shareUrl"`
}

var pluginShareSaveResponseSpec = objectSpec{
	fields: []objectField{
		{name: "canPublishToWorkspace"},
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
		{name: "shareUrl", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareSaveResponse) MarshalJSON() ([]byte, error) {
	type wire PluginShareSaveResponse
	return json.Marshal(wire(v))
}

func (v *PluginShareSaveResponse) UnmarshalJSON(data []byte) error {
	type wire PluginShareSaveResponse
	return decodeObject(data, &pluginShareSaveResponseSpec, (*wire)(v))
}

type PluginShareTarget struct {
	PrincipalID   string                   `json:"principalId"`
	PrincipalType PluginSharePrincipalType `json:"principalType"`
	Role          PluginShareTargetRole    `json:"role"`
}

var pluginShareTargetSpec = objectSpec{
	fields: []objectField{
		{name: "principalId", flags: fieldRequired | fieldRejectNull},
		{name: "principalType", flags: fieldRequired | fieldRejectNull},
		{name: "role", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareTarget) MarshalJSON() ([]byte, error) {
	type wire PluginShareTarget
	return json.Marshal(wire(v))
}

func (v *PluginShareTarget) UnmarshalJSON(data []byte) error {
	type wire PluginShareTarget
	return decodeObject(data, &pluginShareTargetSpec, (*wire)(v))
}

type PluginShareTargetRole string

const (
	PluginShareTargetRoleReader PluginShareTargetRole = "reader"
	PluginShareTargetRoleEditor PluginShareTargetRole = "editor"
)

type PluginShareUpdateDiscoverability string

const (
	PluginShareUpdateDiscoverabilityUnlisted PluginShareUpdateDiscoverability = "UNLISTED"
	PluginShareUpdateDiscoverabilityPrivate  PluginShareUpdateDiscoverability = "PRIVATE"
	PluginShareUpdateDiscoverabilityListed   PluginShareUpdateDiscoverability = "LISTED"
)

type PluginShareUpdateTargetsParams struct {
	Discoverability PluginShareUpdateDiscoverability `json:"discoverability"`
	RemotePluginID  string                           `json:"remotePluginId"`
	ShareTargets    []PluginShareTarget              `json:"shareTargets"`
}

var pluginShareUpdateTargetsParamsSpec = objectSpec{
	fields: []objectField{
		{name: "discoverability", flags: fieldRequired | fieldRejectNull},
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
		{name: "shareTargets", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareUpdateTargetsParams) MarshalJSON() ([]byte, error) {
	type wire PluginShareUpdateTargetsParams
	return json.Marshal(wire(v))
}

func (v *PluginShareUpdateTargetsParams) UnmarshalJSON(data []byte) error {
	type wire PluginShareUpdateTargetsParams
	return decodeObject(data, &pluginShareUpdateTargetsParamsSpec, (*wire)(v))
}

type PluginShareUpdateTargetsResponse struct {
	Discoverability PluginShareDiscoverability `json:"discoverability"`
	Principals      []PluginSharePrincipal     `json:"principals"`
}

var pluginShareUpdateTargetsResponseSpec = objectSpec{
	fields: []objectField{
		{name: "discoverability", flags: fieldRequired | fieldRejectNull},
		{name: "principals", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginShareUpdateTargetsResponse) MarshalJSON() ([]byte, error) {
	type wire PluginShareUpdateTargetsResponse
	return json.Marshal(wire(v))
}

func (v *PluginShareUpdateTargetsResponse) UnmarshalJSON(data []byte) error {
	type wire PluginShareUpdateTargetsResponse
	return decodeObject(data, &pluginShareUpdateTargetsResponseSpec, (*wire)(v))
}

type PluginSkillReadParams struct {
	RemoteMarketplaceName string `json:"remoteMarketplaceName"`
	RemotePluginID        string `json:"remotePluginId"`
	SkillName             string `json:"skillName"`
}

var pluginSkillReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "remoteMarketplaceName", flags: fieldRequired | fieldRejectNull},
		{name: "remotePluginId", flags: fieldRequired | fieldRejectNull},
		{name: "skillName", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginSkillReadParams) MarshalJSON() ([]byte, error) {
	type wire PluginSkillReadParams
	return json.Marshal(wire(v))
}

func (v *PluginSkillReadParams) UnmarshalJSON(data []byte) error {
	type wire PluginSkillReadParams
	return decodeObject(data, &pluginSkillReadParamsSpec, (*wire)(v))
}

type PluginSkillReadResponse struct {
	Contents Optional[string] `json:"contents,omitzero"`
}

var pluginSkillReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "contents"},
	},
}

func (v PluginSkillReadResponse) MarshalJSON() ([]byte, error) {
	type wire PluginSkillReadResponse
	return json.Marshal(wire(v))
}

func (v *PluginSkillReadResponse) UnmarshalJSON(data []byte) error {
	type wire PluginSkillReadResponse
	return decodeObject(data, &pluginSkillReadResponseSpec, (*wire)(v))
}

type PluginSource struct {
	PackageValue OptionalNonNull[string]   `json:"package,omitzero"`
	Path         Optional[AbsolutePathBuf] `json:"path,omitzero"`
	RefName      Optional[string]          `json:"refName,omitzero"`
	Registry     Optional[string]          `json:"registry,omitzero"`
	Sha          Optional[string]          `json:"sha,omitzero"`
	TypeValue    string                    `json:"type"`
	URL          OptionalNonNull[string]   `json:"url,omitzero"`
	Version      Optional[string]          `json:"version,omitzero"`
	RawJSON      json.RawMessage           `json:"-"`
}

var pluginSourceSpec = objectSpec{
	fields: []objectField{
		{name: "package", flags: fieldNonNullOptional},
		{name: "path"},
		{name: "refName"},
		{name: "registry"},
		{name: "sha"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "url", flags: fieldNonNullOptional},
		{name: "version"},
	},
	tag:  5,
	tags: []string{"local", "git", "npm", "remote"},
}

func (v PluginSource) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "local":
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type local"}
		}
		if v.Path.IsNull() {
			return nil, DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "git":
		if !v.URL.IsSet() {
			return nil, DecodeError{Field: "url", Reason: "missing required field for type git"}
		}
	case "npm":
		if !v.PackageValue.IsSet() {
			return nil, DecodeError{Field: "package", Reason: "missing required field for type npm"}
		}
	case "remote":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire PluginSource
	return json.Marshal(wire(v))
}

func (v *PluginSource) UnmarshalJSON(data []byte) error {
	type wire PluginSource
	var s objectScan
	known, err := s.begin(data, &pluginSourceSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "local":
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type local"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "git":
		if !s.has("url") {
			return DecodeError{Field: "url", Reason: "missing required field for type git"}
		}
		if s.isNull("url") {
			return DecodeError{Field: "url", Reason: "cannot be null"}
		}
	case "npm":
		if !s.has("package") {
			return DecodeError{Field: "package", Reason: "missing required field for type npm"}
		}
		if s.isNull("package") {
			return DecodeError{Field: "package", Reason: "cannot be null"}
		}
	case "remote":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type PluginSummary struct {
	AuthPolicy                       PluginAuthPolicy                    `json:"authPolicy"`
	Availability                     OptionalNonNull[PluginAvailability] `json:"availability,omitzero"`
	DisabledReason                   Optional[PluginDisabledReason]      `json:"disabledReason,omitzero"`
	EligiblePlanTypes                Optional[[]string]                  `json:"eligiblePlanTypes,omitzero"`
	Enabled                          bool                                `json:"enabled"`
	ID                               string                              `json:"id"`
	InstallPolicy                    PluginInstallPolicy                 `json:"installPolicy"`
	InstallPolicySource              Optional[PluginInstallPolicySource] `json:"installPolicySource,omitzero"`
	Installed                        bool                                `json:"installed"`
	InstalledAt                      Optional[int64]                     `json:"installedAt,omitzero"`
	InterfaceValue                   Optional[PluginInterface]           `json:"interface,omitzero"`
	Keywords                         OptionalNonNull[[]string]           `json:"keywords,omitzero"`
	LocalVersion                     Optional[string]                    `json:"localVersion,omitzero"`
	MustShowInstallationInterstitial Optional[bool]                      `json:"mustShowInstallationInterstitial,omitzero"`
	Name                             string                              `json:"name"`
	RemotePluginID                   Optional[string]                    `json:"remotePluginId,omitzero"`
	ShareContext                     Optional[PluginShareContext]        `json:"shareContext,omitzero"`
	Source                           PluginSource                        `json:"source"`
	Version                          Optional[string]                    `json:"version,omitzero"`
}

var pluginSummarySpec = objectSpec{
	fields: []objectField{
		{name: "authPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "availability", flags: fieldNonNullOptional, defaultJSON: "\"AVAILABLE\""},
		{name: "disabledReason"},
		{name: "eligiblePlanTypes"},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "installPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "installPolicySource"},
		{name: "installed", flags: fieldRequired | fieldRejectNull},
		{name: "installedAt"},
		{name: "interface"},
		{name: "keywords", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "localVersion", defaultJSON: "null"},
		{name: "mustShowInstallationInterstitial"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "remotePluginId"},
		{name: "shareContext"},
		{name: "source", flags: fieldRequired | fieldRejectNull},
		{name: "version"},
	},
}

func (v PluginSummary) MarshalJSON() ([]byte, error) {
	type wire PluginSummary
	return json.Marshal(wire(v))
}

func (v *PluginSummary) UnmarshalJSON(data []byte) error {
	type wire PluginSummary
	return decodeObject(data, &pluginSummarySpec, (*wire)(v))
}

type PluginUninstallParams struct {
	PluginID string `json:"pluginId"`
}

var pluginUninstallParamsSpec = objectSpec{
	fields: []objectField{
		{name: "pluginId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginUninstallParams) MarshalJSON() ([]byte, error) {
	type wire PluginUninstallParams
	return json.Marshal(wire(v))
}

func (v *PluginUninstallParams) UnmarshalJSON(data []byte) error {
	type wire PluginUninstallParams
	return decodeObject(data, &pluginUninstallParamsSpec, (*wire)(v))
}

type PluginUninstallResponse map[string]json.RawMessage

func (v PluginUninstallResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *PluginUninstallResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = PluginUninstallResponse(raw)
	return nil
}

type PluginsMigration struct {
	MarketplaceName string   `json:"marketplaceName"`
	PluginNames     []string `json:"pluginNames"`
}

var pluginsMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "marketplaceName", flags: fieldRequired | fieldRejectNull},
		{name: "pluginNames", flags: fieldRequired | fieldRejectNull},
	},
}

func (v PluginsMigration) MarshalJSON() ([]byte, error) {
	type wire PluginsMigration
	return json.Marshal(wire(v))
}

func (v *PluginsMigration) UnmarshalJSON(data []byte) error {
	type wire PluginsMigration
	return decodeObject(data, &pluginsMigrationSpec, (*wire)(v))
}

type ProcessExitedNotification struct {
	ExitCode            int32          `json:"exitCode"`
	OutputDrainTimedOut Optional[bool] `json:"outputDrainTimedOut,omitzero"`
	ProcessHandle       string         `json:"processHandle"`
	Stderr              string         `json:"stderr"`
	StderrCapReached    bool           `json:"stderrCapReached"`
	Stdout              string         `json:"stdout"`
	StdoutCapReached    bool           `json:"stdoutCapReached"`
}

var processExitedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "exitCode", flags: fieldRequired | fieldRejectNull},
		{name: "outputDrainTimedOut"},
		{name: "processHandle", flags: fieldRequired | fieldRejectNull},
		{name: "stderr", flags: fieldRequired | fieldRejectNull},
		{name: "stderrCapReached", flags: fieldRequired | fieldRejectNull},
		{name: "stdout", flags: fieldRequired | fieldRejectNull},
		{name: "stdoutCapReached", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProcessExitedNotification) MarshalJSON() ([]byte, error) {
	type wire ProcessExitedNotification
	return json.Marshal(wire(v))
}

func (v *ProcessExitedNotification) UnmarshalJSON(data []byte) error {
	type wire ProcessExitedNotification
	return decodeObject(data, &processExitedNotificationSpec, (*wire)(v))
}

type ProcessKillParams struct {
	ProcessHandle string `json:"processHandle"`
}

var processKillParamsSpec = objectSpec{
	fields: []objectField{
		{name: "processHandle", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProcessKillParams) MarshalJSON() ([]byte, error) {
	type wire ProcessKillParams
	return json.Marshal(wire(v))
}

func (v *ProcessKillParams) UnmarshalJSON(data []byte) error {
	type wire ProcessKillParams
	return decodeObject(data, &processKillParamsSpec, (*wire)(v))
}

type ProcessKillResponse map[string]json.RawMessage

func (v ProcessKillResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ProcessKillResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ProcessKillResponse(raw)
	return nil
}

type ProcessOutputDeltaNotification struct {
	CapReached    bool                `json:"capReached"`
	DeltaBase64   string              `json:"deltaBase64"`
	ProcessHandle string              `json:"processHandle"`
	Stream        ProcessOutputStream `json:"stream"`
}

var processOutputDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "capReached", flags: fieldRequired | fieldRejectNull},
		{name: "deltaBase64", flags: fieldRequired | fieldRejectNull},
		{name: "processHandle", flags: fieldRequired | fieldRejectNull},
		{name: "stream", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProcessOutputDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire ProcessOutputDeltaNotification
	return json.Marshal(wire(v))
}

func (v *ProcessOutputDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire ProcessOutputDeltaNotification
	return decodeObject(data, &processOutputDeltaNotificationSpec, (*wire)(v))
}

type ProcessOutputStream string

const (
	ProcessOutputStreamStdout ProcessOutputStream = "stdout"
	ProcessOutputStreamStderr ProcessOutputStream = "stderr"
)

type ProcessResizePtyParams struct {
	ProcessHandle string              `json:"processHandle"`
	Size          ProcessTerminalSize `json:"size"`
}

var processResizePtyParamsSpec = objectSpec{
	fields: []objectField{
		{name: "processHandle", flags: fieldRequired | fieldRejectNull},
		{name: "size", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProcessResizePtyParams) MarshalJSON() ([]byte, error) {
	type wire ProcessResizePtyParams
	return json.Marshal(wire(v))
}

func (v *ProcessResizePtyParams) UnmarshalJSON(data []byte) error {
	type wire ProcessResizePtyParams
	return decodeObject(data, &processResizePtyParamsSpec, (*wire)(v))
}

type ProcessResizePtyResponse map[string]json.RawMessage

func (v ProcessResizePtyResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ProcessResizePtyResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ProcessResizePtyResponse(raw)
	return nil
}

type ProcessSpawnParams struct {
	Command            []string                              `json:"command"`
	Cwd                AbsolutePathBuf                       `json:"cwd"`
	Env                Optional[map[string]Optional[string]] `json:"env,omitzero"`
	OutputBytesCap     Optional[uint64]                      `json:"outputBytesCap,omitzero"`
	ProcessHandle      string                                `json:"processHandle"`
	Size               Optional[ProcessTerminalSize]         `json:"size,omitzero"`
	StreamStdin        OptionalNonNull[bool]                 `json:"streamStdin,omitzero"`
	StreamStdoutStderr OptionalNonNull[bool]                 `json:"streamStdoutStderr,omitzero"`
	TimeoutMs          Optional[int64]                       `json:"timeoutMs,omitzero"`
	Tty                OptionalNonNull[bool]                 `json:"tty,omitzero"`
}

var processSpawnParamsSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldRequired | fieldRejectNull},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "env"},
		{name: "outputBytesCap"},
		{name: "processHandle", flags: fieldRequired | fieldRejectNull},
		{name: "size"},
		{name: "streamStdin", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "streamStdoutStderr", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "timeoutMs"},
		{name: "tty", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v ProcessSpawnParams) MarshalJSON() ([]byte, error) {
	type wire ProcessSpawnParams
	w := wire(v)
	w.StreamStdin = nonNullWithoutFalse(w.StreamStdin)
	w.StreamStdoutStderr = nonNullWithoutFalse(w.StreamStdoutStderr)
	w.Tty = nonNullWithoutFalse(w.Tty)
	return json.Marshal(w)
}

func (v *ProcessSpawnParams) UnmarshalJSON(data []byte) error {
	type wire ProcessSpawnParams
	var s objectScan
	if _, err := s.begin(data, &processSpawnParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueOutputBytesCap, ok := v.OutputBytesCap.Value(); ok {
		if valueOutputBytesCap < 0 {
			return DecodeError{Field: "outputBytesCap", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ProcessSpawnResponse map[string]json.RawMessage

func (v ProcessSpawnResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ProcessSpawnResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ProcessSpawnResponse(raw)
	return nil
}

type ProcessTerminalSize struct {
	Cols uint16 `json:"cols"`
	Rows uint16 `json:"rows"`
}

var processTerminalSizeSpec = objectSpec{
	fields: []objectField{
		{name: "cols", flags: fieldRequired | fieldRejectNull},
		{name: "rows", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProcessTerminalSize) MarshalJSON() ([]byte, error) {
	type wire ProcessTerminalSize
	return json.Marshal(wire(v))
}

func (v *ProcessTerminalSize) UnmarshalJSON(data []byte) error {
	type wire ProcessTerminalSize
	var s objectScan
	if _, err := s.begin(data, &processTerminalSizeSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Cols < 0 {
		return DecodeError{Field: "cols", Reason: "below minimum 0"}
	}
	if v.Rows < 0 {
		return DecodeError{Field: "rows", Reason: "below minimum 0"}
	}
	return nil
}

type ProcessWriteStdinParams struct {
	CloseStdin    OptionalNonNull[bool] `json:"closeStdin,omitzero"`
	DeltaBase64   Optional[string]      `json:"deltaBase64,omitzero"`
	ProcessHandle string                `json:"processHandle"`
}

var processWriteStdinParamsSpec = objectSpec{
	fields: []objectField{
		{name: "closeStdin", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "deltaBase64"},
		{name: "processHandle", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProcessWriteStdinParams) MarshalJSON() ([]byte, error) {
	type wire ProcessWriteStdinParams
	w := wire(v)
	w.CloseStdin = nonNullWithoutFalse(w.CloseStdin)
	return json.Marshal(w)
}

func (v *ProcessWriteStdinParams) UnmarshalJSON(data []byte) error {
	type wire ProcessWriteStdinParams
	return decodeObject(data, &processWriteStdinParamsSpec, (*wire)(v))
}

type ProcessWriteStdinResponse map[string]json.RawMessage

func (v ProcessWriteStdinResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ProcessWriteStdinResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ProcessWriteStdinResponse(raw)
	return nil
}

type Project struct {
	CreatedAt int64             `json:"createdAt"`
	ID        string            `json:"id"`
	Metadata  map[string]string `json:"metadata"`
	Name      string            `json:"name"`
	Position  int64             `json:"position"`
	RecencyAt Optional[int64]   `json:"recencyAt,omitzero"`
	Roots     []ProjectRoot     `json:"roots"`
	UpdatedAt int64             `json:"updatedAt"`
}

var projectSpec = objectSpec{
	fields: []objectField{
		{name: "createdAt", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "metadata", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "position", flags: fieldRequired | fieldRejectNull},
		{name: "recencyAt"},
		{name: "roots", flags: fieldRequired | fieldRejectNull},
		{name: "updatedAt", flags: fieldRequired | fieldRejectNull},
	},
}

func (v Project) MarshalJSON() ([]byte, error) {
	type wire Project
	return json.Marshal(wire(v))
}

func (v *Project) UnmarshalJSON(data []byte) error {
	type wire Project
	return decodeObject(data, &projectSpec, (*wire)(v))
}

type ProjectChangeType string

const (
	ProjectChangeTypeCreated ProjectChangeType = "created"
	ProjectChangeTypeUpdated ProjectChangeType = "updated"
	ProjectChangeTypeDeleted ProjectChangeType = "deleted"
)

type ProjectChangedNotification struct {
	ChangeType ProjectChangeType `json:"changeType"`
	ProjectID  string            `json:"projectId"`
}

var projectChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "changeType", flags: fieldRequired | fieldRejectNull},
		{name: "projectId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectChangedNotification) MarshalJSON() ([]byte, error) {
	type wire ProjectChangedNotification
	return json.Marshal(wire(v))
}

func (v *ProjectChangedNotification) UnmarshalJSON(data []byte) error {
	type wire ProjectChangedNotification
	return decodeObject(data, &projectChangedNotificationSpec, (*wire)(v))
}

type ProjectCreateParams struct {
	IDempotencyKey string                      `json:"idempotencyKey"`
	Metadata       Optional[map[string]string] `json:"metadata,omitzero"`
	Name           string                      `json:"name"`
	Roots          []ProjectRoot               `json:"roots"`
}

var projectCreateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "idempotencyKey", flags: fieldRequired | fieldRejectNull},
		{name: "metadata"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "roots", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectCreateParams) MarshalJSON() ([]byte, error) {
	type wire ProjectCreateParams
	return json.Marshal(wire(v))
}

func (v *ProjectCreateParams) UnmarshalJSON(data []byte) error {
	type wire ProjectCreateParams
	return decodeObject(data, &projectCreateParamsSpec, (*wire)(v))
}

type ProjectCreateResponse struct {
	Project Project `json:"project"`
}

var projectCreateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "project", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectCreateResponse) MarshalJSON() ([]byte, error) {
	type wire ProjectCreateResponse
	return json.Marshal(wire(v))
}

func (v *ProjectCreateResponse) UnmarshalJSON(data []byte) error {
	type wire ProjectCreateResponse
	return decodeObject(data, &projectCreateResponseSpec, (*wire)(v))
}

type ProjectDeleteParams struct {
	ProjectID string `json:"projectId"`
}

var projectDeleteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "projectId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectDeleteParams) MarshalJSON() ([]byte, error) {
	type wire ProjectDeleteParams
	return json.Marshal(wire(v))
}

func (v *ProjectDeleteParams) UnmarshalJSON(data []byte) error {
	type wire ProjectDeleteParams
	return decodeObject(data, &projectDeleteParamsSpec, (*wire)(v))
}

type ProjectDeleteResponse map[string]json.RawMessage

func (v ProjectDeleteResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ProjectDeleteResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ProjectDeleteResponse(raw)
	return nil
}

type ProjectImportParams struct {
	IDempotencyKey string                      `json:"idempotencyKey"`
	Metadata       Optional[map[string]string] `json:"metadata,omitzero"`
	Name           string                      `json:"name"`
	Roots          []ProjectRoot               `json:"roots"`
	Threads        Optional[[]string]          `json:"threads,omitzero"`
}

var projectImportParamsSpec = objectSpec{
	fields: []objectField{
		{name: "idempotencyKey", flags: fieldRequired | fieldRejectNull},
		{name: "metadata"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "roots", flags: fieldRequired | fieldRejectNull},
		{name: "threads"},
	},
}

func (v ProjectImportParams) MarshalJSON() ([]byte, error) {
	type wire ProjectImportParams
	return json.Marshal(wire(v))
}

func (v *ProjectImportParams) UnmarshalJSON(data []byte) error {
	type wire ProjectImportParams
	return decodeObject(data, &projectImportParamsSpec, (*wire)(v))
}

type ProjectImportResponse struct {
	Project Project `json:"project"`
}

var projectImportResponseSpec = objectSpec{
	fields: []objectField{
		{name: "project", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectImportResponse) MarshalJSON() ([]byte, error) {
	type wire ProjectImportResponse
	return json.Marshal(wire(v))
}

func (v *ProjectImportResponse) UnmarshalJSON(data []byte) error {
	type wire ProjectImportResponse
	return decodeObject(data, &projectImportResponseSpec, (*wire)(v))
}

type ProjectListParams struct {
	Cursor        Optional[string]         `json:"cursor,omitzero"`
	Limit         Optional[uint32]         `json:"limit,omitzero"`
	SortDirection Optional[SortDirection]  `json:"sortDirection,omitzero"`
	SortKey       Optional[ProjectSortKey] `json:"sortKey,omitzero"`
}

var projectListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "sortDirection"},
		{name: "sortKey"},
	},
}

func (v ProjectListParams) MarshalJSON() ([]byte, error) {
	type wire ProjectListParams
	return json.Marshal(wire(v))
}

func (v *ProjectListParams) UnmarshalJSON(data []byte) error {
	type wire ProjectListParams
	var s objectScan
	if _, err := s.begin(data, &projectListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ProjectListResponse struct {
	Data       []Project        `json:"data"`
	NextCursor Optional[string] `json:"nextCursor,omitzero"`
}

var projectListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ProjectListResponse) MarshalJSON() ([]byte, error) {
	type wire ProjectListResponse
	return json.Marshal(wire(v))
}

func (v *ProjectListResponse) UnmarshalJSON(data []byte) error {
	type wire ProjectListResponse
	return decodeObject(data, &projectListResponseSpec, (*wire)(v))
}

type ProjectMoveParams struct {
	BeforeProjectID Optional[string] `json:"beforeProjectId,omitzero"`
	ProjectID       string           `json:"projectId"`
}

var projectMoveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "beforeProjectId"},
		{name: "projectId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectMoveParams) MarshalJSON() ([]byte, error) {
	type wire ProjectMoveParams
	return json.Marshal(wire(v))
}

func (v *ProjectMoveParams) UnmarshalJSON(data []byte) error {
	type wire ProjectMoveParams
	return decodeObject(data, &projectMoveParamsSpec, (*wire)(v))
}

type ProjectMoveResponse map[string]json.RawMessage

func (v ProjectMoveResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ProjectMoveResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ProjectMoveResponse(raw)
	return nil
}

type ProjectReadParams struct {
	ProjectID string `json:"projectId"`
}

var projectReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "projectId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectReadParams) MarshalJSON() ([]byte, error) {
	type wire ProjectReadParams
	return json.Marshal(wire(v))
}

func (v *ProjectReadParams) UnmarshalJSON(data []byte) error {
	type wire ProjectReadParams
	return decodeObject(data, &projectReadParamsSpec, (*wire)(v))
}

type ProjectReadResponse struct {
	Project Project `json:"project"`
}

var projectReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "project", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectReadResponse) MarshalJSON() ([]byte, error) {
	type wire ProjectReadResponse
	return json.Marshal(wire(v))
}

func (v *ProjectReadResponse) UnmarshalJSON(data []byte) error {
	type wire ProjectReadResponse
	return decodeObject(data, &projectReadResponseSpec, (*wire)(v))
}

type ProjectRoot struct {
	Path AbsolutePathBuf `json:"path"`
}

var projectRootSpec = objectSpec{
	fields: []objectField{
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectRoot) MarshalJSON() ([]byte, error) {
	type wire ProjectRoot
	return json.Marshal(wire(v))
}

func (v *ProjectRoot) UnmarshalJSON(data []byte) error {
	type wire ProjectRoot
	return decodeObject(data, &projectRootSpec, (*wire)(v))
}

type ProjectSortKey string

const (
	ProjectSortKeyPosition  ProjectSortKey = "position"
	ProjectSortKeyRecencyAt ProjectSortKey = "recencyAt"
)

type ProjectUpdateParams struct {
	Metadata  Optional[map[string]string] `json:"metadata,omitzero"`
	Name      Optional[string]            `json:"name,omitzero"`
	ProjectID string                      `json:"projectId"`
	Roots     Optional[[]ProjectRoot]     `json:"roots,omitzero"`
}

var projectUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "metadata"},
		{name: "name"},
		{name: "projectId", flags: fieldRequired | fieldRejectNull},
		{name: "roots"},
	},
}

func (v ProjectUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ProjectUpdateParams
	return json.Marshal(wire(v))
}

func (v *ProjectUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ProjectUpdateParams
	return decodeObject(data, &projectUpdateParamsSpec, (*wire)(v))
}

type ProjectUpdateResponse struct {
	Project Project `json:"project"`
}

var projectUpdateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "project", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ProjectUpdateResponse) MarshalJSON() ([]byte, error) {
	type wire ProjectUpdateResponse
	return json.Marshal(wire(v))
}

func (v *ProjectUpdateResponse) UnmarshalJSON(data []byte) error {
	type wire ProjectUpdateResponse
	return decodeObject(data, &projectUpdateResponseSpec, (*wire)(v))
}

type QueuedSubmission struct {
	ClientUserMessageID string      `json:"clientUserMessageId"`
	ID                  string      `json:"id"`
	Input               []UserInput `json:"input"`
}

var queuedSubmissionSpec = objectSpec{
	fields: []objectField{
		{name: "clientUserMessageId", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "input", flags: fieldRequired | fieldRejectNull},
	},
}

func (v QueuedSubmission) MarshalJSON() ([]byte, error) {
	type wire QueuedSubmission
	return json.Marshal(wire(v))
}

func (v *QueuedSubmission) UnmarshalJSON(data []byte) error {
	type wire QueuedSubmission
	return decodeObject(data, &queuedSubmissionSpec, (*wire)(v))
}

type RateLimitReachedType string

const (
	RateLimitReachedTypeRateLimitReached                 RateLimitReachedType = "rate_limit_reached"
	RateLimitReachedTypeWorkspaceOwnerCreditsDepleted    RateLimitReachedType = "workspace_owner_credits_depleted"
	RateLimitReachedTypeWorkspaceMemberCreditsDepleted   RateLimitReachedType = "workspace_member_credits_depleted"
	RateLimitReachedTypeWorkspaceOwnerUsageLimitReached  RateLimitReachedType = "workspace_owner_usage_limit_reached"
	RateLimitReachedTypeWorkspaceMemberUsageLimitReached RateLimitReachedType = "workspace_member_usage_limit_reached"
)

type RateLimitResetCredit struct {
	Description Optional[string]           `json:"description,omitzero"`
	ExpiresAt   Optional[int64]            `json:"expiresAt,omitzero"`
	GrantedAt   int64                      `json:"grantedAt"`
	ID          string                     `json:"id"`
	ResetType   RateLimitResetType         `json:"resetType"`
	Status      RateLimitResetCreditStatus `json:"status"`
	Title       Optional[string]           `json:"title,omitzero"`
}

var rateLimitResetCreditSpec = objectSpec{
	fields: []objectField{
		{name: "description"},
		{name: "expiresAt"},
		{name: "grantedAt", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "resetType", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
	},
}

func (v RateLimitResetCredit) MarshalJSON() ([]byte, error) {
	type wire RateLimitResetCredit
	return json.Marshal(wire(v))
}

func (v *RateLimitResetCredit) UnmarshalJSON(data []byte) error {
	type wire RateLimitResetCredit
	return decodeObject(data, &rateLimitResetCreditSpec, (*wire)(v))
}

type RateLimitResetCreditStatus string

const (
	RateLimitResetCreditStatusAvailable RateLimitResetCreditStatus = "available"
	RateLimitResetCreditStatusRedeeming RateLimitResetCreditStatus = "redeeming"
	RateLimitResetCreditStatusRedeemed  RateLimitResetCreditStatus = "redeemed"
	RateLimitResetCreditStatusUnknown   RateLimitResetCreditStatus = "unknown"
)

type RateLimitResetCreditsSummary struct {
	AvailableCount int64                            `json:"availableCount"`
	Credits        Optional[[]RateLimitResetCredit] `json:"credits,omitzero"`
}

var rateLimitResetCreditsSummarySpec = objectSpec{
	fields: []objectField{
		{name: "availableCount", flags: fieldRequired | fieldRejectNull},
		{name: "credits"},
	},
}

func (v RateLimitResetCreditsSummary) MarshalJSON() ([]byte, error) {
	type wire RateLimitResetCreditsSummary
	return json.Marshal(wire(v))
}

func (v *RateLimitResetCreditsSummary) UnmarshalJSON(data []byte) error {
	type wire RateLimitResetCreditsSummary
	return decodeObject(data, &rateLimitResetCreditsSummarySpec, (*wire)(v))
}

type RateLimitResetType string

const (
	RateLimitResetTypeCodexRateLimits RateLimitResetType = "codexRateLimits"
	RateLimitResetTypeUnknown         RateLimitResetType = "unknown"
)

type RateLimitSnapshot struct {
	Credits              Optional[CreditsSnapshot]           `json:"credits,omitzero"`
	IndividualLimit      Optional[SpendControlLimitSnapshot] `json:"individualLimit,omitzero"`
	LimitID              Optional[string]                    `json:"limitId,omitzero"`
	LimitName            Optional[string]                    `json:"limitName,omitzero"`
	NormalModelSlug      Optional[string]                    `json:"normalModelSlug,omitzero"`
	PlanType             Optional[PlanType]                  `json:"planType,omitzero"`
	Primary              Optional[RateLimitWindow]           `json:"primary,omitzero"`
	RateLimitReachedType Optional[RateLimitReachedType]      `json:"rateLimitReachedType,omitzero"`
	Secondary            Optional[RateLimitWindow]           `json:"secondary,omitzero"`
	SpendControlReached  Optional[bool]                      `json:"spendControlReached,omitzero"`
}

var rateLimitSnapshotSpec = objectSpec{
	fields: []objectField{
		{name: "credits"},
		{name: "individualLimit"},
		{name: "limitId"},
		{name: "limitName"},
		{name: "normalModelSlug"},
		{name: "planType"},
		{name: "primary"},
		{name: "rateLimitReachedType"},
		{name: "secondary"},
		{name: "spendControlReached"},
	},
}

func (v RateLimitSnapshot) MarshalJSON() ([]byte, error) {
	type wire RateLimitSnapshot
	return json.Marshal(wire(v))
}

func (v *RateLimitSnapshot) UnmarshalJSON(data []byte) error {
	type wire RateLimitSnapshot
	return decodeObject(data, &rateLimitSnapshotSpec, (*wire)(v))
}

type RateLimitWindow struct {
	ResetsAt           Optional[int64] `json:"resetsAt,omitzero"`
	UsedPercent        int32           `json:"usedPercent"`
	WindowDurationMins Optional[int64] `json:"windowDurationMins,omitzero"`
}

var rateLimitWindowSpec = objectSpec{
	fields: []objectField{
		{name: "resetsAt"},
		{name: "usedPercent", flags: fieldRequired | fieldRejectNull},
		{name: "windowDurationMins"},
	},
}

func (v RateLimitWindow) MarshalJSON() ([]byte, error) {
	type wire RateLimitWindow
	return json.Marshal(wire(v))
}

func (v *RateLimitWindow) UnmarshalJSON(data []byte) error {
	type wire RateLimitWindow
	return decodeObject(data, &rateLimitWindowSpec, (*wire)(v))
}

type RealtimeConversationVersion string

const (
	RealtimeConversationVersionV1 RealtimeConversationVersion = "v1"
	RealtimeConversationVersionV2 RealtimeConversationVersion = "v2"
	RealtimeConversationVersionV3 RealtimeConversationVersion = "v3"
)

type RealtimeOutputModality string

const (
	RealtimeOutputModalityText  RealtimeOutputModality = "text"
	RealtimeOutputModalityAudio RealtimeOutputModality = "audio"
)

type RealtimeVoice string

const (
	RealtimeVoiceAlloy   RealtimeVoice = "alloy"
	RealtimeVoiceArbor   RealtimeVoice = "arbor"
	RealtimeVoiceAsh     RealtimeVoice = "ash"
	RealtimeVoiceAube    RealtimeVoice = "aube"
	RealtimeVoiceBallad  RealtimeVoice = "ballad"
	RealtimeVoiceBreeze  RealtimeVoice = "breeze"
	RealtimeVoiceBubbie  RealtimeVoice = "bubbie"
	RealtimeVoiceCedar   RealtimeVoice = "cedar"
	RealtimeVoiceCoral   RealtimeVoice = "coral"
	RealtimeVoiceCove    RealtimeVoice = "cove"
	RealtimeVoiceEcho    RealtimeVoice = "echo"
	RealtimeVoiceEmber   RealtimeVoice = "ember"
	RealtimeVoiceHaetsal RealtimeVoice = "haetsal"
	RealtimeVoiceHanul   RealtimeVoice = "hanul"
	RealtimeVoiceHimari  RealtimeVoice = "himari"
	RealtimeVoiceJuniper RealtimeVoice = "juniper"
	RealtimeVoiceLeher   RealtimeVoice = "leher"
	RealtimeVoiceMaple   RealtimeVoice = "maple"
	RealtimeVoiceMarin   RealtimeVoice = "marin"
	RealtimeVoiceMiko    RealtimeVoice = "miko"
	RealtimeVoiceNeer    RealtimeVoice = "neer"
	RealtimeVoicePorto   RealtimeVoice = "porto"
	RealtimeVoiceRindo   RealtimeVoice = "rindo"
	RealtimeVoiceRio     RealtimeVoice = "rio"
	RealtimeVoiceRivage  RealtimeVoice = "rivage"
	RealtimeVoiceSage    RealtimeVoice = "sage"
	RealtimeVoiceSelva   RealtimeVoice = "selva"
	RealtimeVoiceShimmer RealtimeVoice = "shimmer"
	RealtimeVoiceSol     RealtimeVoice = "sol"
	RealtimeVoiceSonna   RealtimeVoice = "sonna"
	RealtimeVoiceSpruce  RealtimeVoice = "spruce"
	RealtimeVoiceTinta   RealtimeVoice = "tinta"
	RealtimeVoiceVale    RealtimeVoice = "vale"
	RealtimeVoiceVerse   RealtimeVoice = "verse"
	RealtimeVoiceViola   RealtimeVoice = "viola"
)

type RealtimeVoicesList struct {
	DefaultV1 RealtimeVoice                    `json:"defaultV1"`
	DefaultV2 RealtimeVoice                    `json:"defaultV2"`
	V1        []RealtimeVoice                  `json:"v1"`
	V2        []RealtimeVoice                  `json:"v2"`
	V3        OptionalNonNull[[]RealtimeVoice] `json:"v3,omitzero"`
}

var realtimeVoicesListSpec = objectSpec{
	fields: []objectField{
		{name: "defaultV1", flags: fieldRequired | fieldRejectNull},
		{name: "defaultV2", flags: fieldRequired | fieldRejectNull},
		{name: "v1", flags: fieldRequired | fieldRejectNull},
		{name: "v2", flags: fieldRequired | fieldRejectNull},
		{name: "v3", flags: fieldNonNullOptional},
	},
}

func (v RealtimeVoicesList) MarshalJSON() ([]byte, error) {
	type wire RealtimeVoicesList
	return json.Marshal(wire(v))
}

func (v *RealtimeVoicesList) UnmarshalJSON(data []byte) error {
	type wire RealtimeVoicesList
	return decodeObject(data, &realtimeVoicesListSpec, (*wire)(v))
}

type ReasoningEffort string

type ReasoningEffortOption struct {
	Description     string          `json:"description"`
	ReasoningEffort ReasoningEffort `json:"reasoningEffort"`
}

var reasoningEffortOptionSpec = objectSpec{
	fields: []objectField{
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "reasoningEffort", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ReasoningEffortOption) MarshalJSON() ([]byte, error) {
	type wire ReasoningEffortOption
	return json.Marshal(wire(v))
}

func (v *ReasoningEffortOption) UnmarshalJSON(data []byte) error {
	type wire ReasoningEffortOption
	return decodeObject(data, &reasoningEffortOptionSpec, (*wire)(v))
}

type ReasoningItemContent struct {
	Text      OptionalNonNull[string] `json:"text,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var reasoningItemContentSpec = objectSpec{
	fields: []objectField{
		{name: "text", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"reasoning_text", "text"},
}

func (v ReasoningItemContent) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "reasoning_text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type reasoning_text"}
		}
	case "text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type text"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ReasoningItemContent
	return json.Marshal(wire(v))
}

func (v *ReasoningItemContent) UnmarshalJSON(data []byte) error {
	type wire ReasoningItemContent
	var s objectScan
	known, err := s.begin(data, &reasoningItemContentSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "reasoning_text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type reasoning_text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ReasoningItemReasoningSummary struct {
	Text      OptionalNonNull[string] `json:"text,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var reasoningItemReasoningSummarySpec = objectSpec{
	fields: []objectField{
		{name: "text", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"summary_text"},
}

func (v ReasoningItemReasoningSummary) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "summary_text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type summary_text"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ReasoningItemReasoningSummary
	return json.Marshal(wire(v))
}

func (v *ReasoningItemReasoningSummary) UnmarshalJSON(data []byte) error {
	type wire ReasoningItemReasoningSummary
	var s objectScan
	known, err := s.begin(data, &reasoningItemReasoningSummarySpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "summary_text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type summary_text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ReasoningSummary = json.RawMessage

type ReasoningSummaryPartAddedNotification struct {
	ItemID       string `json:"itemId"`
	SummaryIndex int64  `json:"summaryIndex"`
	ThreadID     string `json:"threadId"`
	TurnID       string `json:"turnId"`
}

var reasoningSummaryPartAddedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "summaryIndex", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ReasoningSummaryPartAddedNotification) MarshalJSON() ([]byte, error) {
	type wire ReasoningSummaryPartAddedNotification
	return json.Marshal(wire(v))
}

func (v *ReasoningSummaryPartAddedNotification) UnmarshalJSON(data []byte) error {
	type wire ReasoningSummaryPartAddedNotification
	return decodeObject(data, &reasoningSummaryPartAddedNotificationSpec, (*wire)(v))
}

type ReasoningSummaryTextDeltaNotification struct {
	Delta        string `json:"delta"`
	ItemID       string `json:"itemId"`
	SummaryIndex int64  `json:"summaryIndex"`
	ThreadID     string `json:"threadId"`
	TurnID       string `json:"turnId"`
}

var reasoningSummaryTextDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "summaryIndex", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ReasoningSummaryTextDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire ReasoningSummaryTextDeltaNotification
	return json.Marshal(wire(v))
}

func (v *ReasoningSummaryTextDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire ReasoningSummaryTextDeltaNotification
	return decodeObject(data, &reasoningSummaryTextDeltaNotificationSpec, (*wire)(v))
}

type ReasoningTextDeltaNotification struct {
	ContentIndex int64  `json:"contentIndex"`
	Delta        string `json:"delta"`
	ItemID       string `json:"itemId"`
	ThreadID     string `json:"threadId"`
	TurnID       string `json:"turnId"`
}

var reasoningTextDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "contentIndex", flags: fieldRequired | fieldRejectNull},
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ReasoningTextDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire ReasoningTextDeltaNotification
	return json.Marshal(wire(v))
}

func (v *ReasoningTextDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire ReasoningTextDeltaNotification
	return decodeObject(data, &reasoningTextDeltaNotificationSpec, (*wire)(v))
}

type RemoteControlClient struct {
	AppVersion  Optional[string] `json:"appVersion,omitzero"`
	ClientID    string           `json:"clientId"`
	DeviceModel Optional[string] `json:"deviceModel,omitzero"`
	DeviceType  Optional[string] `json:"deviceType,omitzero"`
	DisplayName Optional[string] `json:"displayName,omitzero"`
	LastSeenAt  Optional[int64]  `json:"lastSeenAt,omitzero"`
	OsVersion   Optional[string] `json:"osVersion,omitzero"`
	Platform    Optional[string] `json:"platform,omitzero"`
}

var remoteControlClientSpec = objectSpec{
	fields: []objectField{
		{name: "appVersion"},
		{name: "clientId", flags: fieldRequired | fieldRejectNull},
		{name: "deviceModel"},
		{name: "deviceType"},
		{name: "displayName"},
		{name: "lastSeenAt"},
		{name: "osVersion"},
		{name: "platform"},
	},
}

func (v RemoteControlClient) MarshalJSON() ([]byte, error) {
	type wire RemoteControlClient
	return json.Marshal(wire(v))
}

func (v *RemoteControlClient) UnmarshalJSON(data []byte) error {
	type wire RemoteControlClient
	return decodeObject(data, &remoteControlClientSpec, (*wire)(v))
}

type RemoteControlClientsListOrder string

const (
	RemoteControlClientsListOrderAsc  RemoteControlClientsListOrder = "asc"
	RemoteControlClientsListOrderDesc RemoteControlClientsListOrder = "desc"
)

type RemoteControlClientsListParams struct {
	Cursor        Optional[string]                        `json:"cursor,omitzero"`
	EnvironmentID string                                  `json:"environmentId"`
	Limit         Optional[uint32]                        `json:"limit,omitzero"`
	Order         Optional[RemoteControlClientsListOrder] `json:"order,omitzero"`
}

var remoteControlClientsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
		{name: "limit"},
		{name: "order"},
	},
}

func (v RemoteControlClientsListParams) MarshalJSON() ([]byte, error) {
	type wire RemoteControlClientsListParams
	return json.Marshal(wire(v))
}

func (v *RemoteControlClientsListParams) UnmarshalJSON(data []byte) error {
	type wire RemoteControlClientsListParams
	var s objectScan
	if _, err := s.begin(data, &remoteControlClientsListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type RemoteControlClientsListResponse struct {
	Data       []RemoteControlClient `json:"data"`
	NextCursor Optional[string]      `json:"nextCursor,omitzero"`
}

var remoteControlClientsListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v RemoteControlClientsListResponse) MarshalJSON() ([]byte, error) {
	type wire RemoteControlClientsListResponse
	return json.Marshal(wire(v))
}

func (v *RemoteControlClientsListResponse) UnmarshalJSON(data []byte) error {
	type wire RemoteControlClientsListResponse
	return decodeObject(data, &remoteControlClientsListResponseSpec, (*wire)(v))
}

type RemoteControlClientsRevokeParams struct {
	ClientID      string `json:"clientId"`
	EnvironmentID string `json:"environmentId"`
}

var remoteControlClientsRevokeParamsSpec = objectSpec{
	fields: []objectField{
		{name: "clientId", flags: fieldRequired | fieldRejectNull},
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlClientsRevokeParams) MarshalJSON() ([]byte, error) {
	type wire RemoteControlClientsRevokeParams
	return json.Marshal(wire(v))
}

func (v *RemoteControlClientsRevokeParams) UnmarshalJSON(data []byte) error {
	type wire RemoteControlClientsRevokeParams
	return decodeObject(data, &remoteControlClientsRevokeParamsSpec, (*wire)(v))
}

type RemoteControlClientsRevokeResponse map[string]json.RawMessage

func (v RemoteControlClientsRevokeResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *RemoteControlClientsRevokeResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = RemoteControlClientsRevokeResponse(raw)
	return nil
}

type RemoteControlConnectionStatus string

const (
	RemoteControlConnectionStatusDisabled   RemoteControlConnectionStatus = "disabled"
	RemoteControlConnectionStatusConnecting RemoteControlConnectionStatus = "connecting"
	RemoteControlConnectionStatusConnected  RemoteControlConnectionStatus = "connected"
	RemoteControlConnectionStatusErrored    RemoteControlConnectionStatus = "errored"
)

type RemoteControlDisableParams struct {
	Ephemeral OptionalNonNull[bool] `json:"ephemeral,omitzero"`
}

var remoteControlDisableParamsSpec = objectSpec{
	fields: []objectField{
		{name: "ephemeral", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v RemoteControlDisableParams) MarshalJSON() ([]byte, error) {
	type wire RemoteControlDisableParams
	w := wire(v)
	w.Ephemeral = nonNullWithoutFalse(w.Ephemeral)
	return json.Marshal(w)
}

func (v *RemoteControlDisableParams) UnmarshalJSON(data []byte) error {
	type wire RemoteControlDisableParams
	return decodeObject(data, &remoteControlDisableParamsSpec, (*wire)(v))
}

type RemoteControlDisableResponse struct {
	EnvironmentID  Optional[string]              `json:"environmentId,omitzero"`
	InstallationID string                        `json:"installationId"`
	ServerName     string                        `json:"serverName"`
	Status         RemoteControlConnectionStatus `json:"status"`
}

var remoteControlDisableResponseSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId"},
		{name: "installationId", flags: fieldRequired | fieldRejectNull},
		{name: "serverName", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlDisableResponse) MarshalJSON() ([]byte, error) {
	type wire RemoteControlDisableResponse
	return json.Marshal(wire(v))
}

func (v *RemoteControlDisableResponse) UnmarshalJSON(data []byte) error {
	type wire RemoteControlDisableResponse
	return decodeObject(data, &remoteControlDisableResponseSpec, (*wire)(v))
}

type RemoteControlEnableParams struct {
	Ephemeral OptionalNonNull[bool] `json:"ephemeral,omitzero"`
}

var remoteControlEnableParamsSpec = objectSpec{
	fields: []objectField{
		{name: "ephemeral", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v RemoteControlEnableParams) MarshalJSON() ([]byte, error) {
	type wire RemoteControlEnableParams
	w := wire(v)
	w.Ephemeral = nonNullWithoutFalse(w.Ephemeral)
	return json.Marshal(w)
}

func (v *RemoteControlEnableParams) UnmarshalJSON(data []byte) error {
	type wire RemoteControlEnableParams
	return decodeObject(data, &remoteControlEnableParamsSpec, (*wire)(v))
}

type RemoteControlEnableResponse struct {
	EnvironmentID  Optional[string]              `json:"environmentId,omitzero"`
	InstallationID string                        `json:"installationId"`
	ServerName     string                        `json:"serverName"`
	Status         RemoteControlConnectionStatus `json:"status"`
}

var remoteControlEnableResponseSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId"},
		{name: "installationId", flags: fieldRequired | fieldRejectNull},
		{name: "serverName", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlEnableResponse) MarshalJSON() ([]byte, error) {
	type wire RemoteControlEnableResponse
	return json.Marshal(wire(v))
}

func (v *RemoteControlEnableResponse) UnmarshalJSON(data []byte) error {
	type wire RemoteControlEnableResponse
	return decodeObject(data, &remoteControlEnableResponseSpec, (*wire)(v))
}

type RemoteControlPairingStartParams struct {
	ManualCode OptionalNonNull[bool] `json:"manualCode,omitzero"`
}

var remoteControlPairingStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "manualCode", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v RemoteControlPairingStartParams) MarshalJSON() ([]byte, error) {
	type wire RemoteControlPairingStartParams
	w := wire(v)
	w.ManualCode = nonNullWithoutFalse(w.ManualCode)
	return json.Marshal(w)
}

func (v *RemoteControlPairingStartParams) UnmarshalJSON(data []byte) error {
	type wire RemoteControlPairingStartParams
	return decodeObject(data, &remoteControlPairingStartParamsSpec, (*wire)(v))
}

type RemoteControlPairingStartResponse struct {
	EnvironmentID     string           `json:"environmentId"`
	ExpiresAt         int64            `json:"expiresAt"`
	ManualPairingCode Optional[string] `json:"manualPairingCode,omitzero"`
	PairingCode       string           `json:"pairingCode"`
}

var remoteControlPairingStartResponseSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
		{name: "expiresAt", flags: fieldRequired | fieldRejectNull},
		{name: "manualPairingCode"},
		{name: "pairingCode", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlPairingStartResponse) MarshalJSON() ([]byte, error) {
	type wire RemoteControlPairingStartResponse
	return json.Marshal(wire(v))
}

func (v *RemoteControlPairingStartResponse) UnmarshalJSON(data []byte) error {
	type wire RemoteControlPairingStartResponse
	return decodeObject(data, &remoteControlPairingStartResponseSpec, (*wire)(v))
}

type RemoteControlPairingStatusParams struct {
	ManualPairingCode Optional[string] `json:"manualPairingCode,omitzero"`
	PairingCode       Optional[string] `json:"pairingCode,omitzero"`
}

var remoteControlPairingStatusParamsSpec = objectSpec{
	fields: []objectField{
		{name: "manualPairingCode"},
		{name: "pairingCode"},
	},
}

func (v RemoteControlPairingStatusParams) MarshalJSON() ([]byte, error) {
	type wire RemoteControlPairingStatusParams
	return json.Marshal(wire(v))
}

func (v *RemoteControlPairingStatusParams) UnmarshalJSON(data []byte) error {
	type wire RemoteControlPairingStatusParams
	return decodeObject(data, &remoteControlPairingStatusParamsSpec, (*wire)(v))
}

type RemoteControlPairingStatusResponse struct {
	Claimed bool `json:"claimed"`
}

var remoteControlPairingStatusResponseSpec = objectSpec{
	fields: []objectField{
		{name: "claimed", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlPairingStatusResponse) MarshalJSON() ([]byte, error) {
	type wire RemoteControlPairingStatusResponse
	return json.Marshal(wire(v))
}

func (v *RemoteControlPairingStatusResponse) UnmarshalJSON(data []byte) error {
	type wire RemoteControlPairingStatusResponse
	return decodeObject(data, &remoteControlPairingStatusResponseSpec, (*wire)(v))
}

type RemoteControlStatusChangedNotification struct {
	EnvironmentID  Optional[string]              `json:"environmentId,omitzero"`
	InstallationID string                        `json:"installationId"`
	ServerName     string                        `json:"serverName"`
	Status         RemoteControlConnectionStatus `json:"status"`
}

var remoteControlStatusChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId"},
		{name: "installationId", flags: fieldRequired | fieldRejectNull},
		{name: "serverName", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlStatusChangedNotification) MarshalJSON() ([]byte, error) {
	type wire RemoteControlStatusChangedNotification
	return json.Marshal(wire(v))
}

func (v *RemoteControlStatusChangedNotification) UnmarshalJSON(data []byte) error {
	type wire RemoteControlStatusChangedNotification
	return decodeObject(data, &remoteControlStatusChangedNotificationSpec, (*wire)(v))
}

type RemoteControlStatusReadResponse struct {
	EnvironmentID  Optional[string]              `json:"environmentId,omitzero"`
	InstallationID string                        `json:"installationId"`
	ServerName     string                        `json:"serverName"`
	Status         RemoteControlConnectionStatus `json:"status"`
}

var remoteControlStatusReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "environmentId"},
		{name: "installationId", flags: fieldRequired | fieldRejectNull},
		{name: "serverName", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RemoteControlStatusReadResponse) MarshalJSON() ([]byte, error) {
	type wire RemoteControlStatusReadResponse
	return json.Marshal(wire(v))
}

func (v *RemoteControlStatusReadResponse) UnmarshalJSON(data []byte) error {
	type wire RemoteControlStatusReadResponse
	return decodeObject(data, &remoteControlStatusReadResponseSpec, (*wire)(v))
}

type RequestHeader struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}

var requestHeaderSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "value", flags: fieldRequired | fieldRejectNull},
	},
}

func (v RequestHeader) MarshalJSON() ([]byte, error) {
	type wire RequestHeader
	return json.Marshal(wire(v))
}

func (v *RequestHeader) UnmarshalJSON(data []byte) error {
	type wire RequestHeader
	return decodeObject(data, &requestHeaderSpec, (*wire)(v))
}

type V2RequestID = json.RawMessage

type RequestPermissionProfile struct {
	FileSystem Optional[AdditionalFileSystemPermissions] `json:"fileSystem,omitzero"`
	Network    Optional[AdditionalNetworkPermissions]    `json:"network,omitzero"`
}

var requestPermissionProfileSpec = objectSpec{
	fields: []objectField{
		{name: "fileSystem"},
		{name: "network"},
	},
}

func (v RequestPermissionProfile) MarshalJSON() ([]byte, error) {
	type wire RequestPermissionProfile
	return json.Marshal(wire(v))
}

func (v *RequestPermissionProfile) UnmarshalJSON(data []byte) error {
	type wire RequestPermissionProfile
	return decodeObject(data, &requestPermissionProfileSpec, (*wire)(v))
}

type ResidencyRequirement string

const (
	ResidencyRequirementUs ResidencyRequirement = "us"
)

type Resource struct {
	Meta        json.RawMessage             `json:"_meta,omitempty"`
	Annotations json.RawMessage             `json:"annotations,omitempty"`
	Description Optional[string]            `json:"description,omitzero"`
	Icons       Optional[[]json.RawMessage] `json:"icons,omitzero"`
	MimeType    Optional[string]            `json:"mimeType,omitzero"`
	Name        string                      `json:"name"`
	Size        Optional[int64]             `json:"size,omitzero"`
	Title       Optional[string]            `json:"title,omitzero"`
	Uri         string                      `json:"uri"`
}

var resourceSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "annotations"},
		{name: "description"},
		{name: "icons"},
		{name: "mimeType"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "size"},
		{name: "title"},
		{name: "uri", flags: fieldRequired | fieldRejectNull},
	},
}

func (v Resource) MarshalJSON() ([]byte, error) {
	type wire Resource
	return json.Marshal(wire(v))
}

func (v *Resource) UnmarshalJSON(data []byte) error {
	type wire Resource
	return decodeObject(data, &resourceSpec, (*wire)(v))
}

type ResourceContent = json.RawMessage

type ResourceTemplate struct {
	Annotations json.RawMessage  `json:"annotations,omitempty"`
	Description Optional[string] `json:"description,omitzero"`
	MimeType    Optional[string] `json:"mimeType,omitzero"`
	Name        string           `json:"name"`
	Title       Optional[string] `json:"title,omitzero"`
	UriTemplate string           `json:"uriTemplate"`
}

var resourceTemplateSpec = objectSpec{
	fields: []objectField{
		{name: "annotations"},
		{name: "description"},
		{name: "mimeType"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
		{name: "uriTemplate", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ResourceTemplate) MarshalJSON() ([]byte, error) {
	type wire ResourceTemplate
	return json.Marshal(wire(v))
}

func (v *ResourceTemplate) UnmarshalJSON(data []byte) error {
	type wire ResourceTemplate
	return decodeObject(data, &resourceTemplateSpec, (*wire)(v))
}

type ResponseItem struct {
	Action                                 json.RawMessage                                  `json:"action,omitempty"`
	Arguments                              json.RawMessage                                  `json:"arguments,omitempty"`
	Author                                 OptionalNonNull[string]                          `json:"author,omitzero"`
	CallID                                 Optional[string]                                 `json:"call_id,omitzero"`
	Content                                json.RawMessage                                  `json:"content,omitempty"`
	EncryptedContent                       Optional[string]                                 `json:"encrypted_content,omitzero"`
	EncryptedFunctionArgs                  Optional[[]string]                               `json:"encrypted_function_args,omitzero"`
	Execution                              OptionalNonNull[string]                          `json:"execution,omitzero"`
	ID                                     Optional[string]                                 `json:"id,omitzero"`
	Input                                  OptionalNonNull[string]                          `json:"input,omitzero"`
	InternalChatMessageMetadataPassthrough Optional[InternalChatMessageMetadataPassthrough] `json:"internal_chat_message_metadata_passthrough,omitzero"`
	Name                                   Optional[string]                                 `json:"name,omitzero"`
	Namespace                              Optional[string]                                 `json:"namespace,omitzero"`
	Output                                 OptionalNonNull[FunctionCallOutputBody]          `json:"output,omitzero"`
	Phase                                  Optional[MessagePhase]                           `json:"phase,omitzero"`
	Reasoning                              OptionalNonNull[ConfigurationReasoning]          `json:"reasoning,omitzero"`
	Recipient                              OptionalNonNull[string]                          `json:"recipient,omitzero"`
	Result                                 OptionalNonNull[string]                          `json:"result,omitzero"`
	RevisedPrompt                          Optional[string]                                 `json:"revised_prompt,omitzero"`
	Role                                   OptionalNonNull[string]                          `json:"role,omitzero"`
	Status                                 Optional[string]                                 `json:"status,omitzero"`
	Summary                                OptionalNonNull[[]ReasoningItemReasoningSummary] `json:"summary,omitzero"`
	Tools                                  OptionalNonNull[[]json.RawMessage]               `json:"tools,omitzero"`
	TypeValue                              string                                           `json:"type"`
	RawJSON                                json.RawMessage                                  `json:"-"`
}

var responseItemSpec = objectSpec{
	fields: []objectField{
		{name: "action"},
		{name: "arguments"},
		{name: "author", flags: fieldNonNullOptional},
		{name: "call_id"},
		{name: "content"},
		{name: "encrypted_content"},
		{name: "encrypted_function_args"},
		{name: "execution", flags: fieldNonNullOptional},
		{name: "id"},
		{name: "input", flags: fieldNonNullOptional},
		{name: "internal_chat_message_metadata_passthrough"},
		{name: "name"},
		{name: "namespace"},
		{name: "output", flags: fieldNonNullOptional},
		{name: "phase"},
		{name: "reasoning", flags: fieldNonNullOptional},
		{name: "recipient", flags: fieldNonNullOptional},
		{name: "result", flags: fieldNonNullOptional},
		{name: "revised_prompt"},
		{name: "role", flags: fieldNonNullOptional},
		{name: "status"},
		{name: "summary", flags: fieldNonNullOptional},
		{name: "tools", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  23,
	tags: []string{"additional_tools", "message", "agent_message", "reasoning", "local_shell_call", "function_call", "tool_search_call", "function_call_output", "custom_tool_call", "custom_tool_call_output", "tool_search_output", "web_search_call", "image_generation_call", "compaction", "configuration_update", "compaction_trigger", "context_compaction", "other"},
}

func (v ResponseItem) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "additional_tools":
		if !v.Role.IsSet() {
			return nil, DecodeError{Field: "role", Reason: "missing required field for type additional_tools"}
		}
		if !v.Tools.IsSet() {
			return nil, DecodeError{Field: "tools", Reason: "missing required field for type additional_tools"}
		}
	case "message":
		if len(v.Content) == 0 {
			return nil, DecodeError{Field: "content", Reason: "missing required field for type message"}
		}
		if bytes.Equal(v.Content, []byte("null")) {
			return nil, DecodeError{Field: "content", Reason: "cannot be null"}
		}
		if !v.Role.IsSet() {
			return nil, DecodeError{Field: "role", Reason: "missing required field for type message"}
		}
	case "agent_message":
		if !v.Author.IsSet() {
			return nil, DecodeError{Field: "author", Reason: "missing required field for type agent_message"}
		}
		if len(v.Content) == 0 {
			return nil, DecodeError{Field: "content", Reason: "missing required field for type agent_message"}
		}
		if bytes.Equal(v.Content, []byte("null")) {
			return nil, DecodeError{Field: "content", Reason: "cannot be null"}
		}
		if !v.Recipient.IsSet() {
			return nil, DecodeError{Field: "recipient", Reason: "missing required field for type agent_message"}
		}
	case "reasoning":
		if !v.Summary.IsSet() {
			return nil, DecodeError{Field: "summary", Reason: "missing required field for type reasoning"}
		}
	case "local_shell_call":
		if len(v.Action) == 0 {
			return nil, DecodeError{Field: "action", Reason: "missing required field for type local_shell_call"}
		}
		if bytes.Equal(v.Action, []byte("null")) {
			return nil, DecodeError{Field: "action", Reason: "cannot be null"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type local_shell_call"}
		}
		if v.Status.IsNull() {
			return nil, DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "function_call":
		if len(v.Arguments) == 0 {
			return nil, DecodeError{Field: "arguments", Reason: "missing required field for type function_call"}
		}
		if bytes.Equal(v.Arguments, []byte("null")) {
			return nil, DecodeError{Field: "arguments", Reason: "cannot be null"}
		}
		if !v.CallID.IsSet() {
			return nil, DecodeError{Field: "call_id", Reason: "missing required field for type function_call"}
		}
		if v.CallID.IsNull() {
			return nil, DecodeError{Field: "call_id", Reason: "cannot be null"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type function_call"}
		}
		if v.Name.IsNull() {
			return nil, DecodeError{Field: "name", Reason: "cannot be null"}
		}
	case "tool_search_call":
		if len(v.Arguments) == 0 {
			return nil, DecodeError{Field: "arguments", Reason: "missing required field for type tool_search_call"}
		}
		if !v.Execution.IsSet() {
			return nil, DecodeError{Field: "execution", Reason: "missing required field for type tool_search_call"}
		}
	case "function_call_output":
		if !v.Output.IsSet() {
			return nil, DecodeError{Field: "output", Reason: "missing required field for type function_call_output"}
		}
	case "custom_tool_call":
		if !v.CallID.IsSet() {
			return nil, DecodeError{Field: "call_id", Reason: "missing required field for type custom_tool_call"}
		}
		if v.CallID.IsNull() {
			return nil, DecodeError{Field: "call_id", Reason: "cannot be null"}
		}
		if !v.Input.IsSet() {
			return nil, DecodeError{Field: "input", Reason: "missing required field for type custom_tool_call"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type custom_tool_call"}
		}
		if v.Name.IsNull() {
			return nil, DecodeError{Field: "name", Reason: "cannot be null"}
		}
	case "custom_tool_call_output":
		if !v.CallID.IsSet() {
			return nil, DecodeError{Field: "call_id", Reason: "missing required field for type custom_tool_call_output"}
		}
		if v.CallID.IsNull() {
			return nil, DecodeError{Field: "call_id", Reason: "cannot be null"}
		}
		if !v.Output.IsSet() {
			return nil, DecodeError{Field: "output", Reason: "missing required field for type custom_tool_call_output"}
		}
	case "tool_search_output":
		if !v.Execution.IsSet() {
			return nil, DecodeError{Field: "execution", Reason: "missing required field for type tool_search_output"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type tool_search_output"}
		}
		if v.Status.IsNull() {
			return nil, DecodeError{Field: "status", Reason: "cannot be null"}
		}
		if !v.Tools.IsSet() {
			return nil, DecodeError{Field: "tools", Reason: "missing required field for type tool_search_output"}
		}
	case "web_search_call":
	case "image_generation_call":
		if !v.Result.IsSet() {
			return nil, DecodeError{Field: "result", Reason: "missing required field for type image_generation_call"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type image_generation_call"}
		}
		if v.Status.IsNull() {
			return nil, DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "compaction":
		if !v.EncryptedContent.IsSet() {
			return nil, DecodeError{Field: "encrypted_content", Reason: "missing required field for type compaction"}
		}
		if v.EncryptedContent.IsNull() {
			return nil, DecodeError{Field: "encrypted_content", Reason: "cannot be null"}
		}
	case "configuration_update":
		if !v.Reasoning.IsSet() {
			return nil, DecodeError{Field: "reasoning", Reason: "missing required field for type configuration_update"}
		}
	case "compaction_trigger":
	case "context_compaction":
	case "other":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ResponseItem
	return json.Marshal(wire(v))
}

func (v *ResponseItem) UnmarshalJSON(data []byte) error {
	type wire ResponseItem
	var s objectScan
	known, err := s.begin(data, &responseItemSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "additional_tools":
		if !s.has("role") {
			return DecodeError{Field: "role", Reason: "missing required field for type additional_tools"}
		}
		if s.isNull("role") {
			return DecodeError{Field: "role", Reason: "cannot be null"}
		}
		if !s.has("tools") {
			return DecodeError{Field: "tools", Reason: "missing required field for type additional_tools"}
		}
		if s.isNull("tools") {
			return DecodeError{Field: "tools", Reason: "cannot be null"}
		}
	case "message":
		if !s.has("content") {
			return DecodeError{Field: "content", Reason: "missing required field for type message"}
		}
		if s.isNull("content") {
			return DecodeError{Field: "content", Reason: "cannot be null"}
		}
		if !s.has("role") {
			return DecodeError{Field: "role", Reason: "missing required field for type message"}
		}
		if s.isNull("role") {
			return DecodeError{Field: "role", Reason: "cannot be null"}
		}
	case "agent_message":
		if !s.has("author") {
			return DecodeError{Field: "author", Reason: "missing required field for type agent_message"}
		}
		if s.isNull("author") {
			return DecodeError{Field: "author", Reason: "cannot be null"}
		}
		if !s.has("content") {
			return DecodeError{Field: "content", Reason: "missing required field for type agent_message"}
		}
		if s.isNull("content") {
			return DecodeError{Field: "content", Reason: "cannot be null"}
		}
		if !s.has("recipient") {
			return DecodeError{Field: "recipient", Reason: "missing required field for type agent_message"}
		}
		if s.isNull("recipient") {
			return DecodeError{Field: "recipient", Reason: "cannot be null"}
		}
	case "reasoning":
		if !s.has("summary") {
			return DecodeError{Field: "summary", Reason: "missing required field for type reasoning"}
		}
		if s.isNull("summary") {
			return DecodeError{Field: "summary", Reason: "cannot be null"}
		}
	case "local_shell_call":
		if !s.has("action") {
			return DecodeError{Field: "action", Reason: "missing required field for type local_shell_call"}
		}
		if s.isNull("action") {
			return DecodeError{Field: "action", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type local_shell_call"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "function_call":
		if !s.has("arguments") {
			return DecodeError{Field: "arguments", Reason: "missing required field for type function_call"}
		}
		if s.isNull("arguments") {
			return DecodeError{Field: "arguments", Reason: "cannot be null"}
		}
		if !s.has("call_id") {
			return DecodeError{Field: "call_id", Reason: "missing required field for type function_call"}
		}
		if s.isNull("call_id") {
			return DecodeError{Field: "call_id", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type function_call"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
	case "tool_search_call":
		if !s.has("arguments") {
			return DecodeError{Field: "arguments", Reason: "missing required field for type tool_search_call"}
		}
		if !s.has("execution") {
			return DecodeError{Field: "execution", Reason: "missing required field for type tool_search_call"}
		}
		if s.isNull("execution") {
			return DecodeError{Field: "execution", Reason: "cannot be null"}
		}
	case "function_call_output":
		if !s.has("output") {
			return DecodeError{Field: "output", Reason: "missing required field for type function_call_output"}
		}
		if s.isNull("output") {
			return DecodeError{Field: "output", Reason: "cannot be null"}
		}
	case "custom_tool_call":
		if !s.has("call_id") {
			return DecodeError{Field: "call_id", Reason: "missing required field for type custom_tool_call"}
		}
		if s.isNull("call_id") {
			return DecodeError{Field: "call_id", Reason: "cannot be null"}
		}
		if !s.has("input") {
			return DecodeError{Field: "input", Reason: "missing required field for type custom_tool_call"}
		}
		if s.isNull("input") {
			return DecodeError{Field: "input", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type custom_tool_call"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
	case "custom_tool_call_output":
		if !s.has("call_id") {
			return DecodeError{Field: "call_id", Reason: "missing required field for type custom_tool_call_output"}
		}
		if s.isNull("call_id") {
			return DecodeError{Field: "call_id", Reason: "cannot be null"}
		}
		if !s.has("output") {
			return DecodeError{Field: "output", Reason: "missing required field for type custom_tool_call_output"}
		}
		if s.isNull("output") {
			return DecodeError{Field: "output", Reason: "cannot be null"}
		}
	case "tool_search_output":
		if !s.has("execution") {
			return DecodeError{Field: "execution", Reason: "missing required field for type tool_search_output"}
		}
		if s.isNull("execution") {
			return DecodeError{Field: "execution", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type tool_search_output"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
		if !s.has("tools") {
			return DecodeError{Field: "tools", Reason: "missing required field for type tool_search_output"}
		}
		if s.isNull("tools") {
			return DecodeError{Field: "tools", Reason: "cannot be null"}
		}
	case "web_search_call":
	case "image_generation_call":
		if !s.has("result") {
			return DecodeError{Field: "result", Reason: "missing required field for type image_generation_call"}
		}
		if s.isNull("result") {
			return DecodeError{Field: "result", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type image_generation_call"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "compaction":
		if !s.has("encrypted_content") {
			return DecodeError{Field: "encrypted_content", Reason: "missing required field for type compaction"}
		}
		if s.isNull("encrypted_content") {
			return DecodeError{Field: "encrypted_content", Reason: "cannot be null"}
		}
	case "configuration_update":
		if !s.has("reasoning") {
			return DecodeError{Field: "reasoning", Reason: "missing required field for type configuration_update"}
		}
		if s.isNull("reasoning") {
			return DecodeError{Field: "reasoning", Reason: "cannot be null"}
		}
	case "compaction_trigger":
	case "context_compaction":
	case "other":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ResponseUsageMetadata struct {
	Amount   Optional[string] `json:"amount,omitzero"`
	Metadata json.RawMessage  `json:"metadata,omitempty"`
}

var responseUsageMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "amount"},
		{name: "metadata"},
	},
}

func (v ResponseUsageMetadata) MarshalJSON() ([]byte, error) {
	type wire ResponseUsageMetadata
	return json.Marshal(wire(v))
}

func (v *ResponseUsageMetadata) UnmarshalJSON(data []byte) error {
	type wire ResponseUsageMetadata
	return decodeObject(data, &responseUsageMetadataSpec, (*wire)(v))
}

type ResponsesAPIWebSearchAction struct {
	Pattern   Optional[string]   `json:"pattern,omitzero"`
	Queries   Optional[[]string] `json:"queries,omitzero"`
	Query     Optional[string]   `json:"query,omitzero"`
	TypeValue string             `json:"type"`
	URL       Optional[string]   `json:"url,omitzero"`
	RawJSON   json.RawMessage    `json:"-"`
}

var responsesAPIWebSearchActionSpec = objectSpec{
	fields: []objectField{
		{name: "pattern"},
		{name: "queries"},
		{name: "query"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "url"},
	},
	tag:  3,
	tags: []string{"search", "open_page", "find_in_page", "other"},
}

func (v ResponsesAPIWebSearchAction) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "search":
	case "open_page":
	case "find_in_page":
	case "other":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ResponsesAPIWebSearchAction
	return json.Marshal(wire(v))
}

func (v *ResponsesAPIWebSearchAction) UnmarshalJSON(data []byte) error {
	type wire ResponsesAPIWebSearchAction
	var s objectScan
	known, err := s.begin(data, &responsesAPIWebSearchActionSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "search":
	case "open_page":
	case "find_in_page":
	case "other":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ReviewDelivery string

const (
	ReviewDeliveryInline   ReviewDelivery = "inline"
	ReviewDeliveryDetached ReviewDelivery = "detached"
)

type ReviewStartParams struct {
	Delivery Optional[ReviewDelivery] `json:"delivery,omitzero"`
	Target   ReviewTarget             `json:"target"`
	ThreadID string                   `json:"threadId"`
}

var reviewStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "delivery", defaultJSON: "null"},
		{name: "target", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ReviewStartParams) MarshalJSON() ([]byte, error) {
	type wire ReviewStartParams
	return json.Marshal(wire(v))
}

func (v *ReviewStartParams) UnmarshalJSON(data []byte) error {
	type wire ReviewStartParams
	return decodeObject(data, &reviewStartParamsSpec, (*wire)(v))
}

type ReviewStartResponse struct {
	ReviewThreadID string `json:"reviewThreadId"`
	Turn           Turn   `json:"turn"`
}

var reviewStartResponseSpec = objectSpec{
	fields: []objectField{
		{name: "reviewThreadId", flags: fieldRequired | fieldRejectNull},
		{name: "turn", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ReviewStartResponse) MarshalJSON() ([]byte, error) {
	type wire ReviewStartResponse
	return json.Marshal(wire(v))
}

func (v *ReviewStartResponse) UnmarshalJSON(data []byte) error {
	type wire ReviewStartResponse
	return decodeObject(data, &reviewStartResponseSpec, (*wire)(v))
}

type ReviewTarget struct {
	Branch       OptionalNonNull[string] `json:"branch,omitzero"`
	Instructions OptionalNonNull[string] `json:"instructions,omitzero"`
	Sha          OptionalNonNull[string] `json:"sha,omitzero"`
	Title        Optional[string]        `json:"title,omitzero"`
	TypeValue    string                  `json:"type"`
	RawJSON      json.RawMessage         `json:"-"`
}

var reviewTargetSpec = objectSpec{
	fields: []objectField{
		{name: "branch", flags: fieldNonNullOptional},
		{name: "instructions", flags: fieldNonNullOptional},
		{name: "sha", flags: fieldNonNullOptional},
		{name: "title"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  4,
	tags: []string{"uncommittedChanges", "baseBranch", "commit", "custom"},
}

func (v ReviewTarget) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "uncommittedChanges":
	case "baseBranch":
		if !v.Branch.IsSet() {
			return nil, DecodeError{Field: "branch", Reason: "missing required field for type baseBranch"}
		}
	case "commit":
		if !v.Sha.IsSet() {
			return nil, DecodeError{Field: "sha", Reason: "missing required field for type commit"}
		}
	case "custom":
		if !v.Instructions.IsSet() {
			return nil, DecodeError{Field: "instructions", Reason: "missing required field for type custom"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ReviewTarget
	return json.Marshal(wire(v))
}

func (v *ReviewTarget) UnmarshalJSON(data []byte) error {
	type wire ReviewTarget
	var s objectScan
	known, err := s.begin(data, &reviewTargetSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "uncommittedChanges":
	case "baseBranch":
		if !s.has("branch") {
			return DecodeError{Field: "branch", Reason: "missing required field for type baseBranch"}
		}
		if s.isNull("branch") {
			return DecodeError{Field: "branch", Reason: "cannot be null"}
		}
	case "commit":
		if !s.has("sha") {
			return DecodeError{Field: "sha", Reason: "missing required field for type commit"}
		}
		if s.isNull("sha") {
			return DecodeError{Field: "sha", Reason: "cannot be null"}
		}
	case "custom":
		if !s.has("instructions") {
			return DecodeError{Field: "instructions", Reason: "missing required field for type custom"}
		}
		if s.isNull("instructions") {
			return DecodeError{Field: "instructions", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type RolloutCompressResponse map[string]json.RawMessage

func (v RolloutCompressResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *RolloutCompressResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = RolloutCompressResponse(raw)
	return nil
}

type SandboxMode string

const (
	SandboxModeReadOnly         SandboxMode = "read-only"
	SandboxModeWorkspaceWrite   SandboxMode = "workspace-write"
	SandboxModeDangerFullAccess SandboxMode = "danger-full-access"
)

type SandboxPolicy struct {
	ExcludeSlashTmp     OptionalNonNull[bool]              `json:"excludeSlashTmp,omitzero"`
	ExcludeTmpdirEnvVar OptionalNonNull[bool]              `json:"excludeTmpdirEnvVar,omitzero"`
	NetworkAccess       json.RawMessage                    `json:"networkAccess,omitempty"`
	TypeValue           string                             `json:"type"`
	WritableRoots       OptionalNonNull[[]AbsolutePathBuf] `json:"writableRoots,omitzero"`
	RawJSON             json.RawMessage                    `json:"-"`
}

var sandboxPolicySpec = objectSpec{
	fields: []objectField{
		{name: "excludeSlashTmp", flags: fieldNonNullOptional},
		{name: "excludeTmpdirEnvVar", flags: fieldNonNullOptional},
		{name: "networkAccess", flags: fieldRejectNull},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "writableRoots", flags: fieldNonNullOptional},
	},
	tag:  3,
	tags: []string{"dangerFullAccess", "readOnly", "externalSandbox", "workspaceWrite"},
}

func (v SandboxPolicy) MarshalJSON() ([]byte, error) {
	if bytes.Equal(v.NetworkAccess, []byte("null")) {
		return nil, DecodeError{Field: "networkAccess", Reason: "cannot be null"}
	}
	switch v.TypeValue {
	case "dangerFullAccess":
	case "readOnly":
	case "externalSandbox":
	case "workspaceWrite":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire SandboxPolicy
	return json.Marshal(wire(v))
}

func (v *SandboxPolicy) UnmarshalJSON(data []byte) error {
	type wire SandboxPolicy
	var s objectScan
	known, err := s.begin(data, &sandboxPolicySpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "dangerFullAccess":
	case "readOnly":
	case "externalSandbox":
	case "workspaceWrite":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type SandboxWorkspaceWrite struct {
	ExcludeSlashTmp     OptionalNonNull[bool]     `json:"exclude_slash_tmp,omitzero"`
	ExcludeTmpdirEnvVar OptionalNonNull[bool]     `json:"exclude_tmpdir_env_var,omitzero"`
	NetworkAccess       OptionalNonNull[bool]     `json:"network_access,omitzero"`
	WritableRoots       OptionalNonNull[[]string] `json:"writable_roots,omitzero"`
}

var sandboxWorkspaceWriteSpec = objectSpec{
	fields: []objectField{
		{name: "exclude_slash_tmp", flags: fieldNonNullOptional},
		{name: "exclude_tmpdir_env_var", flags: fieldNonNullOptional},
		{name: "network_access", flags: fieldNonNullOptional},
		{name: "writable_roots", flags: fieldNonNullOptional},
	},
}

func (v SandboxWorkspaceWrite) MarshalJSON() ([]byte, error) {
	type wire SandboxWorkspaceWrite
	return json.Marshal(wire(v))
}

func (v *SandboxWorkspaceWrite) UnmarshalJSON(data []byte) error {
	type wire SandboxWorkspaceWrite
	return decodeObject(data, &sandboxWorkspaceWriteSpec, (*wire)(v))
}

type ScheduledTaskSchedule struct {
	Days          Optional[[]ScheduledTaskWeekday] `json:"days,omitzero"`
	IntervalHours OptionalNonNull[uint32]          `json:"intervalHours,omitzero"`
	Time          OptionalNonNull[string]          `json:"time,omitzero"`
	TypeValue     string                           `json:"type"`
	RawJSON       json.RawMessage                  `json:"-"`
}

var scheduledTaskScheduleSpec = objectSpec{
	fields: []objectField{
		{name: "days"},
		{name: "intervalHours", flags: fieldNonNullOptional},
		{name: "time", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  3,
	tags: []string{"hourly", "daily", "weekdays", "weekly"},
}

func (v ScheduledTaskSchedule) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "hourly":
		if !v.IntervalHours.IsSet() {
			return nil, DecodeError{Field: "intervalHours", Reason: "missing required field for type hourly"}
		}
	case "daily":
		if !v.Time.IsSet() {
			return nil, DecodeError{Field: "time", Reason: "missing required field for type daily"}
		}
	case "weekdays":
		if !v.Time.IsSet() {
			return nil, DecodeError{Field: "time", Reason: "missing required field for type weekdays"}
		}
	case "weekly":
		if !v.Days.IsSet() {
			return nil, DecodeError{Field: "days", Reason: "missing required field for type weekly"}
		}
		if v.Days.IsNull() {
			return nil, DecodeError{Field: "days", Reason: "cannot be null"}
		}
		if !v.Time.IsSet() {
			return nil, DecodeError{Field: "time", Reason: "missing required field for type weekly"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ScheduledTaskSchedule
	return json.Marshal(wire(v))
}

func (v *ScheduledTaskSchedule) UnmarshalJSON(data []byte) error {
	type wire ScheduledTaskSchedule
	var s objectScan
	known, err := s.begin(data, &scheduledTaskScheduleSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueIntervalHours, ok := v.IntervalHours.Value(); ok {
		if valueIntervalHours < 0 {
			return DecodeError{Field: "intervalHours", Reason: "below minimum 0"}
		}
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "hourly":
		if !s.has("intervalHours") {
			return DecodeError{Field: "intervalHours", Reason: "missing required field for type hourly"}
		}
		if s.isNull("intervalHours") {
			return DecodeError{Field: "intervalHours", Reason: "cannot be null"}
		}
	case "daily":
		if !s.has("time") {
			return DecodeError{Field: "time", Reason: "missing required field for type daily"}
		}
		if s.isNull("time") {
			return DecodeError{Field: "time", Reason: "cannot be null"}
		}
	case "weekdays":
		if !s.has("time") {
			return DecodeError{Field: "time", Reason: "missing required field for type weekdays"}
		}
		if s.isNull("time") {
			return DecodeError{Field: "time", Reason: "cannot be null"}
		}
	case "weekly":
		if !s.has("days") {
			return DecodeError{Field: "days", Reason: "missing required field for type weekly"}
		}
		if s.isNull("days") {
			return DecodeError{Field: "days", Reason: "cannot be null"}
		}
		if !s.has("time") {
			return DecodeError{Field: "time", Reason: "missing required field for type weekly"}
		}
		if s.isNull("time") {
			return DecodeError{Field: "time", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ScheduledTaskSummary struct {
	Key      string                `json:"key"`
	Name     string                `json:"name"`
	Prompt   string                `json:"prompt"`
	Schedule ScheduledTaskSchedule `json:"schedule"`
}

var scheduledTaskSummarySpec = objectSpec{
	fields: []objectField{
		{name: "key", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "prompt", flags: fieldRequired | fieldRejectNull},
		{name: "schedule", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ScheduledTaskSummary) MarshalJSON() ([]byte, error) {
	type wire ScheduledTaskSummary
	return json.Marshal(wire(v))
}

func (v *ScheduledTaskSummary) UnmarshalJSON(data []byte) error {
	type wire ScheduledTaskSummary
	return decodeObject(data, &scheduledTaskSummarySpec, (*wire)(v))
}

type ScheduledTaskWeekday string

const (
	ScheduledTaskWeekdayMo ScheduledTaskWeekday = "MO"
	ScheduledTaskWeekdayTu ScheduledTaskWeekday = "TU"
	ScheduledTaskWeekdayWe ScheduledTaskWeekday = "WE"
	ScheduledTaskWeekdayTh ScheduledTaskWeekday = "TH"
	ScheduledTaskWeekdayFr ScheduledTaskWeekday = "FR"
	ScheduledTaskWeekdaySa ScheduledTaskWeekday = "SA"
	ScheduledTaskWeekdaySu ScheduledTaskWeekday = "SU"
)

type SelectedCapabilityRoot struct {
	ID       string                 `json:"id"`
	Location CapabilityRootLocation `json:"location"`
}

var selectedCapabilityRootSpec = objectSpec{
	fields: []objectField{
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "location", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SelectedCapabilityRoot) MarshalJSON() ([]byte, error) {
	type wire SelectedCapabilityRoot
	return json.Marshal(wire(v))
}

func (v *SelectedCapabilityRoot) UnmarshalJSON(data []byte) error {
	type wire SelectedCapabilityRoot
	return decodeObject(data, &selectedCapabilityRootSpec, (*wire)(v))
}

type SendAddCreditsNudgeEmailParams struct {
	CreditType AddCreditsNudgeCreditType `json:"creditType"`
}

var sendAddCreditsNudgeEmailParamsSpec = objectSpec{
	fields: []objectField{
		{name: "creditType", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SendAddCreditsNudgeEmailParams) MarshalJSON() ([]byte, error) {
	type wire SendAddCreditsNudgeEmailParams
	return json.Marshal(wire(v))
}

func (v *SendAddCreditsNudgeEmailParams) UnmarshalJSON(data []byte) error {
	type wire SendAddCreditsNudgeEmailParams
	return decodeObject(data, &sendAddCreditsNudgeEmailParamsSpec, (*wire)(v))
}

type SendAddCreditsNudgeEmailResponse struct {
	Status AddCreditsNudgeEmailStatus `json:"status"`
}

var sendAddCreditsNudgeEmailResponseSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SendAddCreditsNudgeEmailResponse) MarshalJSON() ([]byte, error) {
	type wire SendAddCreditsNudgeEmailResponse
	return json.Marshal(wire(v))
}

func (v *SendAddCreditsNudgeEmailResponse) UnmarshalJSON(data []byte) error {
	type wire SendAddCreditsNudgeEmailResponse
	return decodeObject(data, &sendAddCreditsNudgeEmailResponseSpec, (*wire)(v))
}

type ServerDiagnosticsGauge struct {
	Name  string `json:"name"`
	Value uint64 `json:"value"`
}

var serverDiagnosticsGaugeSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "value", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ServerDiagnosticsGauge) MarshalJSON() ([]byte, error) {
	type wire ServerDiagnosticsGauge
	return json.Marshal(wire(v))
}

func (v *ServerDiagnosticsGauge) UnmarshalJSON(data []byte) error {
	type wire ServerDiagnosticsGauge
	var s objectScan
	if _, err := s.begin(data, &serverDiagnosticsGaugeSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Value < 0 {
		return DecodeError{Field: "value", Reason: "below minimum 0"}
	}
	return nil
}

type ServerDiagnosticsParams map[string]json.RawMessage

func (v ServerDiagnosticsParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ServerDiagnosticsParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ServerDiagnosticsParams(raw)
	return nil
}

type ServerDiagnosticsProcess struct {
	ID                     uint32           `json:"id"`
	PhysicalFootprintBytes Optional[uint64] `json:"physicalFootprintBytes,omitzero"`
	ResidentMemoryBytes    Optional[uint64] `json:"residentMemoryBytes,omitzero"`
}

var serverDiagnosticsProcessSpec = objectSpec{
	fields: []objectField{
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "physicalFootprintBytes"},
		{name: "residentMemoryBytes"},
	},
}

func (v ServerDiagnosticsProcess) MarshalJSON() ([]byte, error) {
	type wire ServerDiagnosticsProcess
	return json.Marshal(wire(v))
}

func (v *ServerDiagnosticsProcess) UnmarshalJSON(data []byte) error {
	type wire ServerDiagnosticsProcess
	var s objectScan
	if _, err := s.begin(data, &serverDiagnosticsProcessSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.ID < 0 {
		return DecodeError{Field: "id", Reason: "below minimum 0"}
	}
	if valuePhysicalFootprintBytes, ok := v.PhysicalFootprintBytes.Value(); ok {
		if valuePhysicalFootprintBytes < 0 {
			return DecodeError{Field: "physicalFootprintBytes", Reason: "below minimum 0"}
		}
	}
	if valueResidentMemoryBytes, ok := v.ResidentMemoryBytes.Value(); ok {
		if valueResidentMemoryBytes < 0 {
			return DecodeError{Field: "residentMemoryBytes", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ServerDiagnosticsResponse struct {
	Gauges  []ServerDiagnosticsGauge `json:"gauges"`
	Process ServerDiagnosticsProcess `json:"process"`
}

var serverDiagnosticsResponseSpec = objectSpec{
	fields: []objectField{
		{name: "gauges", flags: fieldRequired | fieldRejectNull},
		{name: "process", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ServerDiagnosticsResponse) MarshalJSON() ([]byte, error) {
	type wire ServerDiagnosticsResponse
	return json.Marshal(wire(v))
}

func (v *ServerDiagnosticsResponse) UnmarshalJSON(data []byte) error {
	type wire ServerDiagnosticsResponse
	return decodeObject(data, &serverDiagnosticsResponseSpec, (*wire)(v))
}

type ServerRequestResolvedNotification struct {
	RequestID RequestID `json:"requestId"`
	ThreadID  string    `json:"threadId"`
}

var serverRequestResolvedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "requestId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ServerRequestResolvedNotification) MarshalJSON() ([]byte, error) {
	type wire ServerRequestResolvedNotification
	return json.Marshal(wire(v))
}

func (v *ServerRequestResolvedNotification) UnmarshalJSON(data []byte) error {
	type wire ServerRequestResolvedNotification
	return decodeObject(data, &serverRequestResolvedNotificationSpec, (*wire)(v))
}

type SessionMigration struct {
	Cwd   string           `json:"cwd"`
	Path  string           `json:"path"`
	Title Optional[string] `json:"title,omitzero"`
}

var sessionMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "title"},
	},
}

func (v SessionMigration) MarshalJSON() ([]byte, error) {
	type wire SessionMigration
	return json.Marshal(wire(v))
}

func (v *SessionMigration) UnmarshalJSON(data []byte) error {
	type wire SessionMigration
	return decodeObject(data, &sessionMigrationSpec, (*wire)(v))
}

type SessionSource struct {
	StringValue string                          `json:"-"`
	Custom      OptionalNonNull[string]         `json:"custom,omitzero"`
	SubAgent    OptionalNonNull[SubAgentSource] `json:"subAgent,omitzero"`
	RawJSON     json.RawMessage                 `json:"-"`
}

var (
	SessionSourceCli       = SessionSource{StringValue: "cli"}
	SessionSourceVscode    = SessionSource{StringValue: "vscode"}
	SessionSourceExec      = SessionSource{StringValue: "exec"}
	SessionSourceAppServer = SessionSource{StringValue: "appServer"}
	SessionSourceUnknown   = SessionSource{StringValue: "unknown"}
)

func (v SessionSource) IsSet() bool {
	return v.StringValue != "" || len(bytes.TrimSpace(v.RawJSON)) > 0 || v.Custom.IsSet() || v.SubAgent.IsSet()
}

var sessionSourceSpec = objectSpec{
	fields: []objectField{
		{name: "custom", flags: fieldNonNullOptional},
		{name: "subAgent", flags: fieldNonNullOptional},
	},
	fieldOffset: 1,
}

func (v SessionSource) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.Custom.IsSet() {
		out["custom"] = v.Custom
	}
	if v.SubAgent.IsSet() {
		out["subAgent"] = v.SubAgent
	}
	if v.StringValue != "" {
		if len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 {
			return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
		}
		switch v.StringValue {
		case "cli", "vscode", "exec", "appServer", "unknown":
			return json.Marshal(v.StringValue)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported SessionSource string value %q", v.StringValue)}
		}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	if _, ok := out["custom"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	if _, ok := out["subAgent"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid SessionSource raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *SessionSource) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var stringValue string
	if err := json.Unmarshal(trimmed, &stringValue); err == nil {
		switch stringValue {
		case "cli", "vscode", "exec", "appServer", "unknown":
			*v = SessionSource{StringValue: stringValue}
		default:
			*v = SessionSource{RawJSON: append(json.RawMessage(nil), data...)}
		}
		return nil
	}
	v.StringValue = ""
	type wire SessionSource
	var s objectScan
	if _, err := s.begin(data, &sessionSourceSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.nonNull("custom") {
		untaggedOneOfMatches++
	}
	if s.nonNull("subAgent") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type Settings struct {
	DeveloperInstructions Optional[string]          `json:"developer_instructions,omitzero"`
	Model                 string                    `json:"model"`
	ReasoningEffort       Optional[ReasoningEffort] `json:"reasoning_effort,omitzero"`
}

var settingsSpec = objectSpec{
	fields: []objectField{
		{name: "developer_instructions"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "reasoning_effort"},
	},
}

func (v Settings) MarshalJSON() ([]byte, error) {
	type wire Settings
	return json.Marshal(wire(v))
}

func (v *Settings) UnmarshalJSON(data []byte) error {
	type wire Settings
	return decodeObject(data, &settingsSpec, (*wire)(v))
}

type SkillDependencies struct {
	Tools []SkillToolDependency `json:"tools"`
}

var skillDependenciesSpec = objectSpec{
	fields: []objectField{
		{name: "tools", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillDependencies) MarshalJSON() ([]byte, error) {
	type wire SkillDependencies
	return json.Marshal(wire(v))
}

func (v *SkillDependencies) UnmarshalJSON(data []byte) error {
	type wire SkillDependencies
	return decodeObject(data, &skillDependenciesSpec, (*wire)(v))
}

type SkillErrorInfo struct {
	Message string `json:"message"`
	Path    string `json:"path"`
}

var skillErrorInfoSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillErrorInfo) MarshalJSON() ([]byte, error) {
	type wire SkillErrorInfo
	return json.Marshal(wire(v))
}

func (v *SkillErrorInfo) UnmarshalJSON(data []byte) error {
	type wire SkillErrorInfo
	return decodeObject(data, &skillErrorInfoSpec, (*wire)(v))
}

type SkillInterface struct {
	BrandColor       Optional[string]          `json:"brandColor,omitzero"`
	DefaultPrompt    Optional[string]          `json:"defaultPrompt,omitzero"`
	DisplayName      Optional[string]          `json:"displayName,omitzero"`
	IconLarge        Optional[AbsolutePathBuf] `json:"iconLarge,omitzero"`
	IconLargeURL     Optional[string]          `json:"iconLargeUrl,omitzero"`
	IconSmall        Optional[AbsolutePathBuf] `json:"iconSmall,omitzero"`
	IconSmallURL     Optional[string]          `json:"iconSmallUrl,omitzero"`
	ShortDescription Optional[string]          `json:"shortDescription,omitzero"`
}

var skillInterfaceSpec = objectSpec{
	fields: []objectField{
		{name: "brandColor"},
		{name: "defaultPrompt"},
		{name: "displayName"},
		{name: "iconLarge"},
		{name: "iconLargeUrl"},
		{name: "iconSmall"},
		{name: "iconSmallUrl"},
		{name: "shortDescription"},
	},
}

func (v SkillInterface) MarshalJSON() ([]byte, error) {
	type wire SkillInterface
	return json.Marshal(wire(v))
}

func (v *SkillInterface) UnmarshalJSON(data []byte) error {
	type wire SkillInterface
	return decodeObject(data, &skillInterfaceSpec, (*wire)(v))
}

type SkillMetadata struct {
	Dependencies     Optional[SkillDependencies] `json:"dependencies,omitzero"`
	Description      string                      `json:"description"`
	Enabled          bool                        `json:"enabled"`
	InterfaceValue   Optional[SkillInterface]    `json:"interface,omitzero"`
	Name             string                      `json:"name"`
	Path             LegacyAppPathString         `json:"path"`
	PluginID         Optional[string]            `json:"pluginId,omitzero"`
	Scope            SkillScope                  `json:"scope"`
	ShortDescription Optional[string]            `json:"shortDescription,omitzero"`
}

var skillMetadataSpec = objectSpec{
	fields: []objectField{
		{name: "dependencies"},
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "interface"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "pluginId"},
		{name: "scope", flags: fieldRequired | fieldRejectNull},
		{name: "shortDescription"},
	},
}

func (v SkillMetadata) MarshalJSON() ([]byte, error) {
	type wire SkillMetadata
	w := wire(v)
	w.Dependencies = w.Dependencies.withoutNull()
	w.InterfaceValue = w.InterfaceValue.withoutNull()
	w.ShortDescription = w.ShortDescription.withoutNull()
	return json.Marshal(w)
}

func (v *SkillMetadata) UnmarshalJSON(data []byte) error {
	type wire SkillMetadata
	return decodeObject(data, &skillMetadataSpec, (*wire)(v))
}

type SkillMigration struct {
	Name string `json:"name"`
}

var skillMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillMigration) MarshalJSON() ([]byte, error) {
	type wire SkillMigration
	return json.Marshal(wire(v))
}

func (v *SkillMigration) UnmarshalJSON(data []byte) error {
	type wire SkillMigration
	return decodeObject(data, &skillMigrationSpec, (*wire)(v))
}

type SkillScope string

const (
	SkillScopeUser   SkillScope = "user"
	SkillScopeRepo   SkillScope = "repo"
	SkillScopeSystem SkillScope = "system"
	SkillScopeAdmin  SkillScope = "admin"
)

type SkillSummary struct {
	Description      string                        `json:"description"`
	Enabled          bool                          `json:"enabled"`
	InterfaceValue   Optional[SkillInterface]      `json:"interface,omitzero"`
	Name             string                        `json:"name"`
	Path             Optional[LegacyAppPathString] `json:"path,omitzero"`
	ShortDescription Optional[string]              `json:"shortDescription,omitzero"`
}

var skillSummarySpec = objectSpec{
	fields: []objectField{
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "interface"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "path"},
		{name: "shortDescription"},
	},
}

func (v SkillSummary) MarshalJSON() ([]byte, error) {
	type wire SkillSummary
	return json.Marshal(wire(v))
}

func (v *SkillSummary) UnmarshalJSON(data []byte) error {
	type wire SkillSummary
	return decodeObject(data, &skillSummarySpec, (*wire)(v))
}

type SkillToolDependency struct {
	Command     Optional[string] `json:"command,omitzero"`
	Description Optional[string] `json:"description,omitzero"`
	Transport   Optional[string] `json:"transport,omitzero"`
	TypeValue   string           `json:"type"`
	URL         Optional[string] `json:"url,omitzero"`
	Value       string           `json:"value"`
}

var skillToolDependencySpec = objectSpec{
	fields: []objectField{
		{name: "command"},
		{name: "description"},
		{name: "transport"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "url"},
		{name: "value", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillToolDependency) MarshalJSON() ([]byte, error) {
	type wire SkillToolDependency
	w := wire(v)
	w.Command = w.Command.withoutNull()
	w.Description = w.Description.withoutNull()
	w.Transport = w.Transport.withoutNull()
	return json.Marshal(w)
}

func (v *SkillToolDependency) UnmarshalJSON(data []byte) error {
	type wire SkillToolDependency
	return decodeObject(data, &skillToolDependencySpec, (*wire)(v))
}

type SkillsChangedNotification map[string]json.RawMessage

func (v SkillsChangedNotification) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *SkillsChangedNotification) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = SkillsChangedNotification(raw)
	return nil
}

type SkillsConfigWriteParams struct {
	Enabled bool                      `json:"enabled"`
	Name    Optional[string]          `json:"name,omitzero"`
	Path    Optional[AbsolutePathBuf] `json:"path,omitzero"`
}

var skillsConfigWriteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "enabled", flags: fieldRequired | fieldRejectNull},
		{name: "name"},
		{name: "path"},
	},
}

func (v SkillsConfigWriteParams) MarshalJSON() ([]byte, error) {
	type wire SkillsConfigWriteParams
	return json.Marshal(wire(v))
}

func (v *SkillsConfigWriteParams) UnmarshalJSON(data []byte) error {
	type wire SkillsConfigWriteParams
	return decodeObject(data, &skillsConfigWriteParamsSpec, (*wire)(v))
}

type SkillsConfigWriteResponse struct {
	EffectiveEnabled bool `json:"effectiveEnabled"`
}

var skillsConfigWriteResponseSpec = objectSpec{
	fields: []objectField{
		{name: "effectiveEnabled", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillsConfigWriteResponse) MarshalJSON() ([]byte, error) {
	type wire SkillsConfigWriteResponse
	return json.Marshal(wire(v))
}

func (v *SkillsConfigWriteResponse) UnmarshalJSON(data []byte) error {
	type wire SkillsConfigWriteResponse
	return decodeObject(data, &skillsConfigWriteResponseSpec, (*wire)(v))
}

type SkillsExtraRootsSetParams struct {
	ExtraRoots []AbsolutePathBuf `json:"extraRoots"`
}

var skillsExtraRootsSetParamsSpec = objectSpec{
	fields: []objectField{
		{name: "extraRoots", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillsExtraRootsSetParams) MarshalJSON() ([]byte, error) {
	type wire SkillsExtraRootsSetParams
	return json.Marshal(wire(v))
}

func (v *SkillsExtraRootsSetParams) UnmarshalJSON(data []byte) error {
	type wire SkillsExtraRootsSetParams
	return decodeObject(data, &skillsExtraRootsSetParamsSpec, (*wire)(v))
}

type SkillsExtraRootsSetResponse map[string]json.RawMessage

func (v SkillsExtraRootsSetResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *SkillsExtraRootsSetResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = SkillsExtraRootsSetResponse(raw)
	return nil
}

type SkillsListEntry struct {
	Cwd    string           `json:"cwd"`
	Errors []SkillErrorInfo `json:"errors"`
	Skills []SkillMetadata  `json:"skills"`
}

var skillsListEntrySpec = objectSpec{
	fields: []objectField{
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "errors", flags: fieldRequired | fieldRejectNull},
		{name: "skills", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillsListEntry) MarshalJSON() ([]byte, error) {
	type wire SkillsListEntry
	return json.Marshal(wire(v))
}

func (v *SkillsListEntry) UnmarshalJSON(data []byte) error {
	type wire SkillsListEntry
	return decodeObject(data, &skillsListEntrySpec, (*wire)(v))
}

type SkillsListParams struct {
	Cwds        OptionalNonNull[[]string] `json:"cwds,omitzero"`
	ForceReload OptionalNonNull[bool]     `json:"forceReload,omitzero"`
}

var skillsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "forceReload", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v SkillsListParams) MarshalJSON() ([]byte, error) {
	type wire SkillsListParams
	w := wire(v)
	w.Cwds = nonNullWithoutEmpty(w.Cwds)
	w.ForceReload = nonNullWithoutFalse(w.ForceReload)
	return json.Marshal(w)
}

func (v *SkillsListParams) UnmarshalJSON(data []byte) error {
	type wire SkillsListParams
	return decodeObject(data, &skillsListParamsSpec, (*wire)(v))
}

type SkillsListResponse struct {
	Data []SkillsListEntry `json:"data"`
}

var skillsListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SkillsListResponse) MarshalJSON() ([]byte, error) {
	type wire SkillsListResponse
	return json.Marshal(wire(v))
}

func (v *SkillsListResponse) UnmarshalJSON(data []byte) error {
	type wire SkillsListResponse
	return decodeObject(data, &skillsListResponseSpec, (*wire)(v))
}

type SortDirection string

const (
	SortDirectionAsc  SortDirection = "asc"
	SortDirectionDesc SortDirection = "desc"
)

type SpendControlLimitSnapshot struct {
	Limit            string `json:"limit"`
	RemainingPercent int32  `json:"remainingPercent"`
	ResetsAt         int64  `json:"resetsAt"`
	Used             string `json:"used"`
}

var spendControlLimitSnapshotSpec = objectSpec{
	fields: []objectField{
		{name: "limit", flags: fieldRequired | fieldRejectNull},
		{name: "remainingPercent", flags: fieldRequired | fieldRejectNull},
		{name: "resetsAt", flags: fieldRequired | fieldRejectNull},
		{name: "used", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SpendControlLimitSnapshot) MarshalJSON() ([]byte, error) {
	type wire SpendControlLimitSnapshot
	return json.Marshal(wire(v))
}

func (v *SpendControlLimitSnapshot) UnmarshalJSON(data []byte) error {
	type wire SpendControlLimitSnapshot
	return decodeObject(data, &spendControlLimitSnapshotSpec, (*wire)(v))
}

type StrictReviewRequiredNotification struct {
	StartedAtMs int64  `json:"startedAtMs"`
	ThreadID    string `json:"threadId"`
	TurnID      string `json:"turnId"`
}

var strictReviewRequiredNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v StrictReviewRequiredNotification) MarshalJSON() ([]byte, error) {
	type wire StrictReviewRequiredNotification
	return json.Marshal(wire(v))
}

func (v *StrictReviewRequiredNotification) UnmarshalJSON(data []byte) error {
	type wire StrictReviewRequiredNotification
	return decodeObject(data, &strictReviewRequiredNotificationSpec, (*wire)(v))
}

type SubAgentActivityKind string

const (
	SubAgentActivityKindStarted     SubAgentActivityKind = "started"
	SubAgentActivityKindInteracted  SubAgentActivityKind = "interacted"
	SubAgentActivityKindInterrupted SubAgentActivityKind = "interrupted"
	SubAgentActivityKindCompleted   SubAgentActivityKind = "completed"
)

type SubAgentSource struct {
	StringValue string                                      `json:"-"`
	Other       OptionalNonNull[string]                     `json:"other,omitzero"`
	ThreadSpawn OptionalNonNull[map[string]json.RawMessage] `json:"thread_spawn,omitzero"`
	RawJSON     json.RawMessage                             `json:"-"`
}

var (
	SubAgentSourceReview              = SubAgentSource{StringValue: "review"}
	SubAgentSourceCompact             = SubAgentSource{StringValue: "compact"}
	SubAgentSourceMemoryConsolidation = SubAgentSource{StringValue: "memory_consolidation"}
)

func (v SubAgentSource) IsSet() bool {
	return v.StringValue != "" || len(bytes.TrimSpace(v.RawJSON)) > 0 || v.Other.IsSet() || v.ThreadSpawn.IsSet()
}

var subAgentSourceSpec = objectSpec{
	fields: []objectField{
		{name: "other", flags: fieldNonNullOptional},
		{name: "thread_spawn", flags: fieldNonNullOptional},
	},
	fieldOffset: 1,
}

func (v SubAgentSource) MarshalJSON() ([]byte, error) {
	out := map[string]any{}
	if v.Other.IsSet() {
		out["other"] = v.Other
	}
	if v.ThreadSpawn.IsSet() {
		out["thread_spawn"] = v.ThreadSpawn
	}
	if v.StringValue != "" {
		if len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 {
			return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
		}
		switch v.StringValue {
		case "review", "compact", "memory_consolidation":
			return json.Marshal(v.StringValue)
		default:
			return nil, DecodeError{Field: "", Reason: fmt.Sprintf("unsupported SubAgentSource string value %q", v.StringValue)}
		}
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	untaggedOneOfVariant0Matches := true
	if _, ok := out["thread_spawn"]; !ok {
		untaggedOneOfVariant0Matches = false
	}
	if untaggedOneOfVariant0Matches {
		untaggedOneOfMatches++
	}
	untaggedOneOfVariant1Matches := true
	if _, ok := out["other"]; !ok {
		untaggedOneOfVariant1Matches = false
	}
	if untaggedOneOfVariant1Matches {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {
			if !json.Valid(v.RawJSON) {
				return nil, fmt.Errorf("invalid SubAgentSource raw fallback")
			}
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return nil, DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	return json.Marshal(out)
}

func (v *SubAgentSource) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var stringValue string
	if err := json.Unmarshal(trimmed, &stringValue); err == nil {
		switch stringValue {
		case "review", "compact", "memory_consolidation":
			*v = SubAgentSource{StringValue: stringValue}
		default:
			*v = SubAgentSource{RawJSON: append(json.RawMessage(nil), data...)}
		}
		return nil
	}
	v.StringValue = ""
	type wire SubAgentSource
	var s objectScan
	if _, err := s.begin(data, &subAgentSourceSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	untaggedOneOfMatches := 0
	untaggedOneOfKnownVariantMatches := false
	if s.nonNull("thread_spawn") {
		untaggedOneOfMatches++
	}
	if s.nonNull("other") {
		untaggedOneOfMatches++
	}
	if untaggedOneOfMatches == 0 {
		if !untaggedOneOfKnownVariantMatches {
			v.RawJSON = append(v.RawJSON[:0], data...)
			return nil
		}
		return DecodeError{Field: "", Reason: "does not match any oneOf variant"}
	}
	if untaggedOneOfMatches > 1 {
		return DecodeError{Field: "", Reason: "matches multiple oneOf variants"}
	}
	v.RawJSON = nil
	return nil
}

type SubagentMigration struct {
	Name string `json:"name"`
}

var subagentMigrationSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v SubagentMigration) MarshalJSON() ([]byte, error) {
	type wire SubagentMigration
	return json.Marshal(wire(v))
}

func (v *SubagentMigration) UnmarshalJSON(data []byte) error {
	type wire SubagentMigration
	return decodeObject(data, &subagentMigrationSpec, (*wire)(v))
}

type TerminalInteractionNotification struct {
	ItemID    string `json:"itemId"`
	ProcessID string `json:"processId"`
	Stdin     string `json:"stdin"`
	ThreadID  string `json:"threadId"`
	TurnID    string `json:"turnId"`
}

var terminalInteractionNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "processId", flags: fieldRequired | fieldRejectNull},
		{name: "stdin", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TerminalInteractionNotification) MarshalJSON() ([]byte, error) {
	type wire TerminalInteractionNotification
	return json.Marshal(wire(v))
}

func (v *TerminalInteractionNotification) UnmarshalJSON(data []byte) error {
	type wire TerminalInteractionNotification
	return decodeObject(data, &terminalInteractionNotificationSpec, (*wire)(v))
}

type TextElement struct {
	ByteRange   ByteRange        `json:"byteRange"`
	Placeholder Optional[string] `json:"placeholder,omitzero"`
}

var textElementSpec = objectSpec{
	fields: []objectField{
		{name: "byteRange", flags: fieldRequired | fieldRejectNull},
		{name: "placeholder"},
	},
}

func (v TextElement) MarshalJSON() ([]byte, error) {
	type wire TextElement
	return json.Marshal(wire(v))
}

func (v *TextElement) UnmarshalJSON(data []byte) error {
	type wire TextElement
	return decodeObject(data, &textElementSpec, (*wire)(v))
}

type TextPosition struct {
	Column uint64 `json:"column"`
	Line   uint64 `json:"line"`
}

var textPositionSpec = objectSpec{
	fields: []objectField{
		{name: "column", flags: fieldRequired | fieldRejectNull},
		{name: "line", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TextPosition) MarshalJSON() ([]byte, error) {
	type wire TextPosition
	return json.Marshal(wire(v))
}

func (v *TextPosition) UnmarshalJSON(data []byte) error {
	type wire TextPosition
	var s objectScan
	if _, err := s.begin(data, &textPositionSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Column < 0 {
		return DecodeError{Field: "column", Reason: "below minimum 0"}
	}
	if v.Line < 0 {
		return DecodeError{Field: "line", Reason: "below minimum 0"}
	}
	return nil
}

type TextRange struct {
	End   TextPosition `json:"end"`
	Start TextPosition `json:"start"`
}

var textRangeSpec = objectSpec{
	fields: []objectField{
		{name: "end", flags: fieldRequired | fieldRejectNull},
		{name: "start", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TextRange) MarshalJSON() ([]byte, error) {
	type wire TextRange
	return json.Marshal(wire(v))
}

func (v *TextRange) UnmarshalJSON(data []byte) error {
	type wire TextRange
	return decodeObject(data, &textRangeSpec, (*wire)(v))
}

type Thread struct {
	AgentNickname        Optional[string]                   `json:"agentNickname,omitzero"`
	AgentRole            Optional[string]                   `json:"agentRole,omitzero"`
	CanAcceptDirectInput Optional[bool]                     `json:"canAcceptDirectInput,omitzero"`
	CliVersion           string                             `json:"cliVersion"`
	CreatedAt            int64                              `json:"createdAt"`
	Cwd                  AbsolutePathBuf                    `json:"cwd"`
	DaybreakEnabled      Optional[bool]                     `json:"daybreakEnabled,omitzero"`
	Environments         Optional[[]ThreadEnvironment]      `json:"environments,omitzero"`
	Ephemeral            bool                               `json:"ephemeral"`
	Extra                Optional[ThreadExtra]              `json:"extra,omitzero"`
	ForkedFromID         Optional[string]                   `json:"forkedFromId,omitzero"`
	GitInfo              Optional[GitInfo]                  `json:"gitInfo,omitzero"`
	HistoryMode          OptionalNonNull[ThreadHistoryMode] `json:"historyMode,omitzero"`
	ID                   string                             `json:"id"`
	Model                Optional[string]                   `json:"model,omitzero"`
	ModelProvider        string                             `json:"modelProvider"`
	Name                 Optional[string]                   `json:"name,omitzero"`
	Originator           Optional[string]                   `json:"originator,omitzero"`
	ParentThreadID       Optional[string]                   `json:"parentThreadId,omitzero"`
	Path                 Optional[string]                   `json:"path,omitzero"`
	Preview              string                             `json:"preview"`
	ProjectID            Optional[string]                   `json:"projectId,omitzero"`
	ReasoningEffort      Optional[ReasoningEffort]          `json:"reasoningEffort,omitzero"`
	RecencyAt            Optional[int64]                    `json:"recencyAt,omitzero"`
	Section              Optional[ThreadSection]            `json:"section,omitzero"`
	SectionEnteredAt     Optional[int64]                    `json:"sectionEnteredAt,omitzero"`
	SessionID            string                             `json:"sessionId"`
	Source               SessionSource                      `json:"source"`
	Status               ThreadStatus                       `json:"status"`
	ThreadSource         Optional[ThreadSource]             `json:"threadSource,omitzero"`
	Turns                []Turn                             `json:"turns"`
	UpdatedAt            int64                              `json:"updatedAt"`
}

var threadSpec = objectSpec{
	fields: []objectField{
		{name: "agentNickname"},
		{name: "agentRole"},
		{name: "canAcceptDirectInput"},
		{name: "cliVersion", flags: fieldRequired | fieldRejectNull},
		{name: "createdAt", flags: fieldRequired | fieldRejectNull},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "daybreakEnabled"},
		{name: "environments"},
		{name: "ephemeral", flags: fieldRequired | fieldRejectNull},
		{name: "extra"},
		{name: "forkedFromId"},
		{name: "gitInfo"},
		{name: "historyMode", flags: fieldNonNullOptional, defaultJSON: "\"threaded\""},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "model"},
		{name: "modelProvider", flags: fieldRequired | fieldRejectNull},
		{name: "name"},
		{name: "originator"},
		{name: "parentThreadId"},
		{name: "path"},
		{name: "preview", flags: fieldRequired | fieldRejectNull},
		{name: "projectId", flags: fieldRequired},
		{name: "reasoningEffort"},
		{name: "recencyAt"},
		{name: "section"},
		{name: "sectionEnteredAt"},
		{name: "sessionId", flags: fieldRequired | fieldRejectNull},
		{name: "source", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "threadSource"},
		{name: "turns", flags: fieldRequired | fieldRejectNull},
		{name: "updatedAt", flags: fieldRequired | fieldRejectNull},
	},
}

func (v Thread) MarshalJSON() ([]byte, error) {
	type wire Thread
	return json.Marshal(wire(v))
}

func (v *Thread) UnmarshalJSON(data []byte) error {
	type wire Thread
	return decodeObject(data, &threadSpec, (*wire)(v))
}

type ThreadActiveFlag string

const (
	ThreadActiveFlagWaitingOnApproval  ThreadActiveFlag = "waitingOnApproval"
	ThreadActiveFlagWaitingOnUserInput ThreadActiveFlag = "waitingOnUserInput"
)

type ThreadAgentWorktree struct {
	Branch Optional[string]          `json:"branch,omitzero"`
	Path   AbsolutePathBuf           `json:"path"`
	Status ThreadAgentWorktreeStatus `json:"status"`
}

var threadAgentWorktreeSpec = objectSpec{
	fields: []objectField{
		{name: "branch"},
		{name: "path", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAgentWorktree) MarshalJSON() ([]byte, error) {
	type wire ThreadAgentWorktree
	return json.Marshal(wire(v))
}

func (v *ThreadAgentWorktree) UnmarshalJSON(data []byte) error {
	type wire ThreadAgentWorktree
	return decodeObject(data, &threadAgentWorktreeSpec, (*wire)(v))
}

type ThreadAgentWorktreeStatus string

const (
	ThreadAgentWorktreeStatusChanged ThreadAgentWorktreeStatus = "changed"
	ThreadAgentWorktreeStatusClean   ThreadAgentWorktreeStatus = "clean"
	ThreadAgentWorktreeStatusRemoved ThreadAgentWorktreeStatus = "removed"
)

type ThreadApproveGuardianDeniedActionParams struct {
	Event    json.RawMessage `json:"event"`
	ThreadID string          `json:"threadId"`
}

var threadApproveGuardianDeniedActionParamsSpec = objectSpec{
	fields: []objectField{
		{name: "event", flags: fieldRequired},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadApproveGuardianDeniedActionParams) MarshalJSON() ([]byte, error) {
	type wire ThreadApproveGuardianDeniedActionParams
	return json.Marshal(wire(v))
}

func (v *ThreadApproveGuardianDeniedActionParams) UnmarshalJSON(data []byte) error {
	type wire ThreadApproveGuardianDeniedActionParams
	return decodeObject(data, &threadApproveGuardianDeniedActionParamsSpec, (*wire)(v))
}

type ThreadApproveGuardianDeniedActionResponse map[string]json.RawMessage

func (v ThreadApproveGuardianDeniedActionResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadApproveGuardianDeniedActionResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadApproveGuardianDeniedActionResponse(raw)
	return nil
}

type ThreadArchiveParams struct {
	ThreadID string `json:"threadId"`
}

var threadArchiveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadArchiveParams) MarshalJSON() ([]byte, error) {
	type wire ThreadArchiveParams
	return json.Marshal(wire(v))
}

func (v *ThreadArchiveParams) UnmarshalJSON(data []byte) error {
	type wire ThreadArchiveParams
	return decodeObject(data, &threadArchiveParamsSpec, (*wire)(v))
}

type ThreadArchiveResponse map[string]json.RawMessage

func (v ThreadArchiveResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadArchiveResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadArchiveResponse(raw)
	return nil
}

type ThreadArchivedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadArchivedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadArchivedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadArchivedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadArchivedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadArchivedNotification
	return decodeObject(data, &threadArchivedNotificationSpec, (*wire)(v))
}

type ThreadAttachment struct {
	AttachmentType string          `json:"attachmentType"`
	CreatedAt      int64           `json:"createdAt"`
	ID             string          `json:"id"`
	IdentityKey    string          `json:"identityKey"`
	Payload        json.RawMessage `json:"payload"`
}

var threadAttachmentSpec = objectSpec{
	fields: []objectField{
		{name: "attachmentType", flags: fieldRequired | fieldRejectNull},
		{name: "createdAt", flags: fieldRequired | fieldRejectNull},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "identityKey", flags: fieldRequired | fieldRejectNull},
		{name: "payload", flags: fieldRequired},
	},
}

func (v ThreadAttachment) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachment
	return json.Marshal(wire(v))
}

func (v *ThreadAttachment) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachment
	return decodeObject(data, &threadAttachmentSpec, (*wire)(v))
}

type ThreadAttachmentAddOutcome string

const (
	ThreadAttachmentAddOutcomeCreated  ThreadAttachmentAddOutcome = "created"
	ThreadAttachmentAddOutcomeExisting ThreadAttachmentAddOutcome = "existing"
)

type ThreadAttachmentAddParams struct {
	AttachmentType string          `json:"attachmentType"`
	IdentityKey    string          `json:"identityKey"`
	Payload        json.RawMessage `json:"payload"`
	ThreadID       string          `json:"threadId"`
}

var threadAttachmentAddParamsSpec = objectSpec{
	fields: []objectField{
		{name: "attachmentType", flags: fieldRequired | fieldRejectNull},
		{name: "identityKey", flags: fieldRequired | fieldRejectNull},
		{name: "payload", flags: fieldRequired},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAttachmentAddParams) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentAddParams
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentAddParams) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentAddParams
	return decodeObject(data, &threadAttachmentAddParamsSpec, (*wire)(v))
}

type ThreadAttachmentAddResponse struct {
	Attachment ThreadAttachment           `json:"attachment"`
	Outcome    ThreadAttachmentAddOutcome `json:"outcome"`
}

var threadAttachmentAddResponseSpec = objectSpec{
	fields: []objectField{
		{name: "attachment", flags: fieldRequired | fieldRejectNull},
		{name: "outcome", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAttachmentAddResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentAddResponse
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentAddResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentAddResponse
	return decodeObject(data, &threadAttachmentAddResponseSpec, (*wire)(v))
}

type ThreadAttachmentListParams struct {
	Cursor   Optional[string] `json:"cursor,omitzero"`
	Limit    Optional[uint32] `json:"limit,omitzero"`
	ThreadID string           `json:"threadId"`
}

var threadAttachmentListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAttachmentListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentListParams
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentListParams
	var s objectScan
	if _, err := s.begin(data, &threadAttachmentListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadAttachmentListResponse struct {
	Data       []ThreadAttachment `json:"data"`
	NextCursor Optional[string]   `json:"nextCursor,omitzero"`
}

var threadAttachmentListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadAttachmentListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentListResponse
	return decodeObject(data, &threadAttachmentListResponseSpec, (*wire)(v))
}

type ThreadAttachmentOperation string

const (
	ThreadAttachmentOperationCreated ThreadAttachmentOperation = "created"
	ThreadAttachmentOperationDeleted ThreadAttachmentOperation = "deleted"
)

type ThreadAttachmentOwner struct {
	Archived bool   `json:"archived"`
	ThreadID string `json:"threadId"`
}

var threadAttachmentOwnerSpec = objectSpec{
	fields: []objectField{
		{name: "archived", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAttachmentOwner) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentOwner
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentOwner) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentOwner
	return decodeObject(data, &threadAttachmentOwnerSpec, (*wire)(v))
}

type ThreadAttachmentOwnerListParams struct {
	Archived       Optional[bool]   `json:"archived,omitzero"`
	AttachmentType string           `json:"attachmentType"`
	Cursor         Optional[string] `json:"cursor,omitzero"`
	IdentityKey    string           `json:"identityKey"`
	Limit          Optional[uint32] `json:"limit,omitzero"`
}

var threadAttachmentOwnerListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "archived"},
		{name: "attachmentType", flags: fieldRequired | fieldRejectNull},
		{name: "cursor"},
		{name: "identityKey", flags: fieldRequired | fieldRejectNull},
		{name: "limit"},
	},
}

func (v ThreadAttachmentOwnerListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentOwnerListParams
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentOwnerListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentOwnerListParams
	var s objectScan
	if _, err := s.begin(data, &threadAttachmentOwnerListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadAttachmentOwnerListResponse struct {
	Data       []ThreadAttachmentOwner `json:"data"`
	NextCursor Optional[string]        `json:"nextCursor,omitzero"`
}

var threadAttachmentOwnerListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadAttachmentOwnerListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentOwnerListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentOwnerListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentOwnerListResponse
	return decodeObject(data, &threadAttachmentOwnerListResponseSpec, (*wire)(v))
}

type ThreadAttachmentRemoveParams struct {
	AttachmentType string `json:"attachmentType"`
	IdentityKey    string `json:"identityKey"`
	ThreadID       string `json:"threadId"`
}

var threadAttachmentRemoveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "attachmentType", flags: fieldRequired | fieldRejectNull},
		{name: "identityKey", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAttachmentRemoveParams) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentRemoveParams
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentRemoveParams) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentRemoveParams
	return decodeObject(data, &threadAttachmentRemoveParamsSpec, (*wire)(v))
}

type ThreadAttachmentRemoveResponse map[string]json.RawMessage

func (v ThreadAttachmentRemoveResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadAttachmentRemoveResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadAttachmentRemoveResponse(raw)
	return nil
}

type ThreadAttachmentUpdatedNotification struct {
	AttachmentID   string                    `json:"attachmentId"`
	AttachmentType string                    `json:"attachmentType"`
	IdentityKey    string                    `json:"identityKey"`
	Operation      ThreadAttachmentOperation `json:"operation"`
	ThreadID       string                    `json:"threadId"`
}

var threadAttachmentUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "attachmentId", flags: fieldRequired | fieldRejectNull},
		{name: "attachmentType", flags: fieldRequired | fieldRejectNull},
		{name: "identityKey", flags: fieldRequired | fieldRejectNull},
		{name: "operation", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadAttachmentUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadAttachmentUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadAttachmentUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadAttachmentUpdatedNotification
	return decodeObject(data, &threadAttachmentUpdatedNotificationSpec, (*wire)(v))
}

type ThreadBackgroundTerminal struct {
	Command    string              `json:"command"`
	CpuPercent Optional[float64]   `json:"cpuPercent,omitzero"`
	Cwd        LegacyAppPathString `json:"cwd"`
	ItemID     string              `json:"itemId"`
	OsPid      Optional[uint32]    `json:"osPid,omitzero"`
	ProcessID  string              `json:"processId"`
	RssKb      Optional[uint64]    `json:"rssKb,omitzero"`
}

var threadBackgroundTerminalSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldRequired | fieldRejectNull},
		{name: "cpuPercent"},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "osPid"},
		{name: "processId", flags: fieldRequired | fieldRejectNull},
		{name: "rssKb"},
	},
}

func (v ThreadBackgroundTerminal) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminal
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminal) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminal
	var s objectScan
	if _, err := s.begin(data, &threadBackgroundTerminalSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueOsPid, ok := v.OsPid.Value(); ok {
		if valueOsPid < 0 {
			return DecodeError{Field: "osPid", Reason: "below minimum 0"}
		}
	}
	if valueRssKb, ok := v.RssKb.Value(); ok {
		if valueRssKb < 0 {
			return DecodeError{Field: "rssKb", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadBackgroundTerminalsCleanParams struct {
	ThreadID string `json:"threadId"`
}

var threadBackgroundTerminalsCleanParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadBackgroundTerminalsCleanParams) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsCleanParams
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsCleanParams) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsCleanParams
	return decodeObject(data, &threadBackgroundTerminalsCleanParamsSpec, (*wire)(v))
}

type ThreadBackgroundTerminalsCleanResponse map[string]json.RawMessage

func (v ThreadBackgroundTerminalsCleanResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadBackgroundTerminalsCleanResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadBackgroundTerminalsCleanResponse(raw)
	return nil
}

type ThreadBackgroundTerminalsListParams struct {
	Cursor   Optional[string] `json:"cursor,omitzero"`
	Limit    Optional[uint32] `json:"limit,omitzero"`
	ThreadID string           `json:"threadId"`
}

var threadBackgroundTerminalsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadBackgroundTerminalsListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsListParams
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsListParams
	var s objectScan
	if _, err := s.begin(data, &threadBackgroundTerminalsListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadBackgroundTerminalsListResponse struct {
	Data       []ThreadBackgroundTerminal `json:"data"`
	NextCursor Optional[string]           `json:"nextCursor,omitzero"`
}

var threadBackgroundTerminalsListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadBackgroundTerminalsListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsListResponse
	return decodeObject(data, &threadBackgroundTerminalsListResponseSpec, (*wire)(v))
}

type ThreadBackgroundTerminalsPendingParams struct {
	ThreadID string `json:"threadId"`
}

var threadBackgroundTerminalsPendingParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadBackgroundTerminalsPendingParams) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsPendingParams
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsPendingParams) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsPendingParams
	return decodeObject(data, &threadBackgroundTerminalsPendingParamsSpec, (*wire)(v))
}

type ThreadBackgroundTerminalsPendingResponse struct {
	ActiveTurnID       Optional[string]            `json:"activeTurnId,omitzero"`
	NotificationQueued bool                        `json:"notificationQueued"`
	Terminals          []PendingBackgroundTerminal `json:"terminals"`
	TurnStarting       bool                        `json:"turnStarting"`
}

var threadBackgroundTerminalsPendingResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activeTurnId"},
		{name: "notificationQueued", flags: fieldRequired | fieldRejectNull},
		{name: "terminals", flags: fieldRequired | fieldRejectNull},
		{name: "turnStarting", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadBackgroundTerminalsPendingResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsPendingResponse
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsPendingResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsPendingResponse
	return decodeObject(data, &threadBackgroundTerminalsPendingResponseSpec, (*wire)(v))
}

type ThreadBackgroundTerminalsTerminateParams struct {
	ProcessID string `json:"processId"`
	ThreadID  string `json:"threadId"`
}

var threadBackgroundTerminalsTerminateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "processId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadBackgroundTerminalsTerminateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsTerminateParams
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsTerminateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsTerminateParams
	return decodeObject(data, &threadBackgroundTerminalsTerminateParamsSpec, (*wire)(v))
}

type ThreadBackgroundTerminalsTerminateResponse struct {
	Terminated bool `json:"terminated"`
}

var threadBackgroundTerminalsTerminateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "terminated", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadBackgroundTerminalsTerminateResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadBackgroundTerminalsTerminateResponse
	return json.Marshal(wire(v))
}

func (v *ThreadBackgroundTerminalsTerminateResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadBackgroundTerminalsTerminateResponse
	return decodeObject(data, &threadBackgroundTerminalsTerminateResponseSpec, (*wire)(v))
}

type ThreadClosedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadClosedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadClosedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadClosedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadClosedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadClosedNotification
	return decodeObject(data, &threadClosedNotificationSpec, (*wire)(v))
}

type ThreadCompactStartParams struct {
	ThreadID string `json:"threadId"`
}

var threadCompactStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadCompactStartParams) MarshalJSON() ([]byte, error) {
	type wire ThreadCompactStartParams
	return json.Marshal(wire(v))
}

func (v *ThreadCompactStartParams) UnmarshalJSON(data []byte) error {
	type wire ThreadCompactStartParams
	return decodeObject(data, &threadCompactStartParamsSpec, (*wire)(v))
}

type ThreadCompactStartResponse map[string]json.RawMessage

func (v ThreadCompactStartResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadCompactStartResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadCompactStartResponse(raw)
	return nil
}

type ThreadDecrementElicitationParams struct {
	ThreadID string `json:"threadId"`
}

var threadDecrementElicitationParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadDecrementElicitationParams) MarshalJSON() ([]byte, error) {
	type wire ThreadDecrementElicitationParams
	return json.Marshal(wire(v))
}

func (v *ThreadDecrementElicitationParams) UnmarshalJSON(data []byte) error {
	type wire ThreadDecrementElicitationParams
	return decodeObject(data, &threadDecrementElicitationParamsSpec, (*wire)(v))
}

type ThreadDecrementElicitationResponse struct {
	Count  int64 `json:"count"`
	Paused bool  `json:"paused"`
}

var threadDecrementElicitationResponseSpec = objectSpec{
	fields: []objectField{
		{name: "count", flags: fieldRequired | fieldRejectNull},
		{name: "paused", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadDecrementElicitationResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadDecrementElicitationResponse
	return json.Marshal(wire(v))
}

func (v *ThreadDecrementElicitationResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadDecrementElicitationResponse
	return decodeObject(data, &threadDecrementElicitationResponseSpec, (*wire)(v))
}

type ThreadDeleteParams struct {
	ThreadID string `json:"threadId"`
}

var threadDeleteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadDeleteParams) MarshalJSON() ([]byte, error) {
	type wire ThreadDeleteParams
	return json.Marshal(wire(v))
}

func (v *ThreadDeleteParams) UnmarshalJSON(data []byte) error {
	type wire ThreadDeleteParams
	return decodeObject(data, &threadDeleteParamsSpec, (*wire)(v))
}

type ThreadDeleteResponse map[string]json.RawMessage

func (v ThreadDeleteResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadDeleteResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadDeleteResponse(raw)
	return nil
}

type ThreadDeletedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadDeletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadDeletedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadDeletedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadDeletedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadDeletedNotification
	return decodeObject(data, &threadDeletedNotificationSpec, (*wire)(v))
}

type ThreadEnvironment struct {
	Cwd                   LegacyAppPathString   `json:"cwd"`
	EnvironmentID         string                `json:"environmentId"`
	RuntimeWorkspaceRoots []LegacyAppPathString `json:"runtimeWorkspaceRoots"`
}

var threadEnvironmentSpec = objectSpec{
	fields: []objectField{
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
		{name: "runtimeWorkspaceRoots", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadEnvironment) MarshalJSON() ([]byte, error) {
	type wire ThreadEnvironment
	return json.Marshal(wire(v))
}

func (v *ThreadEnvironment) UnmarshalJSON(data []byte) error {
	type wire ThreadEnvironment
	return decodeObject(data, &threadEnvironmentSpec, (*wire)(v))
}

type ThreadExtra struct {
	AgentWorktree Optional[ThreadAgentWorktree] `json:"agentWorktree,omitzero"`
}

var threadExtraSpec = objectSpec{
	fields: []objectField{
		{name: "agentWorktree"},
	},
}

func (v ThreadExtra) MarshalJSON() ([]byte, error) {
	type wire ThreadExtra
	return json.Marshal(wire(v))
}

func (v *ThreadExtra) UnmarshalJSON(data []byte) error {
	type wire ThreadExtra
	return decodeObject(data, &threadExtraSpec, (*wire)(v))
}

type ThreadForkParams struct {
	ApprovalPolicy                   Optional[AskForApproval]             `json:"approvalPolicy,omitzero"`
	ApprovalsReviewer                Optional[ApprovalsReviewer]          `json:"approvalsReviewer,omitzero"`
	AttachToSource                   OptionalNonNull[bool]                `json:"attachToSource,omitzero"`
	BaseInstructions                 Optional[string]                     `json:"baseInstructions,omitzero"`
	BeforeTurnID                     Optional[string]                     `json:"beforeTurnId,omitzero"`
	Config                           Optional[map[string]json.RawMessage] `json:"config,omitzero"`
	Cwd                              Optional[string]                     `json:"cwd,omitzero"`
	DeferGoalContinuation            OptionalNonNull[bool]                `json:"deferGoalContinuation,omitzero"`
	DeveloperInstructions            Optional[string]                     `json:"developerInstructions,omitzero"`
	Ephemeral                        OptionalNonNull[bool]                `json:"ephemeral,omitzero"`
	ExcludeTurns                     OptionalNonNull[bool]                `json:"excludeTurns,omitzero"`
	ExperimentalDisablePredictionMcp OptionalNonNull[bool]                `json:"experimentalDisablePredictionMcp,omitzero"`
	ExperimentalPredictionMode       OptionalNonNull[bool]                `json:"experimentalPredictionMode,omitzero"`
	LastTurnID                       Optional[string]                     `json:"lastTurnId,omitzero"`
	Model                            Optional[string]                     `json:"model,omitzero"`
	ModelProvider                    Optional[string]                     `json:"modelProvider,omitzero"`
	Path                             Optional[string]                     `json:"path,omitzero"`
	Permissions                      Optional[string]                     `json:"permissions,omitzero"`
	RuntimeWorkspaceRoots            Optional[[]AbsolutePathBuf]          `json:"runtimeWorkspaceRoots,omitzero"`
	Sandbox                          Optional[SandboxMode]                `json:"sandbox,omitzero"`
	ServiceTier                      Optional[string]                     `json:"serviceTier,omitzero"`
	ThreadID                         string                               `json:"threadId"`
	ThreadSource                     Optional[ThreadSource]               `json:"threadSource,omitzero"`
}

var threadForkParamsSpec = objectSpec{
	fields: []objectField{
		{name: "approvalPolicy"},
		{name: "approvalsReviewer"},
		{name: "attachToSource", flags: fieldNonNullOptional},
		{name: "baseInstructions"},
		{name: "beforeTurnId"},
		{name: "config"},
		{name: "cwd"},
		{name: "deferGoalContinuation", flags: fieldNonNullOptional},
		{name: "developerInstructions"},
		{name: "ephemeral", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "excludeTurns", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "experimentalDisablePredictionMcp", flags: fieldNonNullOptional},
		{name: "experimentalPredictionMode", flags: fieldNonNullOptional},
		{name: "lastTurnId"},
		{name: "model"},
		{name: "modelProvider"},
		{name: "path"},
		{name: "permissions"},
		{name: "runtimeWorkspaceRoots"},
		{name: "sandbox"},
		{name: "serviceTier"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "threadSource"},
	},
}

func (v ThreadForkParams) MarshalJSON() ([]byte, error) {
	type wire ThreadForkParams
	w := wire(v)
	w.Ephemeral = nonNullWithoutFalse(w.Ephemeral)
	w.ExcludeTurns = nonNullWithoutFalse(w.ExcludeTurns)
	return json.Marshal(w)
}

func (v *ThreadForkParams) UnmarshalJSON(data []byte) error {
	type wire ThreadForkParams
	return decodeObject(data, &threadForkParamsSpec, (*wire)(v))
}

type ThreadForkResponse struct {
	ActivePermissionProfile Optional[ActivePermissionProfile]      `json:"activePermissionProfile,omitzero"`
	ApprovalPolicy          AskForApproval                         `json:"approvalPolicy"`
	ApprovalsReviewer       ApprovalsReviewer                      `json:"approvalsReviewer"`
	Cwd                     AbsolutePathBuf                        `json:"cwd"`
	DisabledPluginIDs       OptionalNonNull[[]string]              `json:"disabledPluginIds,omitzero"`
	InstructionSources      OptionalNonNull[[]LegacyAppPathString] `json:"instructionSources,omitzero"`
	Model                   string                                 `json:"model"`
	ModelProvider           string                                 `json:"modelProvider"`
	MultiAgentMode          OptionalNonNull[MultiAgentMode]        `json:"multiAgentMode,omitzero"`
	ReasoningEffort         Optional[ReasoningEffort]              `json:"reasoningEffort,omitzero"`
	RuntimeWorkspaceRoots   OptionalNonNull[[]AbsolutePathBuf]     `json:"runtimeWorkspaceRoots,omitzero"`
	Sandbox                 SandboxPolicy                          `json:"sandbox"`
	ServiceTier             Optional[string]                       `json:"serviceTier,omitzero"`
	Thread                  Thread                                 `json:"thread"`
}

var threadForkResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activePermissionProfile", defaultJSON: "null"},
		{name: "approvalPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "approvalsReviewer", flags: fieldRequired | fieldRejectNull},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "disabledPluginIds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "instructionSources", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "modelProvider", flags: fieldRequired | fieldRejectNull},
		{name: "multiAgentMode", flags: fieldNonNullOptional, defaultJSON: "\"explicitRequestOnly\""},
		{name: "reasoningEffort"},
		{name: "runtimeWorkspaceRoots", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "sandbox", flags: fieldRequired | fieldRejectNull},
		{name: "serviceTier"},
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadForkResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadForkResponse
	return json.Marshal(wire(v))
}

func (v *ThreadForkResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadForkResponse
	return decodeObject(data, &threadForkResponseSpec, (*wire)(v))
}

type ThreadGoal struct {
	CreatedAt       int64            `json:"createdAt"`
	Objective       string           `json:"objective"`
	Status          ThreadGoalStatus `json:"status"`
	ThreadID        string           `json:"threadId"`
	TimeUsedSeconds int64            `json:"timeUsedSeconds"`
	TokenBudget     Optional[int64]  `json:"tokenBudget,omitzero"`
	TokensUsed      int64            `json:"tokensUsed"`
	UpdatedAt       int64            `json:"updatedAt"`
}

var threadGoalSpec = objectSpec{
	fields: []objectField{
		{name: "createdAt", flags: fieldRequired | fieldRejectNull},
		{name: "objective", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "timeUsedSeconds", flags: fieldRequired | fieldRejectNull},
		{name: "tokenBudget"},
		{name: "tokensUsed", flags: fieldRequired | fieldRejectNull},
		{name: "updatedAt", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadGoal) MarshalJSON() ([]byte, error) {
	type wire ThreadGoal
	return json.Marshal(wire(v))
}

func (v *ThreadGoal) UnmarshalJSON(data []byte) error {
	type wire ThreadGoal
	return decodeObject(data, &threadGoalSpec, (*wire)(v))
}

type ThreadGoalClearParams struct {
	Origin   Optional[ThreadGoalMutationOrigin] `json:"origin,omitzero"`
	ThreadID string                             `json:"threadId"`
}

var threadGoalClearParamsSpec = objectSpec{
	fields: []objectField{
		{name: "origin"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadGoalClearParams) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalClearParams
	return json.Marshal(wire(v))
}

func (v *ThreadGoalClearParams) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalClearParams
	return decodeObject(data, &threadGoalClearParamsSpec, (*wire)(v))
}

type ThreadGoalClearResponse struct {
	Cleared bool `json:"cleared"`
}

var threadGoalClearResponseSpec = objectSpec{
	fields: []objectField{
		{name: "cleared", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadGoalClearResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalClearResponse
	return json.Marshal(wire(v))
}

func (v *ThreadGoalClearResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalClearResponse
	return decodeObject(data, &threadGoalClearResponseSpec, (*wire)(v))
}

type ThreadGoalClearedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadGoalClearedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadGoalClearedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalClearedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadGoalClearedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalClearedNotification
	return decodeObject(data, &threadGoalClearedNotificationSpec, (*wire)(v))
}

type ThreadGoalGetParams struct {
	ThreadID string `json:"threadId"`
}

var threadGoalGetParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadGoalGetParams) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalGetParams
	return json.Marshal(wire(v))
}

func (v *ThreadGoalGetParams) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalGetParams
	return decodeObject(data, &threadGoalGetParamsSpec, (*wire)(v))
}

type ThreadGoalGetResponse struct {
	Goal Optional[ThreadGoal] `json:"goal,omitzero"`
}

var threadGoalGetResponseSpec = objectSpec{
	fields: []objectField{
		{name: "goal"},
	},
}

func (v ThreadGoalGetResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalGetResponse
	return json.Marshal(wire(v))
}

func (v *ThreadGoalGetResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalGetResponse
	return decodeObject(data, &threadGoalGetResponseSpec, (*wire)(v))
}

type ThreadGoalMutationOrigin string

const (
	ThreadGoalMutationOriginUser      ThreadGoalMutationOrigin = "user"
	ThreadGoalMutationOriginAutomatic ThreadGoalMutationOrigin = "automatic"
)

type ThreadGoalSetParams struct {
	Objective   Optional[string]                   `json:"objective,omitzero"`
	Origin      Optional[ThreadGoalMutationOrigin] `json:"origin,omitzero"`
	Status      Optional[ThreadGoalStatus]         `json:"status,omitzero"`
	ThreadID    string                             `json:"threadId"`
	TokenBudget Optional[int64]                    `json:"tokenBudget,omitzero"`
}

var threadGoalSetParamsSpec = objectSpec{
	fields: []objectField{
		{name: "objective"},
		{name: "origin"},
		{name: "status"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "tokenBudget"},
	},
}

func (v ThreadGoalSetParams) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalSetParams
	return json.Marshal(wire(v))
}

func (v *ThreadGoalSetParams) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalSetParams
	return decodeObject(data, &threadGoalSetParamsSpec, (*wire)(v))
}

type ThreadGoalSetResponse struct {
	Goal ThreadGoal `json:"goal"`
}

var threadGoalSetResponseSpec = objectSpec{
	fields: []objectField{
		{name: "goal", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadGoalSetResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalSetResponse
	return json.Marshal(wire(v))
}

func (v *ThreadGoalSetResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalSetResponse
	return decodeObject(data, &threadGoalSetResponseSpec, (*wire)(v))
}

type ThreadGoalStatus string

const (
	ThreadGoalStatusActive        ThreadGoalStatus = "active"
	ThreadGoalStatusPaused        ThreadGoalStatus = "paused"
	ThreadGoalStatusBlocked       ThreadGoalStatus = "blocked"
	ThreadGoalStatusUsageLimited  ThreadGoalStatus = "usageLimited"
	ThreadGoalStatusBudgetLimited ThreadGoalStatus = "budgetLimited"
	ThreadGoalStatusComplete      ThreadGoalStatus = "complete"
)

type ThreadGoalUpdatedNotification struct {
	Goal     ThreadGoal       `json:"goal"`
	ThreadID string           `json:"threadId"`
	TurnID   Optional[string] `json:"turnId,omitzero"`
}

var threadGoalUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "goal", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId"},
	},
}

func (v ThreadGoalUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadGoalUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadGoalUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadGoalUpdatedNotification
	return decodeObject(data, &threadGoalUpdatedNotificationSpec, (*wire)(v))
}

type ThreadHistoryMode string

const (
	ThreadHistoryModeLegacy    ThreadHistoryMode = "legacy"
	ThreadHistoryModePaginated ThreadHistoryMode = "paginated"
)

type ThreadID string

type ThreadIncrementElicitationParams struct {
	ThreadID string `json:"threadId"`
}

var threadIncrementElicitationParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadIncrementElicitationParams) MarshalJSON() ([]byte, error) {
	type wire ThreadIncrementElicitationParams
	return json.Marshal(wire(v))
}

func (v *ThreadIncrementElicitationParams) UnmarshalJSON(data []byte) error {
	type wire ThreadIncrementElicitationParams
	return decodeObject(data, &threadIncrementElicitationParamsSpec, (*wire)(v))
}

type ThreadIncrementElicitationResponse struct {
	Count  int64 `json:"count"`
	Paused bool  `json:"paused"`
}

var threadIncrementElicitationResponseSpec = objectSpec{
	fields: []objectField{
		{name: "count", flags: fieldRequired | fieldRejectNull},
		{name: "paused", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadIncrementElicitationResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadIncrementElicitationResponse
	return json.Marshal(wire(v))
}

func (v *ThreadIncrementElicitationResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadIncrementElicitationResponse
	return decodeObject(data, &threadIncrementElicitationResponseSpec, (*wire)(v))
}

type ThreadInjectItemsParams struct {
	Items    []json.RawMessage `json:"items"`
	ThreadID string            `json:"threadId"`
}

var threadInjectItemsParamsSpec = objectSpec{
	fields: []objectField{
		{name: "items", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadInjectItemsParams) MarshalJSON() ([]byte, error) {
	type wire ThreadInjectItemsParams
	return json.Marshal(wire(v))
}

func (v *ThreadInjectItemsParams) UnmarshalJSON(data []byte) error {
	type wire ThreadInjectItemsParams
	return decodeObject(data, &threadInjectItemsParamsSpec, (*wire)(v))
}

type ThreadInjectItemsResponse map[string]json.RawMessage

func (v ThreadInjectItemsResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadInjectItemsResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadInjectItemsResponse(raw)
	return nil
}

type ThreadItem struct {
	Action                Optional[WebSearchAction]                    `json:"action,omitzero"`
	AgentPath             OptionalNonNull[string]                      `json:"agentPath,omitzero"`
	AgentThreadID         OptionalNonNull[string]                      `json:"agentThreadId,omitzero"`
	AgentsStates          OptionalNonNull[map[string]CollabAgentState] `json:"agentsStates,omitzero"`
	AggregatedOutput      Optional[string]                             `json:"aggregatedOutput,omitzero"`
	AppContext            Optional[McpToolCallAppContext]              `json:"appContext,omitzero"`
	Arguments             json.RawMessage                              `json:"arguments,omitempty"`
	Changes               OptionalNonNull[[]FileUpdateChange]          `json:"changes,omitzero"`
	ClientID              Optional[string]                             `json:"clientId,omitzero"`
	Command               OptionalNonNull[string]                      `json:"command,omitzero"`
	CommandActions        OptionalNonNull[[]CommandAction]             `json:"commandActions,omitzero"`
	CompletedAtMs         Optional[int64]                              `json:"completedAtMs,omitzero"`
	Content               json.RawMessage                              `json:"content,omitempty"`
	ContentItems          Optional[[]DynamicToolCallOutputContentItem] `json:"contentItems,omitzero"`
	Cwd                   OptionalNonNull[LegacyAppPathString]         `json:"cwd,omitzero"`
	Delivery              Optional[AgentMessageDelivery]               `json:"delivery,omitzero"`
	DurationMs            json.RawMessage                              `json:"durationMs,omitempty"`
	Error                 Optional[McpToolCallError]                   `json:"error,omitzero"`
	ExitCode              Optional[int32]                              `json:"exitCode,omitzero"`
	Failure               Optional[ImageGenerationFailure]             `json:"failure,omitzero"`
	FileID                OptionalNonNull[string]                      `json:"fileId,omitzero"`
	Fragments             OptionalNonNull[[]HookPromptFragment]        `json:"fragments,omitzero"`
	GenerationID          Optional[string]                             `json:"generationId,omitzero"`
	ID                    OptionalNonNull[string]                      `json:"id,omitzero"`
	InputImages           OptionalNonNull[[]ImageGenerationInputImage] `json:"inputImages,omitzero"`
	Kind                  OptionalNonNull[SubAgentActivityKind]        `json:"kind,omitzero"`
	McpAppResourceUri     Optional[string]                             `json:"mcpAppResourceUri,omitzero"`
	McpAppUi              Optional[McpAppUi]                           `json:"mcpAppUi,omitzero"`
	MemoryCitation        Optional[MemoryCitation]                     `json:"memoryCitation,omitzero"`
	Model                 Optional[string]                             `json:"model,omitzero"`
	Name                  OptionalNonNull[string]                      `json:"name,omitzero"`
	Namespace             Optional[string]                             `json:"namespace,omitzero"`
	Notification          Optional[string]                             `json:"notification,omitzero"`
	Output                OptionalNonNull[FunctionCallOutputBody]      `json:"output,omitzero"`
	Path                  OptionalNonNull[LegacyAppPathString]         `json:"path,omitzero"`
	Phase                 Optional[MessagePhase]                       `json:"phase,omitzero"`
	PluginID              Optional[string]                             `json:"pluginId,omitzero"`
	ProcessID             Optional[string]                             `json:"processId,omitzero"`
	Prompt                Optional[string]                             `json:"prompt,omitzero"`
	Query                 OptionalNonNull[string]                      `json:"query,omitzero"`
	Questions             Optional[[]AsyncUserInputQuestion]           `json:"questions,omitzero"`
	ReadOnlyHint          Optional[bool]                               `json:"readOnlyHint,omitzero"`
	ReasoningEffort       Optional[ReasoningEffort]                    `json:"reasoningEffort,omitzero"`
	ReceiverThreadIDs     OptionalNonNull[[]string]                    `json:"receiverThreadIds,omitzero"`
	Result                json.RawMessage                              `json:"result,omitempty"`
	Results               Optional[[]json.RawMessage]                  `json:"results,omitzero"`
	Review                OptionalNonNull[string]                      `json:"review,omitzero"`
	RevisedPrompt         Optional[string]                             `json:"revisedPrompt,omitzero"`
	RunID                 OptionalNonNull[string]                      `json:"runId,omitzero"`
	SavedPath             Optional[AbsolutePathBuf]                    `json:"savedPath,omitzero"`
	ScriptPath            Optional[string]                             `json:"scriptPath,omitzero"`
	SenderThreadID        OptionalNonNull[string]                      `json:"senderThreadId,omitzero"`
	Server                OptionalNonNull[string]                      `json:"server,omitzero"`
	Source                OptionalNonNull[CommandExecutionSource]      `json:"source,omitzero"`
	StartedAtMs           Optional[int64]                              `json:"startedAtMs,omitzero"`
	Status                OptionalNonNull[CommandExecutionStatus]      `json:"status,omitzero"`
	Success               Optional[bool]                               `json:"success,omitzero"`
	Summary               OptionalNonNull[[]string]                    `json:"summary,omitzero"`
	Text                  OptionalNonNull[string]                      `json:"text,omitzero"`
	Tool                  OptionalNonNull[string]                      `json:"tool,omitzero"`
	TransparentBackground Optional[bool]                               `json:"transparentBackground,omitzero"`
	TypeValue             string                                       `json:"type"`
	RawJSON               json.RawMessage                              `json:"-"`
}

var threadItemSpec = objectSpec{
	fields: []objectField{
		{name: "action"},
		{name: "agentPath", flags: fieldNonNullOptional},
		{name: "agentThreadId", flags: fieldNonNullOptional},
		{name: "agentsStates", flags: fieldNonNullOptional},
		{name: "aggregatedOutput"},
		{name: "appContext"},
		{name: "arguments"},
		{name: "changes", flags: fieldNonNullOptional},
		{name: "clientId"},
		{name: "command", flags: fieldNonNullOptional},
		{name: "commandActions", flags: fieldNonNullOptional},
		{name: "completedAtMs"},
		{name: "content", flags: fieldRejectNull},
		{name: "contentItems"},
		{name: "cwd", flags: fieldNonNullOptional},
		{name: "delivery"},
		{name: "durationMs"},
		{name: "error"},
		{name: "exitCode"},
		{name: "failure"},
		{name: "fileId", flags: fieldNonNullOptional, anyOfTags: []string{"\"imageGeneration\""}},
		{name: "fragments", flags: fieldNonNullOptional},
		{name: "generationId"},
		{name: "id", flags: fieldNonNullOptional},
		{name: "inputImages", flags: fieldNonNullOptional},
		{name: "kind", flags: fieldNonNullOptional},
		{name: "mcpAppResourceUri"},
		{name: "mcpAppUi"},
		{name: "memoryCitation"},
		{name: "model"},
		{name: "name", flags: fieldNonNullOptional},
		{name: "namespace"},
		{name: "notification"},
		{name: "output", flags: fieldNonNullOptional},
		{name: "path", flags: fieldNonNullOptional},
		{name: "phase"},
		{name: "pluginId"},
		{name: "processId"},
		{name: "prompt"},
		{name: "query", flags: fieldNonNullOptional},
		{name: "questions"},
		{name: "readOnlyHint"},
		{name: "reasoningEffort"},
		{name: "receiverThreadIds", flags: fieldNonNullOptional},
		{name: "result", anyOfTags: []string{"\"imageGeneration\""}},
		{name: "results"},
		{name: "review", flags: fieldNonNullOptional},
		{name: "revisedPrompt"},
		{name: "runId", flags: fieldNonNullOptional},
		{name: "savedPath"},
		{name: "scriptPath"},
		{name: "senderThreadId", flags: fieldNonNullOptional},
		{name: "server", flags: fieldNonNullOptional},
		{name: "source", flags: fieldNonNullOptional},
		{name: "startedAtMs"},
		{name: "status", flags: fieldNonNullOptional},
		{name: "success"},
		{name: "summary", flags: fieldNonNullOptional},
		{name: "text", flags: fieldNonNullOptional},
		{name: "tool", flags: fieldNonNullOptional},
		{name: "transparentBackground"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  61,
	tags: []string{"userMessage", "hookPrompt", "agentMessage", "functionCallOutput", "plan", "reasoning", "commandExecution", "fileChange", "mcpToolCall", "dynamicToolCall", "collabAgentToolCall", "subAgentActivity", "webSearch", "imageView", "sleep", "imageGeneration", "workflow", "enteredReviewMode", "exitedReviewMode", "contextCompaction"},
}

func (v ThreadItem) MarshalJSON() ([]byte, error) {
	if bytes.Equal(v.Content, []byte("null")) {
		return nil, DecodeError{Field: "content", Reason: "cannot be null"}
	}
	type wire ThreadItem
	w := wire(v)
	switch v.TypeValue {
	case "userMessage":
		if len(v.Content) == 0 {
			return nil, DecodeError{Field: "content", Reason: "missing required field for type userMessage"}
		}
		if bytes.Equal(v.Content, []byte("null")) {
			return nil, DecodeError{Field: "content", Reason: "cannot be null"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type userMessage"}
		}
	case "hookPrompt":
		if !v.Fragments.IsSet() {
			return nil, DecodeError{Field: "fragments", Reason: "missing required field for type hookPrompt"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type hookPrompt"}
		}
	case "agentMessage":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type agentMessage"}
		}
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type agentMessage"}
		}
	case "functionCallOutput":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type functionCallOutput"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type functionCallOutput"}
		}
		if !v.Output.IsSet() {
			return nil, DecodeError{Field: "output", Reason: "missing required field for type functionCallOutput"}
		}
	case "plan":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type plan"}
		}
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type plan"}
		}
	case "reasoning":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type reasoning"}
		}
	case "commandExecution":
		if !v.Command.IsSet() {
			return nil, DecodeError{Field: "command", Reason: "missing required field for type commandExecution"}
		}
		if !v.CommandActions.IsSet() {
			return nil, DecodeError{Field: "commandActions", Reason: "missing required field for type commandExecution"}
		}
		if !v.Cwd.IsSet() {
			return nil, DecodeError{Field: "cwd", Reason: "missing required field for type commandExecution"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type commandExecution"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type commandExecution"}
		}
	case "fileChange":
		if !v.Changes.IsSet() {
			return nil, DecodeError{Field: "changes", Reason: "missing required field for type fileChange"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type fileChange"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type fileChange"}
		}
	case "mcpToolCall":
		if len(v.Arguments) == 0 {
			return nil, DecodeError{Field: "arguments", Reason: "missing required field for type mcpToolCall"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type mcpToolCall"}
		}
		if !v.Server.IsSet() {
			return nil, DecodeError{Field: "server", Reason: "missing required field for type mcpToolCall"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type mcpToolCall"}
		}
		if !v.Tool.IsSet() {
			return nil, DecodeError{Field: "tool", Reason: "missing required field for type mcpToolCall"}
		}
	case "dynamicToolCall":
		if len(v.Arguments) == 0 {
			return nil, DecodeError{Field: "arguments", Reason: "missing required field for type dynamicToolCall"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type dynamicToolCall"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type dynamicToolCall"}
		}
		if !v.Tool.IsSet() {
			return nil, DecodeError{Field: "tool", Reason: "missing required field for type dynamicToolCall"}
		}
	case "collabAgentToolCall":
		if !v.AgentsStates.IsSet() {
			return nil, DecodeError{Field: "agentsStates", Reason: "missing required field for type collabAgentToolCall"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type collabAgentToolCall"}
		}
		if !v.ReceiverThreadIDs.IsSet() {
			return nil, DecodeError{Field: "receiverThreadIds", Reason: "missing required field for type collabAgentToolCall"}
		}
		if !v.SenderThreadID.IsSet() {
			return nil, DecodeError{Field: "senderThreadId", Reason: "missing required field for type collabAgentToolCall"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type collabAgentToolCall"}
		}
		if !v.Tool.IsSet() {
			return nil, DecodeError{Field: "tool", Reason: "missing required field for type collabAgentToolCall"}
		}
	case "subAgentActivity":
		if !v.AgentPath.IsSet() {
			return nil, DecodeError{Field: "agentPath", Reason: "missing required field for type subAgentActivity"}
		}
		if !v.AgentThreadID.IsSet() {
			return nil, DecodeError{Field: "agentThreadId", Reason: "missing required field for type subAgentActivity"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type subAgentActivity"}
		}
		if !v.Kind.IsSet() {
			return nil, DecodeError{Field: "kind", Reason: "missing required field for type subAgentActivity"}
		}
	case "webSearch":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type webSearch"}
		}
		if !v.Query.IsSet() {
			return nil, DecodeError{Field: "query", Reason: "missing required field for type webSearch"}
		}
	case "imageView":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type imageView"}
		}
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type imageView"}
		}
	case "sleep":
		if len(v.DurationMs) == 0 {
			return nil, DecodeError{Field: "durationMs", Reason: "missing required field for type sleep"}
		}
		if bytes.Equal(v.DurationMs, []byte("null")) {
			return nil, DecodeError{Field: "durationMs", Reason: "cannot be null"}
		}
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type sleep"}
		}
	case "imageGeneration":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type imageGeneration"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type imageGeneration"}
		}
		taggedAnyOfVariant15Matches := (w.FileID.IsSet() && marshalsNonNull(w.FileID)) || (len(w.Result) > 0 && marshalsNonNull(w.Result))
		if !taggedAnyOfVariant15Matches {
			return nil, DecodeError{Field: "", Reason: "does not match any anyOf variant for type imageGeneration"}
		}
	case "workflow":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type workflow"}
		}
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type workflow"}
		}
		if !v.RunID.IsSet() {
			return nil, DecodeError{Field: "runId", Reason: "missing required field for type workflow"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type workflow"}
		}
	case "enteredReviewMode":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type enteredReviewMode"}
		}
		if !v.Review.IsSet() {
			return nil, DecodeError{Field: "review", Reason: "missing required field for type enteredReviewMode"}
		}
	case "exitedReviewMode":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type exitedReviewMode"}
		}
		if !v.Review.IsSet() {
			return nil, DecodeError{Field: "review", Reason: "missing required field for type exitedReviewMode"}
		}
	case "contextCompaction":
		if !v.ID.IsSet() {
			return nil, DecodeError{Field: "id", Reason: "missing required field for type contextCompaction"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	return json.Marshal(w)
}

func (v *ThreadItem) UnmarshalJSON(data []byte) error {
	type wire ThreadItem
	var s objectScan
	known, err := s.begin(data, &threadItemSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "userMessage":
		if !s.has("content") {
			return DecodeError{Field: "content", Reason: "missing required field for type userMessage"}
		}
		if s.isNull("content") {
			return DecodeError{Field: "content", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type userMessage"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
	case "hookPrompt":
		if !s.has("fragments") {
			return DecodeError{Field: "fragments", Reason: "missing required field for type hookPrompt"}
		}
		if s.isNull("fragments") {
			return DecodeError{Field: "fragments", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type hookPrompt"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
	case "agentMessage":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type agentMessage"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type agentMessage"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "functionCallOutput":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type functionCallOutput"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type functionCallOutput"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("output") {
			return DecodeError{Field: "output", Reason: "missing required field for type functionCallOutput"}
		}
		if s.isNull("output") {
			return DecodeError{Field: "output", Reason: "cannot be null"}
		}
	case "plan":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type plan"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type plan"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "reasoning":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type reasoning"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
	case "commandExecution":
		if !s.has("command") {
			return DecodeError{Field: "command", Reason: "missing required field for type commandExecution"}
		}
		if s.isNull("command") {
			return DecodeError{Field: "command", Reason: "cannot be null"}
		}
		if !s.has("commandActions") {
			return DecodeError{Field: "commandActions", Reason: "missing required field for type commandExecution"}
		}
		if s.isNull("commandActions") {
			return DecodeError{Field: "commandActions", Reason: "cannot be null"}
		}
		if !s.has("cwd") {
			return DecodeError{Field: "cwd", Reason: "missing required field for type commandExecution"}
		}
		if s.isNull("cwd") {
			return DecodeError{Field: "cwd", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type commandExecution"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type commandExecution"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "fileChange":
		if !s.has("changes") {
			return DecodeError{Field: "changes", Reason: "missing required field for type fileChange"}
		}
		if s.isNull("changes") {
			return DecodeError{Field: "changes", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type fileChange"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type fileChange"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "mcpToolCall":
		if !s.has("arguments") {
			return DecodeError{Field: "arguments", Reason: "missing required field for type mcpToolCall"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type mcpToolCall"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("server") {
			return DecodeError{Field: "server", Reason: "missing required field for type mcpToolCall"}
		}
		if s.isNull("server") {
			return DecodeError{Field: "server", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type mcpToolCall"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
		if !s.has("tool") {
			return DecodeError{Field: "tool", Reason: "missing required field for type mcpToolCall"}
		}
		if s.isNull("tool") {
			return DecodeError{Field: "tool", Reason: "cannot be null"}
		}
	case "dynamicToolCall":
		if !s.has("arguments") {
			return DecodeError{Field: "arguments", Reason: "missing required field for type dynamicToolCall"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type dynamicToolCall"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type dynamicToolCall"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
		if !s.has("tool") {
			return DecodeError{Field: "tool", Reason: "missing required field for type dynamicToolCall"}
		}
		if s.isNull("tool") {
			return DecodeError{Field: "tool", Reason: "cannot be null"}
		}
	case "collabAgentToolCall":
		if !s.has("agentsStates") {
			return DecodeError{Field: "agentsStates", Reason: "missing required field for type collabAgentToolCall"}
		}
		if s.isNull("agentsStates") {
			return DecodeError{Field: "agentsStates", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type collabAgentToolCall"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("receiverThreadIds") {
			return DecodeError{Field: "receiverThreadIds", Reason: "missing required field for type collabAgentToolCall"}
		}
		if s.isNull("receiverThreadIds") {
			return DecodeError{Field: "receiverThreadIds", Reason: "cannot be null"}
		}
		if !s.has("senderThreadId") {
			return DecodeError{Field: "senderThreadId", Reason: "missing required field for type collabAgentToolCall"}
		}
		if s.isNull("senderThreadId") {
			return DecodeError{Field: "senderThreadId", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type collabAgentToolCall"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
		if !s.has("tool") {
			return DecodeError{Field: "tool", Reason: "missing required field for type collabAgentToolCall"}
		}
		if s.isNull("tool") {
			return DecodeError{Field: "tool", Reason: "cannot be null"}
		}
	case "subAgentActivity":
		if !s.has("agentPath") {
			return DecodeError{Field: "agentPath", Reason: "missing required field for type subAgentActivity"}
		}
		if s.isNull("agentPath") {
			return DecodeError{Field: "agentPath", Reason: "cannot be null"}
		}
		if !s.has("agentThreadId") {
			return DecodeError{Field: "agentThreadId", Reason: "missing required field for type subAgentActivity"}
		}
		if s.isNull("agentThreadId") {
			return DecodeError{Field: "agentThreadId", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type subAgentActivity"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("kind") {
			return DecodeError{Field: "kind", Reason: "missing required field for type subAgentActivity"}
		}
		if s.isNull("kind") {
			return DecodeError{Field: "kind", Reason: "cannot be null"}
		}
	case "webSearch":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type webSearch"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("query") {
			return DecodeError{Field: "query", Reason: "missing required field for type webSearch"}
		}
		if s.isNull("query") {
			return DecodeError{Field: "query", Reason: "cannot be null"}
		}
	case "imageView":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type imageView"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type imageView"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "sleep":
		if !s.has("durationMs") {
			return DecodeError{Field: "durationMs", Reason: "missing required field for type sleep"}
		}
		if s.isNull("durationMs") {
			return DecodeError{Field: "durationMs", Reason: "cannot be null"}
		}
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type sleep"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
	case "imageGeneration":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type imageGeneration"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type imageGeneration"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
		taggedAnyOfVariant15Matches := (v.FileID.IsSet()) || (s.nonNull("result"))
		if !taggedAnyOfVariant15Matches {
			return DecodeError{Field: "", Reason: "does not match any anyOf variant for type imageGeneration"}
		}
	case "workflow":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type workflow"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type workflow"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("runId") {
			return DecodeError{Field: "runId", Reason: "missing required field for type workflow"}
		}
		if s.isNull("runId") {
			return DecodeError{Field: "runId", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type workflow"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
	case "enteredReviewMode":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type enteredReviewMode"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("review") {
			return DecodeError{Field: "review", Reason: "missing required field for type enteredReviewMode"}
		}
		if s.isNull("review") {
			return DecodeError{Field: "review", Reason: "cannot be null"}
		}
	case "exitedReviewMode":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type exitedReviewMode"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
		if !s.has("review") {
			return DecodeError{Field: "review", Reason: "missing required field for type exitedReviewMode"}
		}
		if s.isNull("review") {
			return DecodeError{Field: "review", Reason: "cannot be null"}
		}
	case "contextCompaction":
		if !s.has("id") {
			return DecodeError{Field: "id", Reason: "missing required field for type contextCompaction"}
		}
		if s.isNull("id") {
			return DecodeError{Field: "id", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadItemEntry struct {
	CompletedAtMs Optional[int64] `json:"completedAtMs,omitzero"`
	Item          ThreadItem      `json:"item"`
	StartedAtMs   Optional[int64] `json:"startedAtMs,omitzero"`
	TurnID        string          `json:"turnId"`
}

var threadItemEntrySpec = objectSpec{
	fields: []objectField{
		{name: "completedAtMs"},
		{name: "item", flags: fieldRequired | fieldRejectNull},
		{name: "startedAtMs"},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadItemEntry) MarshalJSON() ([]byte, error) {
	type wire ThreadItemEntry
	return json.Marshal(wire(v))
}

func (v *ThreadItemEntry) UnmarshalJSON(data []byte) error {
	type wire ThreadItemEntry
	return decodeObject(data, &threadItemEntrySpec, (*wire)(v))
}

type ThreadItemsListAnchor struct {
	ItemID    OptionalNonNull[string] `json:"itemId,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var threadItemsListAnchorSpec = objectSpec{
	fields: []objectField{
		{name: "itemId", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"item"},
}

func (v ThreadItemsListAnchor) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "item":
		if !v.ItemID.IsSet() {
			return nil, DecodeError{Field: "itemId", Reason: "missing required field for type item"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadItemsListAnchor
	return json.Marshal(wire(v))
}

func (v *ThreadItemsListAnchor) UnmarshalJSON(data []byte) error {
	type wire ThreadItemsListAnchor
	var s objectScan
	known, err := s.begin(data, &threadItemsListAnchorSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "item":
		if !s.has("itemId") {
			return DecodeError{Field: "itemId", Reason: "missing required field for type item"}
		}
		if s.isNull("itemId") {
			return DecodeError{Field: "itemId", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadItemsListCursor struct {
	stringValue           *string
	threadItemsListAnchor *ThreadItemsListAnchor
}

func NewThreadItemsListCursorString(value string) ThreadItemsListCursor {
	return ThreadItemsListCursor{stringValue: &value}
}

func NewThreadItemsListCursorThreadItemsListAnchor(value ThreadItemsListAnchor) ThreadItemsListCursor {
	return ThreadItemsListCursor{threadItemsListAnchor: &value}
}

func (v ThreadItemsListCursor) StringValue() (string, bool) {
	if v.stringValue == nil {
		return "", false
	}
	return *v.stringValue, true
}

func (v ThreadItemsListCursor) ThreadItemsListAnchor() (ThreadItemsListAnchor, bool) {
	if v.threadItemsListAnchor == nil {
		return ThreadItemsListAnchor{}, false
	}
	return *v.threadItemsListAnchor, true
}

func (v ThreadItemsListCursor) MarshalJSON() ([]byte, error) {
	switch {
	case v.stringValue != nil && v.threadItemsListAnchor == nil:
		return json.Marshal(v.stringValue)
	case v.threadItemsListAnchor != nil && v.stringValue == nil:
		return json.Marshal(v.threadItemsListAnchor)
	default:
		return nil, DecodeError{Field: "", Reason: "empty or ambiguous ThreadItemsListCursor"}
	}
}

func (v *ThreadItemsListCursor) UnmarshalJSON(data []byte) error {
	trimmed := bytes.TrimSpace(data)
	if len(trimmed) == 0 {
		return DecodeError{Field: "", Reason: "empty ThreadItemsListCursor"}
	}
	if bytes.Equal(trimmed, []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	switch trimmed[0] {
	case '"':
		var value string
		if err := json.Unmarshal(trimmed, &value); err != nil {
			return err
		}
		*v = NewThreadItemsListCursorString(value)
	case '{':
		var value ThreadItemsListAnchor
		if err := json.Unmarshal(trimmed, &value); err != nil {
			return err
		}
		*v = NewThreadItemsListCursorThreadItemsListAnchor(value)
	default:
		return DecodeError{Field: "", Reason: "no matching union variant"}
	}
	return nil
}

type ThreadItemsListParams struct {
	Cursor        Optional[ThreadItemsListCursor] `json:"cursor,omitzero"`
	Limit         Optional[uint32]                `json:"limit,omitzero"`
	SortDirection Optional[SortDirection]         `json:"sortDirection,omitzero"`
	ThreadID      string                          `json:"threadId"`
	TurnID        Optional[string]                `json:"turnId,omitzero"`
}

var threadItemsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "sortDirection"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId"},
	},
}

func (v ThreadItemsListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadItemsListParams
	return json.Marshal(wire(v))
}

func (v *ThreadItemsListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadItemsListParams
	var s objectScan
	if _, err := s.begin(data, &threadItemsListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadItemsListResponse struct {
	BackwardsCursor Optional[string]  `json:"backwardsCursor,omitzero"`
	Data            []ThreadItemEntry `json:"data"`
	NextCursor      Optional[string]  `json:"nextCursor,omitzero"`
}

var threadItemsListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "backwardsCursor"},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadItemsListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadItemsListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadItemsListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadItemsListResponse
	return decodeObject(data, &threadItemsListResponseSpec, (*wire)(v))
}

type ThreadItemsReadParams struct {
	ItemIDs  []string `json:"itemIds"`
	ThreadID string   `json:"threadId"`
	TurnID   string   `json:"turnId"`
}

var threadItemsReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "itemIds", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadItemsReadParams) MarshalJSON() ([]byte, error) {
	type wire ThreadItemsReadParams
	return json.Marshal(wire(v))
}

func (v *ThreadItemsReadParams) UnmarshalJSON(data []byte) error {
	type wire ThreadItemsReadParams
	return decodeObject(data, &threadItemsReadParamsSpec, (*wire)(v))
}

type ThreadItemsReadResponse struct {
	Data []ThreadItemEntry `json:"data"`
}

var threadItemsReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadItemsReadResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadItemsReadResponse
	return json.Marshal(wire(v))
}

func (v *ThreadItemsReadResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadItemsReadResponse
	return decodeObject(data, &threadItemsReadResponseSpec, (*wire)(v))
}

type ThreadListCwdFilter = json.RawMessage

type ThreadListParams struct {
	AncestorThreadID   Optional[string]              `json:"ancestorThreadId,omitzero"`
	Archived           Optional[bool]                `json:"archived,omitzero"`
	AttachedToThreadID Optional[string]              `json:"attachedToThreadId,omitzero"`
	Cursor             Optional[string]              `json:"cursor,omitzero"`
	Cwd                Optional[ThreadListCwdFilter] `json:"cwd,omitzero"`
	ExcludedThreadIDs  Optional[[]string]            `json:"excludedThreadIds,omitzero"`
	Limit              Optional[uint32]              `json:"limit,omitzero"`
	ModelProviders     Optional[[]string]            `json:"modelProviders,omitzero"`
	Originators        Optional[[]string]            `json:"originators,omitzero"`
	ParentThreadID     Optional[string]              `json:"parentThreadId,omitzero"`
	ProjectID          Optional[string]              `json:"projectId,omitzero"`
	SearchTerm         Optional[string]              `json:"searchTerm,omitzero"`
	SectionID          Optional[string]              `json:"sectionId,omitzero"`
	SortDirection      Optional[SortDirection]       `json:"sortDirection,omitzero"`
	SortKey            Optional[ThreadSortKey]       `json:"sortKey,omitzero"`
	SourceKinds        Optional[[]ThreadSourceKind]  `json:"sourceKinds,omitzero"`
	UseStateDbOnly     OptionalNonNull[bool]         `json:"useStateDbOnly,omitzero"`
}

var threadListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "ancestorThreadId"},
		{name: "archived"},
		{name: "attachedToThreadId"},
		{name: "cursor"},
		{name: "cwd"},
		{name: "excludedThreadIds"},
		{name: "limit"},
		{name: "modelProviders"},
		{name: "originators"},
		{name: "parentThreadId"},
		{name: "projectId"},
		{name: "searchTerm"},
		{name: "sectionId"},
		{name: "sortDirection"},
		{name: "sortKey"},
		{name: "sourceKinds"},
		{name: "useStateDbOnly", flags: fieldNonNullOptional, defaultJSON: "false"},
	},
}

func (v ThreadListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadListParams
	w := wire(v)
	w.UseStateDbOnly = nonNullWithoutFalse(w.UseStateDbOnly)
	return json.Marshal(w)
}

func (v *ThreadListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadListParams
	var s objectScan
	if _, err := s.begin(data, &threadListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadListResponse struct {
	BackwardsCursor Optional[string]                     `json:"backwardsCursor,omitzero"`
	Data            []Thread                             `json:"data"`
	NextCursor      Optional[string]                     `json:"nextCursor,omitzero"`
	ReadStates      Optional[map[string]ThreadReadState] `json:"readStates,omitzero"`
}

var threadListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "backwardsCursor"},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
		{name: "readStates"},
	},
}

func (v ThreadListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadListResponse
	return decodeObject(data, &threadListResponseSpec, (*wire)(v))
}

type ThreadLoadedListParams struct {
	Cursor Optional[string] `json:"cursor,omitzero"`
	Limit  Optional[uint32] `json:"limit,omitzero"`
}

var threadLoadedListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
	},
}

func (v ThreadLoadedListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadLoadedListParams
	return json.Marshal(wire(v))
}

func (v *ThreadLoadedListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadLoadedListParams
	var s objectScan
	if _, err := s.begin(data, &threadLoadedListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadLoadedListResponse struct {
	Data       []string         `json:"data"`
	NextCursor Optional[string] `json:"nextCursor,omitzero"`
}

var threadLoadedListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadLoadedListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadLoadedListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadLoadedListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadLoadedListResponse
	return decodeObject(data, &threadLoadedListResponseSpec, (*wire)(v))
}

type ThreadMemoryMode string

const (
	ThreadMemoryModeEnabled  ThreadMemoryMode = "enabled"
	ThreadMemoryModeDisabled ThreadMemoryMode = "disabled"
)

type ThreadMemoryModeSetParams struct {
	Mode     ThreadMemoryMode `json:"mode"`
	ThreadID string           `json:"threadId"`
}

var threadMemoryModeSetParamsSpec = objectSpec{
	fields: []objectField{
		{name: "mode", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadMemoryModeSetParams) MarshalJSON() ([]byte, error) {
	type wire ThreadMemoryModeSetParams
	return json.Marshal(wire(v))
}

func (v *ThreadMemoryModeSetParams) UnmarshalJSON(data []byte) error {
	type wire ThreadMemoryModeSetParams
	return decodeObject(data, &threadMemoryModeSetParamsSpec, (*wire)(v))
}

type ThreadMemoryModeSetResponse map[string]json.RawMessage

func (v ThreadMemoryModeSetResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadMemoryModeSetResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadMemoryModeSetResponse(raw)
	return nil
}

type ThreadMemoryReadParams struct {
	ThreadID string `json:"threadId"`
}

var threadMemoryReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadMemoryReadParams) MarshalJSON() ([]byte, error) {
	type wire ThreadMemoryReadParams
	return json.Marshal(wire(v))
}

func (v *ThreadMemoryReadParams) UnmarshalJSON(data []byte) error {
	type wire ThreadMemoryReadParams
	return decodeObject(data, &threadMemoryReadParamsSpec, (*wire)(v))
}

type ThreadMemoryReadResponse struct {
	Summary Optional[string] `json:"summary,omitzero"`
}

var threadMemoryReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "summary"},
	},
}

func (v ThreadMemoryReadResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadMemoryReadResponse
	return json.Marshal(wire(v))
}

func (v *ThreadMemoryReadResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadMemoryReadResponse
	return decodeObject(data, &threadMemoryReadResponseSpec, (*wire)(v))
}

type ThreadMetadataGitInfoUpdateParams struct {
	Branch    Optional[string] `json:"branch,omitzero"`
	OriginURL Optional[string] `json:"originUrl,omitzero"`
	Sha       Optional[string] `json:"sha,omitzero"`
}

var threadMetadataGitInfoUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "branch"},
		{name: "originUrl"},
		{name: "sha"},
	},
}

func (v ThreadMetadataGitInfoUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadMetadataGitInfoUpdateParams
	return json.Marshal(wire(v))
}

func (v *ThreadMetadataGitInfoUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadMetadataGitInfoUpdateParams
	return decodeObject(data, &threadMetadataGitInfoUpdateParamsSpec, (*wire)(v))
}

type ThreadMetadataUpdateParams struct {
	DaybreakEnabled Optional[bool]                              `json:"daybreakEnabled,omitzero"`
	GitInfo         Optional[ThreadMetadataGitInfoUpdateParams] `json:"gitInfo,omitzero"`
	ProjectID       Optional[string]                            `json:"projectId,omitzero"`
	ThreadID        string                                      `json:"threadId"`
}

var threadMetadataUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "daybreakEnabled"},
		{name: "gitInfo"},
		{name: "projectId"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadMetadataUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadMetadataUpdateParams
	return json.Marshal(wire(v))
}

func (v *ThreadMetadataUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadMetadataUpdateParams
	return decodeObject(data, &threadMetadataUpdateParamsSpec, (*wire)(v))
}

type ThreadMetadataUpdateResponse struct {
	Thread Thread `json:"thread"`
}

var threadMetadataUpdateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadMetadataUpdateResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadMetadataUpdateResponse
	return json.Marshal(wire(v))
}

func (v *ThreadMetadataUpdateResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadMetadataUpdateResponse
	return decodeObject(data, &threadMetadataUpdateResponseSpec, (*wire)(v))
}

type ThreadNameUpdatedNotification struct {
	ThreadID   string           `json:"threadId"`
	ThreadName Optional[string] `json:"threadName,omitzero"`
}

var threadNameUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "threadName"},
	},
}

func (v ThreadNameUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadNameUpdatedNotification
	w := wire(v)
	w.ThreadName = w.ThreadName.withoutNull()
	return json.Marshal(w)
}

func (v *ThreadNameUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadNameUpdatedNotification
	return decodeObject(data, &threadNameUpdatedNotificationSpec, (*wire)(v))
}

type ThreadPredictionRequestParams struct {
	SourceTurnID string `json:"sourceTurnId"`
	ThreadID     string `json:"threadId"`
}

var threadPredictionRequestParamsSpec = objectSpec{
	fields: []objectField{
		{name: "sourceTurnId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadPredictionRequestParams) MarshalJSON() ([]byte, error) {
	type wire ThreadPredictionRequestParams
	return json.Marshal(wire(v))
}

func (v *ThreadPredictionRequestParams) UnmarshalJSON(data []byte) error {
	type wire ThreadPredictionRequestParams
	return decodeObject(data, &threadPredictionRequestParamsSpec, (*wire)(v))
}

type ThreadPredictionRequestResponse map[string]json.RawMessage

func (v ThreadPredictionRequestResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadPredictionRequestResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadPredictionRequestResponse(raw)
	return nil
}

type ThreadPredictionResult struct {
	Text      Optional[string] `json:"text,omitzero"`
	TypeValue string           `json:"type"`
	RawJSON   json.RawMessage  `json:"-"`
}

var threadPredictionResultSpec = objectSpec{
	fields: []objectField{
		{name: "text"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"completed", "failed"},
}

func (v ThreadPredictionResult) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "completed":
	case "failed":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadPredictionResult
	return json.Marshal(wire(v))
}

func (v *ThreadPredictionResult) UnmarshalJSON(data []byte) error {
	type wire ThreadPredictionResult
	var s objectScan
	known, err := s.begin(data, &threadPredictionResultSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "completed":
	case "failed":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadPredictionUpdatedNotification struct {
	Result       ThreadPredictionResult `json:"result"`
	SourceTurnID string                 `json:"sourceTurnId"`
	ThreadID     string                 `json:"threadId"`
}

var threadPredictionUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "result", flags: fieldRequired | fieldRejectNull},
		{name: "sourceTurnId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadPredictionUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadPredictionUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadPredictionUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadPredictionUpdatedNotification
	return decodeObject(data, &threadPredictionUpdatedNotificationSpec, (*wire)(v))
}

type ThreadProjectUpdatedNotification struct {
	ProjectID Optional[string] `json:"projectId,omitzero"`
	ThreadID  string           `json:"threadId"`
}

var threadProjectUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "projectId", flags: fieldRequired},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadProjectUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadProjectUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadProjectUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadProjectUpdatedNotification
	return decodeObject(data, &threadProjectUpdatedNotificationSpec, (*wire)(v))
}

type ThreadQueueAddParams struct {
	ClientUserMessageID string      `json:"clientUserMessageId"`
	Input               []UserInput `json:"input"`
	ThreadID            string      `json:"threadId"`
}

var threadQueueAddParamsSpec = objectSpec{
	fields: []objectField{
		{name: "clientUserMessageId", flags: fieldRequired | fieldRejectNull},
		{name: "input", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueAddParams) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueAddParams
	return json.Marshal(wire(v))
}

func (v *ThreadQueueAddParams) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueAddParams
	return decodeObject(data, &threadQueueAddParamsSpec, (*wire)(v))
}

type ThreadQueueAddResponse struct {
	QueuedSubmission QueuedSubmission `json:"queuedSubmission"`
}

var threadQueueAddResponseSpec = objectSpec{
	fields: []objectField{
		{name: "queuedSubmission", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueAddResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueAddResponse
	return json.Marshal(wire(v))
}

func (v *ThreadQueueAddResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueAddResponse
	return decodeObject(data, &threadQueueAddResponseSpec, (*wire)(v))
}

type ThreadQueueChangedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadQueueChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueChangedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueChangedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadQueueChangedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueChangedNotification
	return decodeObject(data, &threadQueueChangedNotificationSpec, (*wire)(v))
}

type ThreadQueueDeleteParams struct {
	QueuedSubmissionID string `json:"queuedSubmissionId"`
	ThreadID           string `json:"threadId"`
}

var threadQueueDeleteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "queuedSubmissionId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueDeleteParams) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueDeleteParams
	return json.Marshal(wire(v))
}

func (v *ThreadQueueDeleteParams) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueDeleteParams
	return decodeObject(data, &threadQueueDeleteParamsSpec, (*wire)(v))
}

type ThreadQueueDeleteResponse struct {
	Deleted bool `json:"deleted"`
}

var threadQueueDeleteResponseSpec = objectSpec{
	fields: []objectField{
		{name: "deleted", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueDeleteResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueDeleteResponse
	return json.Marshal(wire(v))
}

func (v *ThreadQueueDeleteResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueDeleteResponse
	return decodeObject(data, &threadQueueDeleteResponseSpec, (*wire)(v))
}

type ThreadQueueListParams struct {
	Cursor   Optional[string] `json:"cursor,omitzero"`
	Limit    Optional[uint32] `json:"limit,omitzero"`
	ThreadID string           `json:"threadId"`
}

var threadQueueListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueListParams
	return json.Marshal(wire(v))
}

func (v *ThreadQueueListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueListParams
	var s objectScan
	if _, err := s.begin(data, &threadQueueListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadQueueListResponse struct {
	Data       []QueuedSubmission `json:"data"`
	NextCursor Optional[string]   `json:"nextCursor,omitzero"`
}

var threadQueueListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadQueueListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadQueueListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueListResponse
	return decodeObject(data, &threadQueueListResponseSpec, (*wire)(v))
}

type ThreadQueueReorderParams struct {
	QueuedSubmissionIDs []string `json:"queuedSubmissionIds"`
	ThreadID            string   `json:"threadId"`
}

var threadQueueReorderParamsSpec = objectSpec{
	fields: []objectField{
		{name: "queuedSubmissionIds", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueReorderParams) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueReorderParams
	return json.Marshal(wire(v))
}

func (v *ThreadQueueReorderParams) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueReorderParams
	return decodeObject(data, &threadQueueReorderParamsSpec, (*wire)(v))
}

type ThreadQueueReorderResponse map[string]json.RawMessage

func (v ThreadQueueReorderResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadQueueReorderResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadQueueReorderResponse(raw)
	return nil
}

type ThreadQueueStartParams struct {
	QueuedSubmissionID Optional[string] `json:"queuedSubmissionId,omitzero"`
	ThreadID           string           `json:"threadId"`
}

var threadQueueStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "queuedSubmissionId"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueStartParams) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueStartParams
	return json.Marshal(wire(v))
}

func (v *ThreadQueueStartParams) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueStartParams
	return decodeObject(data, &threadQueueStartParamsSpec, (*wire)(v))
}

type ThreadQueueStartResponse struct {
	Turn Turn `json:"turn"`
}

var threadQueueStartResponseSpec = objectSpec{
	fields: []objectField{
		{name: "turn", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueStartResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueStartResponse
	return json.Marshal(wire(v))
}

func (v *ThreadQueueStartResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueStartResponse
	return decodeObject(data, &threadQueueStartResponseSpec, (*wire)(v))
}

type ThreadQueueUpdateParams struct {
	Input              []UserInput `json:"input"`
	QueuedSubmissionID string      `json:"queuedSubmissionId"`
	ThreadID           string      `json:"threadId"`
}

var threadQueueUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "input", flags: fieldRequired | fieldRejectNull},
		{name: "queuedSubmissionId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueUpdateParams
	return json.Marshal(wire(v))
}

func (v *ThreadQueueUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueUpdateParams
	return decodeObject(data, &threadQueueUpdateParamsSpec, (*wire)(v))
}

type ThreadQueueUpdateResponse struct {
	QueuedSubmission QueuedSubmission `json:"queuedSubmission"`
}

var threadQueueUpdateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "queuedSubmission", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadQueueUpdateResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadQueueUpdateResponse
	return json.Marshal(wire(v))
}

func (v *ThreadQueueUpdateResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadQueueUpdateResponse
	return decodeObject(data, &threadQueueUpdateResponseSpec, (*wire)(v))
}

type ThreadReadParams struct {
	IncludeTurns OptionalNonNull[bool] `json:"includeTurns,omitzero"`
	ThreadID     string                `json:"threadId"`
}

var threadReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "includeTurns", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadReadParams) MarshalJSON() ([]byte, error) {
	type wire ThreadReadParams
	w := wire(v)
	w.IncludeTurns = nonNullWithoutFalse(w.IncludeTurns)
	return json.Marshal(w)
}

func (v *ThreadReadParams) UnmarshalJSON(data []byte) error {
	type wire ThreadReadParams
	return decodeObject(data, &threadReadParamsSpec, (*wire)(v))
}

type ThreadReadResponse struct {
	ReadState Optional[ThreadReadState] `json:"readState,omitzero"`
	Thread    Thread                    `json:"thread"`
}

var threadReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "readState"},
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadReadResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadReadResponse
	return json.Marshal(wire(v))
}

func (v *ThreadReadResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadReadResponse
	return decodeObject(data, &threadReadResponseSpec, (*wire)(v))
}

type ThreadReadState struct {
	FirstUnread Optional[ThreadUnreadPosition] `json:"firstUnread,omitzero"`
	Revision    string                         `json:"revision"`
}

var threadReadStateSpec = objectSpec{
	fields: []objectField{
		{name: "firstUnread"},
		{name: "revision", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadReadState) MarshalJSON() ([]byte, error) {
	type wire ThreadReadState
	return json.Marshal(wire(v))
}

func (v *ThreadReadState) UnmarshalJSON(data []byte) error {
	type wire ThreadReadState
	return decodeObject(data, &threadReadStateSpec, (*wire)(v))
}

type ThreadReadStateChangedNotification struct {
	ReadState ThreadReadState `json:"readState"`
	ThreadID  string          `json:"threadId"`
}

var threadReadStateChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "readState", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadReadStateChangedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadReadStateChangedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadReadStateChangedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadReadStateChangedNotification
	return decodeObject(data, &threadReadStateChangedNotificationSpec, (*wire)(v))
}

type ThreadReadStateOperation struct {
	TypeValue string          `json:"type"`
	RawJSON   json.RawMessage `json:"-"`
}

var threadReadStateOperationSpec = objectSpec{
	fields: []objectField{
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  0,
	tags: []string{"read", "unread"},
}

func (v ThreadReadStateOperation) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "read":
	case "unread":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadReadStateOperation
	return json.Marshal(wire(v))
}

func (v *ThreadReadStateOperation) UnmarshalJSON(data []byte) error {
	type wire ThreadReadStateOperation
	var s objectScan
	known, err := s.begin(data, &threadReadStateOperationSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "read":
	case "unread":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadReadStateUpdateParams struct {
	ExpectedRevision string                   `json:"expectedRevision"`
	Operation        ThreadReadStateOperation `json:"operation"`
	ThreadID         string                   `json:"threadId"`
}

var threadReadStateUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "expectedRevision", flags: fieldRequired | fieldRejectNull},
		{name: "operation", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadReadStateUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadReadStateUpdateParams
	return json.Marshal(wire(v))
}

func (v *ThreadReadStateUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadReadStateUpdateParams
	return decodeObject(data, &threadReadStateUpdateParamsSpec, (*wire)(v))
}

type ThreadReadStateUpdateResponse struct {
	ReadState ThreadReadState `json:"readState"`
}

var threadReadStateUpdateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "readState", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadReadStateUpdateResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadReadStateUpdateResponse
	return json.Marshal(wire(v))
}

func (v *ThreadReadStateUpdateResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadReadStateUpdateResponse
	return decodeObject(data, &threadReadStateUpdateResponseSpec, (*wire)(v))
}

type ThreadRealtimeAppendAudioParams struct {
	Audio    ThreadRealtimeAudioChunk `json:"audio"`
	ThreadID string                   `json:"threadId"`
}

var threadRealtimeAppendAudioParamsSpec = objectSpec{
	fields: []objectField{
		{name: "audio", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeAppendAudioParams) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeAppendAudioParams
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeAppendAudioParams) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeAppendAudioParams
	return decodeObject(data, &threadRealtimeAppendAudioParamsSpec, (*wire)(v))
}

type ThreadRealtimeAppendAudioResponse map[string]json.RawMessage

func (v ThreadRealtimeAppendAudioResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadRealtimeAppendAudioResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadRealtimeAppendAudioResponse(raw)
	return nil
}

type ThreadRealtimeAppendSpeechParams struct {
	Text     string `json:"text"`
	ThreadID string `json:"threadId"`
}

var threadRealtimeAppendSpeechParamsSpec = objectSpec{
	fields: []objectField{
		{name: "text", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeAppendSpeechParams) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeAppendSpeechParams
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeAppendSpeechParams) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeAppendSpeechParams
	return decodeObject(data, &threadRealtimeAppendSpeechParamsSpec, (*wire)(v))
}

type ThreadRealtimeAppendSpeechResponse map[string]json.RawMessage

func (v ThreadRealtimeAppendSpeechResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadRealtimeAppendSpeechResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadRealtimeAppendSpeechResponse(raw)
	return nil
}

type ThreadRealtimeAppendTextParams struct {
	Role     OptionalNonNull[ConversationTextRole] `json:"role,omitzero"`
	Text     string                                `json:"text"`
	ThreadID string                                `json:"threadId"`
}

var threadRealtimeAppendTextParamsSpec = objectSpec{
	fields: []objectField{
		{name: "role", flags: fieldNonNullOptional, defaultJSON: "\"user\""},
		{name: "text", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeAppendTextParams) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeAppendTextParams
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeAppendTextParams) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeAppendTextParams
	return decodeObject(data, &threadRealtimeAppendTextParamsSpec, (*wire)(v))
}

type ThreadRealtimeAppendTextResponse map[string]json.RawMessage

func (v ThreadRealtimeAppendTextResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadRealtimeAppendTextResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadRealtimeAppendTextResponse(raw)
	return nil
}

type ThreadRealtimeAudioChunk struct {
	Data              string           `json:"data"`
	ItemID            Optional[string] `json:"itemId,omitzero"`
	NumChannels       uint16           `json:"numChannels"`
	SampleRate        uint32           `json:"sampleRate"`
	SamplesPerChannel Optional[uint32] `json:"samplesPerChannel,omitzero"`
}

var threadRealtimeAudioChunkSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "itemId"},
		{name: "numChannels", flags: fieldRequired | fieldRejectNull},
		{name: "sampleRate", flags: fieldRequired | fieldRejectNull},
		{name: "samplesPerChannel"},
	},
}

func (v ThreadRealtimeAudioChunk) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeAudioChunk
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeAudioChunk) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeAudioChunk
	var s objectScan
	if _, err := s.begin(data, &threadRealtimeAudioChunkSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.NumChannels < 0 {
		return DecodeError{Field: "numChannels", Reason: "below minimum 0"}
	}
	if v.SampleRate < 0 {
		return DecodeError{Field: "sampleRate", Reason: "below minimum 0"}
	}
	if valueSamplesPerChannel, ok := v.SamplesPerChannel.Value(); ok {
		if valueSamplesPerChannel < 0 {
			return DecodeError{Field: "samplesPerChannel", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadRealtimeBemItemPresentation struct {
	Index     OptionalNonNull[uint32] `json:"index,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var threadRealtimeBemItemPresentationSpec = objectSpec{
	fields: []objectField{
		{name: "index", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"wholeItem", "inlineMarkdown", "inlineVisualization"},
}

func (v ThreadRealtimeBemItemPresentation) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "wholeItem":
	case "inlineMarkdown":
	case "inlineVisualization":
		if !v.Index.IsSet() {
			return nil, DecodeError{Field: "index", Reason: "missing required field for type inlineVisualization"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadRealtimeBemItemPresentation
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeBemItemPresentation) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeBemItemPresentation
	var s objectScan
	known, err := s.begin(data, &threadRealtimeBemItemPresentationSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueIndex, ok := v.Index.Value(); ok {
		if valueIndex < 0 {
			return DecodeError{Field: "index", Reason: "below minimum 0"}
		}
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "wholeItem":
	case "inlineMarkdown":
	case "inlineVisualization":
		if !s.has("index") {
			return DecodeError{Field: "index", Reason: "missing required field for type inlineVisualization"}
		}
		if s.isNull("index") {
			return DecodeError{Field: "index", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadRealtimeClosedNotification struct {
	Reason   Optional[string] `json:"reason,omitzero"`
	ThreadID string           `json:"threadId"`
}

var threadRealtimeClosedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "reason"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeClosedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeClosedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeClosedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeClosedNotification
	return decodeObject(data, &threadRealtimeClosedNotificationSpec, (*wire)(v))
}

type ThreadRealtimeErrorNotification struct {
	Message  string `json:"message"`
	ThreadID string `json:"threadId"`
}

var threadRealtimeErrorNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeErrorNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeErrorNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeErrorNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeErrorNotification
	return decodeObject(data, &threadRealtimeErrorNotificationSpec, (*wire)(v))
}

type ThreadRealtimeInitialItem struct {
	Role ConversationTextRole `json:"role"`
	Text string               `json:"text"`
}

var threadRealtimeInitialItemSpec = objectSpec{
	fields: []objectField{
		{name: "role", flags: fieldRequired | fieldRejectNull},
		{name: "text", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeInitialItem) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeInitialItem
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeInitialItem) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeInitialItem
	return decodeObject(data, &threadRealtimeInitialItemSpec, (*wire)(v))
}

type ThreadRealtimeItem struct {
	ID                string                                             `json:"id"`
	ItemID            OptionalNonNull[string]                            `json:"item_id,omitzero"`
	Outcome           OptionalNonNull[ThreadRealtimeSessionOutcome]      `json:"outcome,omitzero"`
	Presentation      OptionalNonNull[ThreadRealtimeBemItemPresentation] `json:"presentation,omitzero"`
	RealtimeSessionID string                                             `json:"realtimeSessionId"`
	Role              OptionalNonNull[ThreadRealtimeTranscriptRole]      `json:"role,omitzero"`
	Text              OptionalNonNull[string]                            `json:"text,omitzero"`
	TurnID            OptionalNonNull[string]                            `json:"turn_id,omitzero"`
	TypeValue         string                                             `json:"type"`
	RawJSON           json.RawMessage                                    `json:"-"`
}

var threadRealtimeItemSpec = objectSpec{
	fields: []objectField{
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "item_id", flags: fieldNonNullOptional},
		{name: "outcome", flags: fieldNonNullOptional},
		{name: "presentation", flags: fieldNonNullOptional},
		{name: "realtimeSessionId", flags: fieldRequired | fieldRejectNull},
		{name: "role", flags: fieldNonNullOptional},
		{name: "text", flags: fieldNonNullOptional},
		{name: "turn_id", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  8,
	tags: []string{"realtimeSessionStarted", "transcriptSegment", "bemItemPromoted", "realtimeSessionClosed"},
}

func (v ThreadRealtimeItem) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "realtimeSessionStarted":
	case "transcriptSegment":
		if !v.Role.IsSet() {
			return nil, DecodeError{Field: "role", Reason: "missing required field for type transcriptSegment"}
		}
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type transcriptSegment"}
		}
	case "bemItemPromoted":
		if !v.ItemID.IsSet() {
			return nil, DecodeError{Field: "item_id", Reason: "missing required field for type bemItemPromoted"}
		}
		if !v.Presentation.IsSet() {
			return nil, DecodeError{Field: "presentation", Reason: "missing required field for type bemItemPromoted"}
		}
		if !v.TurnID.IsSet() {
			return nil, DecodeError{Field: "turn_id", Reason: "missing required field for type bemItemPromoted"}
		}
	case "realtimeSessionClosed":
		if !v.Outcome.IsSet() {
			return nil, DecodeError{Field: "outcome", Reason: "missing required field for type realtimeSessionClosed"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadRealtimeItem
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeItem) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeItem
	var s objectScan
	known, err := s.begin(data, &threadRealtimeItemSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "realtimeSessionStarted":
	case "transcriptSegment":
		if !s.has("role") {
			return DecodeError{Field: "role", Reason: "missing required field for type transcriptSegment"}
		}
		if s.isNull("role") {
			return DecodeError{Field: "role", Reason: "cannot be null"}
		}
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type transcriptSegment"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "bemItemPromoted":
		if !s.has("item_id") {
			return DecodeError{Field: "item_id", Reason: "missing required field for type bemItemPromoted"}
		}
		if s.isNull("item_id") {
			return DecodeError{Field: "item_id", Reason: "cannot be null"}
		}
		if !s.has("presentation") {
			return DecodeError{Field: "presentation", Reason: "missing required field for type bemItemPromoted"}
		}
		if s.isNull("presentation") {
			return DecodeError{Field: "presentation", Reason: "cannot be null"}
		}
		if !s.has("turn_id") {
			return DecodeError{Field: "turn_id", Reason: "missing required field for type bemItemPromoted"}
		}
		if s.isNull("turn_id") {
			return DecodeError{Field: "turn_id", Reason: "cannot be null"}
		}
	case "realtimeSessionClosed":
		if !s.has("outcome") {
			return DecodeError{Field: "outcome", Reason: "missing required field for type realtimeSessionClosed"}
		}
		if s.isNull("outcome") {
			return DecodeError{Field: "outcome", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadRealtimeItemAddedNotification struct {
	Item     json.RawMessage `json:"item"`
	ThreadID string          `json:"threadId"`
}

var threadRealtimeItemAddedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "item", flags: fieldRequired},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeItemAddedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeItemAddedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeItemAddedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeItemAddedNotification
	return decodeObject(data, &threadRealtimeItemAddedNotificationSpec, (*wire)(v))
}

type ThreadRealtimeItemCompletedNotification struct {
	Item     ThreadRealtimeItem `json:"item"`
	ThreadID string             `json:"threadId"`
}

var threadRealtimeItemCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "item", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeItemCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeItemCompletedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeItemCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeItemCompletedNotification
	return decodeObject(data, &threadRealtimeItemCompletedNotificationSpec, (*wire)(v))
}

type ThreadRealtimeItemStartedNotification struct {
	Item     ThreadRealtimeItem `json:"item"`
	ThreadID string             `json:"threadId"`
}

var threadRealtimeItemStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "item", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeItemStartedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeItemStartedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeItemStartedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeItemStartedNotification
	return decodeObject(data, &threadRealtimeItemStartedNotificationSpec, (*wire)(v))
}

type ThreadRealtimeItemTranscriptDeltaNotification struct {
	Delta    string `json:"delta"`
	ItemID   string `json:"itemId"`
	ThreadID string `json:"threadId"`
}

var threadRealtimeItemTranscriptDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeItemTranscriptDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeItemTranscriptDeltaNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeItemTranscriptDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeItemTranscriptDeltaNotification
	return decodeObject(data, &threadRealtimeItemTranscriptDeltaNotificationSpec, (*wire)(v))
}

type ThreadRealtimeListVoicesParams map[string]json.RawMessage

func (v ThreadRealtimeListVoicesParams) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadRealtimeListVoicesParams) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadRealtimeListVoicesParams(raw)
	return nil
}

type ThreadRealtimeListVoicesResponse struct {
	Voices RealtimeVoicesList `json:"voices"`
}

var threadRealtimeListVoicesResponseSpec = objectSpec{
	fields: []objectField{
		{name: "voices", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeListVoicesResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeListVoicesResponse
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeListVoicesResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeListVoicesResponse
	return decodeObject(data, &threadRealtimeListVoicesResponseSpec, (*wire)(v))
}

type ThreadRealtimeOutputAudioDeltaNotification struct {
	Audio    ThreadRealtimeAudioChunk `json:"audio"`
	ThreadID string                   `json:"threadId"`
}

var threadRealtimeOutputAudioDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "audio", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeOutputAudioDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeOutputAudioDeltaNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeOutputAudioDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeOutputAudioDeltaNotification
	return decodeObject(data, &threadRealtimeOutputAudioDeltaNotificationSpec, (*wire)(v))
}

type ThreadRealtimeSdpNotification struct {
	Sdp      string `json:"sdp"`
	ThreadID string `json:"threadId"`
}

var threadRealtimeSdpNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "sdp", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeSdpNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeSdpNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeSdpNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeSdpNotification
	return decodeObject(data, &threadRealtimeSdpNotificationSpec, (*wire)(v))
}

type ThreadRealtimeSessionOutcome string

const (
	ThreadRealtimeSessionOutcomeEnded  ThreadRealtimeSessionOutcome = "ended"
	ThreadRealtimeSessionOutcomeFailed ThreadRealtimeSessionOutcome = "failed"
)

type ThreadRealtimeStartParams struct {
	BackendReasoningStatus              OptionalNonNull[bool]                  `json:"backendReasoningStatus,omitzero"`
	ClientManagedHandoffs               Optional[bool]                         `json:"clientManagedHandoffs,omitzero"`
	CodexResponseHandoffChannelPrefixes Optional[map[string][]string]          `json:"codexResponseHandoffChannelPrefixes,omitzero"`
	CodexResponseHandoffMode            Optional[CodexResponseHandoffMode]     `json:"codexResponseHandoffMode,omitzero"`
	CodexResponseItemPrefix             Optional[string]                       `json:"codexResponseItemPrefix,omitzero"`
	CodexResponsesAsItems               Optional[bool]                         `json:"codexResponsesAsItems,omitzero"`
	DelegationAckFiller                 Optional[bool]                         `json:"delegationAckFiller,omitzero"`
	FlushTranscriptTailOnSessionEnd     Optional[bool]                         `json:"flushTranscriptTailOnSessionEnd,omitzero"`
	IncludeStartupContext               Optional[bool]                         `json:"includeStartupContext,omitzero"`
	InitialItems                        Optional[[]ThreadRealtimeInitialItem]  `json:"initialItems,omitzero"`
	Model                               Optional[string]                       `json:"model,omitzero"`
	OutputModality                      RealtimeOutputModality                 `json:"outputModality"`
	Prompt                              Optional[string]                       `json:"prompt,omitzero"`
	RealtimeEndInstructions             Optional[string]                       `json:"realtimeEndInstructions,omitzero"`
	RealtimeSessionID                   Optional[string]                       `json:"realtimeSessionId,omitzero"`
	RealtimeStartInstructions           Optional[string]                       `json:"realtimeStartInstructions,omitzero"`
	ThreadID                            string                                 `json:"threadId"`
	Transport                           Optional[ThreadRealtimeStartTransport] `json:"transport,omitzero"`
	Version                             Optional[RealtimeConversationVersion]  `json:"version,omitzero"`
	Voice                               Optional[RealtimeVoice]                `json:"voice,omitzero"`
	VoiceSessionID                      Optional[string]                       `json:"voiceSessionId,omitzero"`
}

var threadRealtimeStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "backendReasoningStatus", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "clientManagedHandoffs"},
		{name: "codexResponseHandoffChannelPrefixes"},
		{name: "codexResponseHandoffMode"},
		{name: "codexResponseItemPrefix"},
		{name: "codexResponsesAsItems"},
		{name: "delegationAckFiller"},
		{name: "flushTranscriptTailOnSessionEnd"},
		{name: "includeStartupContext"},
		{name: "initialItems"},
		{name: "model"},
		{name: "outputModality", flags: fieldRequired | fieldRejectNull},
		{name: "prompt"},
		{name: "realtimeEndInstructions"},
		{name: "realtimeSessionId"},
		{name: "realtimeStartInstructions"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "transport"},
		{name: "version"},
		{name: "voice"},
		{name: "voiceSessionId"},
	},
}

func (v ThreadRealtimeStartParams) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeStartParams
	w := wire(v)
	w.BackendReasoningStatus = nonNullWithoutFalse(w.BackendReasoningStatus)
	return json.Marshal(w)
}

func (v *ThreadRealtimeStartParams) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeStartParams
	return decodeObject(data, &threadRealtimeStartParamsSpec, (*wire)(v))
}

type ThreadRealtimeStartResponse map[string]json.RawMessage

func (v ThreadRealtimeStartResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadRealtimeStartResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadRealtimeStartResponse(raw)
	return nil
}

type ThreadRealtimeStartTransport struct {
	CallID    OptionalNonNull[string] `json:"callId,omitzero"`
	Sdp       OptionalNonNull[string] `json:"sdp,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var threadRealtimeStartTransportSpec = objectSpec{
	fields: []objectField{
		{name: "callId", flags: fieldNonNullOptional},
		{name: "sdp", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  2,
	tags: []string{"websocket", "webrtc", "existingCall"},
}

func (v ThreadRealtimeStartTransport) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "websocket":
	case "webrtc":
		if !v.Sdp.IsSet() {
			return nil, DecodeError{Field: "sdp", Reason: "missing required field for type webrtc"}
		}
	case "existingCall":
		if !v.CallID.IsSet() {
			return nil, DecodeError{Field: "callId", Reason: "missing required field for type existingCall"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadRealtimeStartTransport
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeStartTransport) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeStartTransport
	var s objectScan
	known, err := s.begin(data, &threadRealtimeStartTransportSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "websocket":
	case "webrtc":
		if !s.has("sdp") {
			return DecodeError{Field: "sdp", Reason: "missing required field for type webrtc"}
		}
		if s.isNull("sdp") {
			return DecodeError{Field: "sdp", Reason: "cannot be null"}
		}
	case "existingCall":
		if !s.has("callId") {
			return DecodeError{Field: "callId", Reason: "missing required field for type existingCall"}
		}
		if s.isNull("callId") {
			return DecodeError{Field: "callId", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadRealtimeStartedNotification struct {
	RealtimeSessionID Optional[string]            `json:"realtimeSessionId,omitzero"`
	ThreadID          string                      `json:"threadId"`
	Version           RealtimeConversationVersion `json:"version"`
}

var threadRealtimeStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "realtimeSessionId"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "version", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeStartedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeStartedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeStartedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeStartedNotification
	return decodeObject(data, &threadRealtimeStartedNotificationSpec, (*wire)(v))
}

type ThreadRealtimeStopParams struct {
	ThreadID string `json:"threadId"`
}

var threadRealtimeStopParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeStopParams) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeStopParams
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeStopParams) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeStopParams
	return decodeObject(data, &threadRealtimeStopParamsSpec, (*wire)(v))
}

type ThreadRealtimeStopResponse map[string]json.RawMessage

func (v ThreadRealtimeStopResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadRealtimeStopResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadRealtimeStopResponse(raw)
	return nil
}

type ThreadRealtimeTranscriptDeltaNotification struct {
	Delta    string `json:"delta"`
	Role     string `json:"role"`
	ThreadID string `json:"threadId"`
}

var threadRealtimeTranscriptDeltaNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "delta", flags: fieldRequired | fieldRejectNull},
		{name: "role", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeTranscriptDeltaNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeTranscriptDeltaNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeTranscriptDeltaNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeTranscriptDeltaNotification
	return decodeObject(data, &threadRealtimeTranscriptDeltaNotificationSpec, (*wire)(v))
}

type ThreadRealtimeTranscriptDoneNotification struct {
	Role     string `json:"role"`
	Text     string `json:"text"`
	ThreadID string `json:"threadId"`
}

var threadRealtimeTranscriptDoneNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "role", flags: fieldRequired | fieldRejectNull},
		{name: "text", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRealtimeTranscriptDoneNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRealtimeTranscriptDoneNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRealtimeTranscriptDoneNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRealtimeTranscriptDoneNotification
	return decodeObject(data, &threadRealtimeTranscriptDoneNotificationSpec, (*wire)(v))
}

type ThreadRealtimeTranscriptRole string

const (
	ThreadRealtimeTranscriptRoleUser      ThreadRealtimeTranscriptRole = "user"
	ThreadRealtimeTranscriptRoleAssistant ThreadRealtimeTranscriptRole = "assistant"
)

type ThreadResumeInitialTurnsPageParams struct {
	ItemsView     Optional[TurnItemsView] `json:"itemsView,omitzero"`
	Limit         Optional[uint32]        `json:"limit,omitzero"`
	SortDirection Optional[SortDirection] `json:"sortDirection,omitzero"`
}

var threadResumeInitialTurnsPageParamsSpec = objectSpec{
	fields: []objectField{
		{name: "itemsView"},
		{name: "limit"},
		{name: "sortDirection"},
	},
}

func (v ThreadResumeInitialTurnsPageParams) MarshalJSON() ([]byte, error) {
	type wire ThreadResumeInitialTurnsPageParams
	return json.Marshal(wire(v))
}

func (v *ThreadResumeInitialTurnsPageParams) UnmarshalJSON(data []byte) error {
	type wire ThreadResumeInitialTurnsPageParams
	var s objectScan
	if _, err := s.begin(data, &threadResumeInitialTurnsPageParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadResumeParams struct {
	ApprovalPolicy        Optional[AskForApproval]                     `json:"approvalPolicy,omitzero"`
	ApprovalsReviewer     Optional[ApprovalsReviewer]                  `json:"approvalsReviewer,omitzero"`
	BaseInstructions      Optional[string]                             `json:"baseInstructions,omitzero"`
	Config                Optional[map[string]json.RawMessage]         `json:"config,omitzero"`
	Cwd                   Optional[string]                             `json:"cwd,omitzero"`
	DeveloperInstructions Optional[string]                             `json:"developerInstructions,omitzero"`
	DynamicTools          Optional[[]DynamicToolSpec]                  `json:"dynamicTools,omitzero"`
	ExcludeTurns          OptionalNonNull[bool]                        `json:"excludeTurns,omitzero"`
	History               Optional[[]ResponseItem]                     `json:"history,omitzero"`
	InitialTurnsPage      Optional[ThreadResumeInitialTurnsPageParams] `json:"initialTurnsPage,omitzero"`
	Model                 Optional[string]                             `json:"model,omitzero"`
	ModelProvider         Optional[string]                             `json:"modelProvider,omitzero"`
	Path                  Optional[string]                             `json:"path,omitzero"`
	Permissions           Optional[string]                             `json:"permissions,omitzero"`
	Personality           Optional[Personality]                        `json:"personality,omitzero"`
	RuntimeWorkspaceRoots Optional[[]AbsolutePathBuf]                  `json:"runtimeWorkspaceRoots,omitzero"`
	Sandbox               Optional[SandboxMode]                        `json:"sandbox,omitzero"`
	ServiceTier           Optional[string]                             `json:"serviceTier,omitzero"`
	ThreadID              string                                       `json:"threadId"`
}

var threadResumeParamsSpec = objectSpec{
	fields: []objectField{
		{name: "approvalPolicy"},
		{name: "approvalsReviewer"},
		{name: "baseInstructions"},
		{name: "config"},
		{name: "cwd"},
		{name: "developerInstructions"},
		{name: "dynamicTools", flags: fieldCustom, defaultJSON: "null"},
		{name: "excludeTurns", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "history"},
		{name: "initialTurnsPage"},
		{name: "model"},
		{name: "modelProvider"},
		{name: "path"},
		{name: "permissions"},
		{name: "personality"},
		{name: "runtimeWorkspaceRoots"},
		{name: "sandbox"},
		{name: "serviceTier"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadResumeParams) MarshalJSON() ([]byte, error) {
	type wire ThreadResumeParams
	w := wire(v)
	w.ExcludeTurns = nonNullWithoutFalse(w.ExcludeTurns)
	return json.Marshal(w)
}

func (v *ThreadResumeParams) UnmarshalJSON(data []byte) error {
	type wire ThreadResumeParams
	var s objectScan
	if _, err := s.begin(data, &threadResumeParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if raw, ok := s.value("dynamicTools"); ok {
		if bytes.Equal(raw, []byte("null")) {
			v.DynamicTools = Null[[]DynamicToolSpec]()
		} else {
			value, err := normalizeDynamicToolSpecsJSON(raw)
			if err != nil {
				return fmt.Errorf("field dynamicTools: %w", err)
			}
			v.DynamicTools = Some(value)
		}
	}
	return nil
}

type ThreadResumeResponse struct {
	ActivePermissionProfile Optional[ActivePermissionProfile]      `json:"activePermissionProfile,omitzero"`
	ApprovalPolicy          AskForApproval                         `json:"approvalPolicy"`
	ApprovalsReviewer       ApprovalsReviewer                      `json:"approvalsReviewer"`
	CollaborationMode       Optional[CollaborationMode]            `json:"collaborationMode,omitzero"`
	Cwd                     AbsolutePathBuf                        `json:"cwd"`
	DisabledPluginIDs       OptionalNonNull[[]string]              `json:"disabledPluginIds,omitzero"`
	InitialTurnsPage        Optional[TurnsPage]                    `json:"initialTurnsPage,omitzero"`
	InstructionSources      OptionalNonNull[[]LegacyAppPathString] `json:"instructionSources,omitzero"`
	ItemsBackwardsCursor    Optional[string]                       `json:"itemsBackwardsCursor,omitzero"`
	Model                   string                                 `json:"model"`
	ModelProvider           string                                 `json:"modelProvider"`
	MultiAgentMode          OptionalNonNull[MultiAgentMode]        `json:"multiAgentMode,omitzero"`
	ReasoningEffort         Optional[ReasoningEffort]              `json:"reasoningEffort,omitzero"`
	RuntimeWorkspaceRoots   OptionalNonNull[[]AbsolutePathBuf]     `json:"runtimeWorkspaceRoots,omitzero"`
	Sandbox                 SandboxPolicy                          `json:"sandbox"`
	ServiceTier             Optional[string]                       `json:"serviceTier,omitzero"`
	Thread                  Thread                                 `json:"thread"`
	TurnsBackwardsCursor    Optional[string]                       `json:"turnsBackwardsCursor,omitzero"`
}

var threadResumeResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activePermissionProfile", defaultJSON: "null"},
		{name: "approvalPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "approvalsReviewer", flags: fieldRequired | fieldRejectNull},
		{name: "collaborationMode"},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "disabledPluginIds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "initialTurnsPage", defaultJSON: "null"},
		{name: "instructionSources", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "itemsBackwardsCursor"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "modelProvider", flags: fieldRequired | fieldRejectNull},
		{name: "multiAgentMode", flags: fieldNonNullOptional, defaultJSON: "\"explicitRequestOnly\""},
		{name: "reasoningEffort"},
		{name: "runtimeWorkspaceRoots", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "sandbox", flags: fieldRequired | fieldRejectNull},
		{name: "serviceTier"},
		{name: "thread", flags: fieldRequired | fieldRejectNull},
		{name: "turnsBackwardsCursor"},
	},
}

func (v ThreadResumeResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadResumeResponse
	return json.Marshal(wire(v))
}

func (v *ThreadResumeResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadResumeResponse
	return decodeObject(data, &threadResumeResponseSpec, (*wire)(v))
}

type ThreadRevertParams struct {
	BeforeTurnID string `json:"beforeTurnId"`
	ThreadID     string `json:"threadId"`
}

var threadRevertParamsSpec = objectSpec{
	fields: []objectField{
		{name: "beforeTurnId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRevertParams) MarshalJSON() ([]byte, error) {
	type wire ThreadRevertParams
	return json.Marshal(wire(v))
}

func (v *ThreadRevertParams) UnmarshalJSON(data []byte) error {
	type wire ThreadRevertParams
	return decodeObject(data, &threadRevertParamsSpec, (*wire)(v))
}

type ThreadRevertResponse struct {
	ItemsBackwardsCursor Optional[string] `json:"itemsBackwardsCursor,omitzero"`
	Thread               Thread           `json:"thread"`
	TurnsBackwardsCursor Optional[string] `json:"turnsBackwardsCursor,omitzero"`
}

var threadRevertResponseSpec = objectSpec{
	fields: []objectField{
		{name: "itemsBackwardsCursor"},
		{name: "thread", flags: fieldRequired | fieldRejectNull},
		{name: "turnsBackwardsCursor"},
	},
}

func (v ThreadRevertResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadRevertResponse
	return json.Marshal(wire(v))
}

func (v *ThreadRevertResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadRevertResponse
	return decodeObject(data, &threadRevertResponseSpec, (*wire)(v))
}

type ThreadRevertedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadRevertedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadRevertedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadRevertedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadRevertedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadRevertedNotification
	return decodeObject(data, &threadRevertedNotificationSpec, (*wire)(v))
}

type ThreadSampleCancelParams struct {
	RequestID RequestID `json:"requestId"`
}

var threadSampleCancelParamsSpec = objectSpec{
	fields: []objectField{
		{name: "requestId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSampleCancelParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSampleCancelParams
	return json.Marshal(wire(v))
}

func (v *ThreadSampleCancelParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSampleCancelParams
	return decodeObject(data, &threadSampleCancelParamsSpec, (*wire)(v))
}

type ThreadSampleCancelResponse map[string]json.RawMessage

func (v ThreadSampleCancelResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadSampleCancelResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadSampleCancelResponse(raw)
	return nil
}

type ThreadSampleParams struct {
	DisableMcp                 OptionalNonNull[bool]       `json:"disableMcp,omitzero"`
	Effort                     Optional[ReasoningEffort]   `json:"effort,omitzero"`
	ExpectedTurnID             string                      `json:"expectedTurnId"`
	Prompt                     string                      `json:"prompt"`
	ResponsesapiClientMetadata Optional[map[string]string] `json:"responsesapiClientMetadata,omitzero"`
	ThreadID                   string                      `json:"threadId"`
	ThreadSource               Optional[ThreadSource]      `json:"threadSource,omitzero"`
	TurnTrigger                Optional[string]            `json:"turnTrigger,omitzero"`
}

var threadSampleParamsSpec = objectSpec{
	fields: []objectField{
		{name: "disableMcp", flags: fieldNonNullOptional},
		{name: "effort"},
		{name: "expectedTurnId", flags: fieldRequired | fieldRejectNull},
		{name: "prompt", flags: fieldRequired | fieldRejectNull},
		{name: "responsesapiClientMetadata"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "threadSource"},
		{name: "turnTrigger"},
	},
}

func (v ThreadSampleParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSampleParams
	return json.Marshal(wire(v))
}

func (v *ThreadSampleParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSampleParams
	return decodeObject(data, &threadSampleParamsSpec, (*wire)(v))
}

type ThreadSampleResponse struct {
	CachedInputTokens Optional[int64]  `json:"cachedInputTokens,omitzero"`
	InputTokens       Optional[int64]  `json:"inputTokens,omitzero"`
	OutputTokens      Optional[int64]  `json:"outputTokens,omitzero"`
	ResponseID        string           `json:"responseId"`
	SourceTurnID      string           `json:"sourceTurnId"`
	Text              Optional[string] `json:"text,omitzero"`
}

var threadSampleResponseSpec = objectSpec{
	fields: []objectField{
		{name: "cachedInputTokens"},
		{name: "inputTokens"},
		{name: "outputTokens"},
		{name: "responseId", flags: fieldRequired | fieldRejectNull},
		{name: "sourceTurnId", flags: fieldRequired | fieldRejectNull},
		{name: "text"},
	},
}

func (v ThreadSampleResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadSampleResponse
	return json.Marshal(wire(v))
}

func (v *ThreadSampleResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadSampleResponse
	return decodeObject(data, &threadSampleResponseSpec, (*wire)(v))
}

type ThreadSearchOccurrence struct {
	ItemID            string                `json:"itemId"`
	Snippet           string                `json:"snippet"`
	SnippetMatchRange ThreadSearchTextRange `json:"snippetMatchRange"`
	TurnCursor        string                `json:"turnCursor"`
	TurnID            string                `json:"turnId"`
}

var threadSearchOccurrenceSpec = objectSpec{
	fields: []objectField{
		{name: "itemId", flags: fieldRequired | fieldRejectNull},
		{name: "snippet", flags: fieldRequired | fieldRejectNull},
		{name: "snippetMatchRange", flags: fieldRequired | fieldRejectNull},
		{name: "turnCursor", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSearchOccurrence) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchOccurrence
	return json.Marshal(wire(v))
}

func (v *ThreadSearchOccurrence) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchOccurrence
	return decodeObject(data, &threadSearchOccurrenceSpec, (*wire)(v))
}

type ThreadSearchOccurrencesParams struct {
	Cursor     Optional[string] `json:"cursor,omitzero"`
	Limit      Optional[uint32] `json:"limit,omitzero"`
	SearchTerm string           `json:"searchTerm"`
	ThreadID   string           `json:"threadId"`
}

var threadSearchOccurrencesParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "searchTerm", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSearchOccurrencesParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchOccurrencesParams
	return json.Marshal(wire(v))
}

func (v *ThreadSearchOccurrencesParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchOccurrencesParams
	var s objectScan
	if _, err := s.begin(data, &threadSearchOccurrencesParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadSearchOccurrencesResponse struct {
	Data       []ThreadSearchOccurrence `json:"data"`
	NextCursor Optional[string]         `json:"nextCursor,omitzero"`
}

var threadSearchOccurrencesResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadSearchOccurrencesResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchOccurrencesResponse
	return json.Marshal(wire(v))
}

func (v *ThreadSearchOccurrencesResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchOccurrencesResponse
	return decodeObject(data, &threadSearchOccurrencesResponseSpec, (*wire)(v))
}

type ThreadSearchParams struct {
	Archived      Optional[bool]                `json:"archived,omitzero"`
	Cursor        Optional[string]              `json:"cursor,omitzero"`
	Limit         Optional[uint32]              `json:"limit,omitzero"`
	SearchTerm    string                        `json:"searchTerm"`
	SortDirection Optional[SortDirection]       `json:"sortDirection,omitzero"`
	SortKey       Optional[ThreadSearchSortKey] `json:"sortKey,omitzero"`
	SourceKinds   Optional[[]ThreadSourceKind]  `json:"sourceKinds,omitzero"`
}

var threadSearchParamsSpec = objectSpec{
	fields: []objectField{
		{name: "archived"},
		{name: "cursor"},
		{name: "limit"},
		{name: "searchTerm", flags: fieldRequired | fieldRejectNull},
		{name: "sortDirection"},
		{name: "sortKey"},
		{name: "sourceKinds"},
	},
}

func (v ThreadSearchParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchParams
	return json.Marshal(wire(v))
}

func (v *ThreadSearchParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchParams
	var s objectScan
	if _, err := s.begin(data, &threadSearchParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadSearchResponse struct {
	BackwardsCursor Optional[string]     `json:"backwardsCursor,omitzero"`
	Data            []ThreadSearchResult `json:"data"`
	NextCursor      Optional[string]     `json:"nextCursor,omitzero"`
}

var threadSearchResponseSpec = objectSpec{
	fields: []objectField{
		{name: "backwardsCursor"},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadSearchResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchResponse
	return json.Marshal(wire(v))
}

func (v *ThreadSearchResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchResponse
	return decodeObject(data, &threadSearchResponseSpec, (*wire)(v))
}

type ThreadSearchResult struct {
	Snippet string `json:"snippet"`
	Thread  Thread `json:"thread"`
}

var threadSearchResultSpec = objectSpec{
	fields: []objectField{
		{name: "snippet", flags: fieldRequired | fieldRejectNull},
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSearchResult) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchResult
	return json.Marshal(wire(v))
}

func (v *ThreadSearchResult) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchResult
	return decodeObject(data, &threadSearchResultSpec, (*wire)(v))
}

type ThreadSearchSortKey string

const (
	ThreadSearchSortKeyCreatedAt ThreadSearchSortKey = "created_at"
	ThreadSearchSortKeyUpdatedAt ThreadSearchSortKey = "updated_at"
	ThreadSearchSortKeyRecencyAt ThreadSearchSortKey = "recency_at"
)

type ThreadSearchTextRange struct {
	End   uint32 `json:"end"`
	Start uint32 `json:"start"`
}

var threadSearchTextRangeSpec = objectSpec{
	fields: []objectField{
		{name: "end", flags: fieldRequired | fieldRejectNull},
		{name: "start", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSearchTextRange) MarshalJSON() ([]byte, error) {
	type wire ThreadSearchTextRange
	return json.Marshal(wire(v))
}

func (v *ThreadSearchTextRange) UnmarshalJSON(data []byte) error {
	type wire ThreadSearchTextRange
	var s objectScan
	if _, err := s.begin(data, &threadSearchTextRangeSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.End < 0 {
		return DecodeError{Field: "end", Reason: "below minimum 0"}
	}
	if v.Start < 0 {
		return DecodeError{Field: "start", Reason: "below minimum 0"}
	}
	return nil
}

type ThreadSection struct {
	Appearance Optional[ThreadSectionAppearance] `json:"appearance,omitzero"`
	ID         string                            `json:"id"`
	Name       string                            `json:"name"`
}

var threadSectionSpec = objectSpec{
	fields: []objectField{
		{name: "appearance"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSection) MarshalJSON() ([]byte, error) {
	type wire ThreadSection
	return json.Marshal(wire(v))
}

func (v *ThreadSection) UnmarshalJSON(data []byte) error {
	type wire ThreadSection
	return decodeObject(data, &threadSectionSpec, (*wire)(v))
}

type ThreadSectionAppearance struct {
	Color Optional[string] `json:"color,omitzero"`
	Icon  Optional[string] `json:"icon,omitzero"`
}

var threadSectionAppearanceSpec = objectSpec{
	fields: []objectField{
		{name: "color"},
		{name: "icon"},
	},
}

func (v ThreadSectionAppearance) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionAppearance
	return json.Marshal(wire(v))
}

func (v *ThreadSectionAppearance) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionAppearance
	return decodeObject(data, &threadSectionAppearanceSpec, (*wire)(v))
}

type ThreadSectionCreateParams struct {
	Appearance Optional[ThreadSectionAppearance] `json:"appearance,omitzero"`
	Name       string                            `json:"name"`
}

var threadSectionCreateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "appearance"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSectionCreateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionCreateParams
	return json.Marshal(wire(v))
}

func (v *ThreadSectionCreateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionCreateParams
	return decodeObject(data, &threadSectionCreateParamsSpec, (*wire)(v))
}

type ThreadSectionCreateResponse struct {
	Section ThreadSection `json:"section"`
}

var threadSectionCreateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "section", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSectionCreateResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionCreateResponse
	return json.Marshal(wire(v))
}

func (v *ThreadSectionCreateResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionCreateResponse
	return decodeObject(data, &threadSectionCreateResponseSpec, (*wire)(v))
}

type ThreadSectionDeleteParams struct {
	SectionID string `json:"sectionId"`
}

var threadSectionDeleteParamsSpec = objectSpec{
	fields: []objectField{
		{name: "sectionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSectionDeleteParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionDeleteParams
	return json.Marshal(wire(v))
}

func (v *ThreadSectionDeleteParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionDeleteParams
	return decodeObject(data, &threadSectionDeleteParamsSpec, (*wire)(v))
}

type ThreadSectionDeleteResponse map[string]json.RawMessage

func (v ThreadSectionDeleteResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadSectionDeleteResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadSectionDeleteResponse(raw)
	return nil
}

type ThreadSectionListParams struct {
	Cursor Optional[string] `json:"cursor,omitzero"`
	Limit  Optional[uint32] `json:"limit,omitzero"`
}

var threadSectionListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
	},
}

func (v ThreadSectionListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionListParams
	return json.Marshal(wire(v))
}

func (v *ThreadSectionListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionListParams
	var s objectScan
	if _, err := s.begin(data, &threadSectionListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadSectionListResponse struct {
	Data       []ThreadSection  `json:"data"`
	NextCursor Optional[string] `json:"nextCursor,omitzero"`
}

var threadSectionListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadSectionListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadSectionListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionListResponse
	return decodeObject(data, &threadSectionListResponseSpec, (*wire)(v))
}

type ThreadSectionMoveParams struct {
	BeforeThreadID Optional[string] `json:"beforeThreadId,omitzero"`
	SectionID      Optional[string] `json:"sectionId,omitzero"`
	ThreadID       string           `json:"threadId"`
}

var threadSectionMoveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "beforeThreadId"},
		{name: "sectionId", flags: fieldRequired},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSectionMoveParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionMoveParams
	return json.Marshal(wire(v))
}

func (v *ThreadSectionMoveParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionMoveParams
	return decodeObject(data, &threadSectionMoveParamsSpec, (*wire)(v))
}

type ThreadSectionMoveResponse map[string]json.RawMessage

func (v ThreadSectionMoveResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadSectionMoveResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadSectionMoveResponse(raw)
	return nil
}

type ThreadSectionUpdateParams struct {
	Appearance Optional[ThreadSectionAppearance] `json:"appearance,omitzero"`
	Name       string                            `json:"name"`
	SectionID  string                            `json:"sectionId"`
}

var threadSectionUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "appearance"},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "sectionId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSectionUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionUpdateParams
	return json.Marshal(wire(v))
}

func (v *ThreadSectionUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionUpdateParams
	return decodeObject(data, &threadSectionUpdateParamsSpec, (*wire)(v))
}

type ThreadSectionUpdateResponse struct {
	Section ThreadSection `json:"section"`
}

var threadSectionUpdateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "section", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSectionUpdateResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadSectionUpdateResponse
	return json.Marshal(wire(v))
}

func (v *ThreadSectionUpdateResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadSectionUpdateResponse
	return decodeObject(data, &threadSectionUpdateResponseSpec, (*wire)(v))
}

type ThreadSetNameParams struct {
	Name     string `json:"name"`
	ThreadID string `json:"threadId"`
}

var threadSetNameParamsSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSetNameParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSetNameParams
	return json.Marshal(wire(v))
}

func (v *ThreadSetNameParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSetNameParams
	return decodeObject(data, &threadSetNameParamsSpec, (*wire)(v))
}

type ThreadSetNameResponse map[string]json.RawMessage

func (v ThreadSetNameResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadSetNameResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadSetNameResponse(raw)
	return nil
}

type ThreadSettings struct {
	ActivePermissionProfile Optional[ActivePermissionProfile] `json:"activePermissionProfile,omitzero"`
	ApprovalPolicy          AskForApproval                    `json:"approvalPolicy"`
	ApprovalsReviewer       ApprovalsReviewer                 `json:"approvalsReviewer"`
	CollaborationMode       CollaborationMode                 `json:"collaborationMode"`
	Cwd                     AbsolutePathBuf                   `json:"cwd"`
	DisabledPluginIDs       OptionalNonNull[[]string]         `json:"disabledPluginIds,omitzero"`
	Effort                  Optional[ReasoningEffort]         `json:"effort,omitzero"`
	Model                   string                            `json:"model"`
	ModelProvider           string                            `json:"modelProvider"`
	MultiAgentMode          OptionalNonNull[MultiAgentMode]   `json:"multiAgentMode,omitzero"`
	Personality             Optional[Personality]             `json:"personality,omitzero"`
	SandboxPolicy           SandboxPolicy                     `json:"sandboxPolicy"`
	ServiceTier             Optional[string]                  `json:"serviceTier,omitzero"`
	Summary                 Optional[ReasoningSummary]        `json:"summary,omitzero"`
}

var threadSettingsSpec = objectSpec{
	fields: []objectField{
		{name: "activePermissionProfile"},
		{name: "approvalPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "approvalsReviewer", flags: fieldRequired | fieldRejectNull},
		{name: "collaborationMode", flags: fieldRequired | fieldRejectNull},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "disabledPluginIds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "effort"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "modelProvider", flags: fieldRequired | fieldRejectNull},
		{name: "multiAgentMode", flags: fieldNonNullOptional, defaultJSON: "\"auto\""},
		{name: "personality"},
		{name: "sandboxPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "serviceTier"},
		{name: "summary"},
	},
}

func (v ThreadSettings) MarshalJSON() ([]byte, error) {
	type wire ThreadSettings
	return json.Marshal(wire(v))
}

func (v *ThreadSettings) UnmarshalJSON(data []byte) error {
	type wire ThreadSettings
	return decodeObject(data, &threadSettingsSpec, (*wire)(v))
}

type ThreadSettingsUpdateParams struct {
	ApprovalPolicy    Optional[AskForApproval]    `json:"approvalPolicy,omitzero"`
	ApprovalsReviewer Optional[ApprovalsReviewer] `json:"approvalsReviewer,omitzero"`
	CollaborationMode Optional[CollaborationMode] `json:"collaborationMode,omitzero"`
	Cwd               Optional[string]            `json:"cwd,omitzero"`
	DisabledPluginIDs Optional[[]string]          `json:"disabledPluginIds,omitzero"`
	Effort            Optional[ReasoningEffort]   `json:"effort,omitzero"`
	Model             Optional[string]            `json:"model,omitzero"`
	MultiAgentMode    Optional[MultiAgentMode]    `json:"multiAgentMode,omitzero"`
	Permissions       Optional[string]            `json:"permissions,omitzero"`
	Personality       Optional[Personality]       `json:"personality,omitzero"`
	SandboxPolicy     Optional[SandboxPolicy]     `json:"sandboxPolicy,omitzero"`
	ServiceTier       Optional[string]            `json:"serviceTier,omitzero"`
	Summary           Optional[ReasoningSummary]  `json:"summary,omitzero"`
	ThreadID          string                      `json:"threadId"`
}

var threadSettingsUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "approvalPolicy"},
		{name: "approvalsReviewer"},
		{name: "collaborationMode"},
		{name: "cwd"},
		{name: "disabledPluginIds"},
		{name: "effort"},
		{name: "model"},
		{name: "multiAgentMode"},
		{name: "permissions"},
		{name: "personality"},
		{name: "sandboxPolicy"},
		{name: "serviceTier"},
		{name: "summary"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSettingsUpdateParams) MarshalJSON() ([]byte, error) {
	type wire ThreadSettingsUpdateParams
	return json.Marshal(wire(v))
}

func (v *ThreadSettingsUpdateParams) UnmarshalJSON(data []byte) error {
	type wire ThreadSettingsUpdateParams
	return decodeObject(data, &threadSettingsUpdateParamsSpec, (*wire)(v))
}

type ThreadSettingsUpdateResponse map[string]json.RawMessage

func (v ThreadSettingsUpdateResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadSettingsUpdateResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadSettingsUpdateResponse(raw)
	return nil
}

type ThreadSettingsUpdatedNotification struct {
	ThreadID       string         `json:"threadId"`
	ThreadSettings ThreadSettings `json:"threadSettings"`
}

var threadSettingsUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "threadSettings", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadSettingsUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadSettingsUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadSettingsUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadSettingsUpdatedNotification
	return decodeObject(data, &threadSettingsUpdatedNotificationSpec, (*wire)(v))
}

type ThreadShellCommandParams struct {
	Command   string          `json:"command"`
	ThreadID  string          `json:"threadId"`
	TimeoutMs Optional[int64] `json:"timeoutMs,omitzero"`
}

var threadShellCommandParamsSpec = objectSpec{
	fields: []objectField{
		{name: "command", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "timeoutMs"},
	},
}

func (v ThreadShellCommandParams) MarshalJSON() ([]byte, error) {
	type wire ThreadShellCommandParams
	return json.Marshal(wire(v))
}

func (v *ThreadShellCommandParams) UnmarshalJSON(data []byte) error {
	type wire ThreadShellCommandParams
	return decodeObject(data, &threadShellCommandParamsSpec, (*wire)(v))
}

type ThreadShellCommandResponse map[string]json.RawMessage

func (v ThreadShellCommandResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *ThreadShellCommandResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = ThreadShellCommandResponse(raw)
	return nil
}

type ThreadSortKey string

const (
	ThreadSortKeyCreatedAt       ThreadSortKey = "created_at"
	ThreadSortKeyUpdatedAt       ThreadSortKey = "updated_at"
	ThreadSortKeyRecencyAt       ThreadSortKey = "recency_at"
	ThreadSortKeySectionPosition ThreadSortKey = "section_position"
)

type ThreadSource string

type ThreadSourceKind string

const (
	ThreadSourceKindCli                 ThreadSourceKind = "cli"
	ThreadSourceKindVscode              ThreadSourceKind = "vscode"
	ThreadSourceKindExec                ThreadSourceKind = "exec"
	ThreadSourceKindAppServer           ThreadSourceKind = "appServer"
	ThreadSourceKindSubAgent            ThreadSourceKind = "subAgent"
	ThreadSourceKindSubAgentReview      ThreadSourceKind = "subAgentReview"
	ThreadSourceKindSubAgentCompact     ThreadSourceKind = "subAgentCompact"
	ThreadSourceKindSubAgentThreadSpawn ThreadSourceKind = "subAgentThreadSpawn"
	ThreadSourceKindSubAgentOther       ThreadSourceKind = "subAgentOther"
	ThreadSourceKindUnknown             ThreadSourceKind = "unknown"
)

type ThreadStartParams struct {
	AllowProviderModelFallback OptionalNonNull[bool]                `json:"allowProviderModelFallback,omitzero"`
	ApprovalPolicy             Optional[AskForApproval]             `json:"approvalPolicy,omitzero"`
	ApprovalsReviewer          Optional[ApprovalsReviewer]          `json:"approvalsReviewer,omitzero"`
	BaseInstructions           Optional[string]                     `json:"baseInstructions,omitzero"`
	Config                     Optional[map[string]json.RawMessage] `json:"config,omitzero"`
	Cwd                        Optional[string]                     `json:"cwd,omitzero"`
	DaybreakEnabled            Optional[bool]                       `json:"daybreakEnabled,omitzero"`
	DeveloperInstructions      Optional[string]                     `json:"developerInstructions,omitzero"`
	DynamicTools               Optional[[]DynamicToolSpec]          `json:"dynamicTools,omitzero"`
	Environments               Optional[[]TurnEnvironmentParams]    `json:"environments,omitzero"`
	Ephemeral                  Optional[bool]                       `json:"ephemeral,omitzero"`
	ExperimentalRawEvents      OptionalNonNull[bool]                `json:"experimentalRawEvents,omitzero"`
	HistoryMode                Optional[ThreadHistoryMode]          `json:"historyMode,omitzero"`
	MockExperimentalField      Optional[string]                     `json:"mockExperimentalField,omitzero"`
	Model                      Optional[string]                     `json:"model,omitzero"`
	ModelProvider              Optional[string]                     `json:"modelProvider,omitzero"`
	MultiAgentMode             Optional[MultiAgentMode]             `json:"multiAgentMode,omitzero"`
	Permissions                Optional[string]                     `json:"permissions,omitzero"`
	Personality                Optional[Personality]                `json:"personality,omitzero"`
	ProjectID                  Optional[string]                     `json:"projectId,omitzero"`
	RuntimeWorkspaceRoots      Optional[[]AbsolutePathBuf]          `json:"runtimeWorkspaceRoots,omitzero"`
	Sandbox                    Optional[SandboxMode]                `json:"sandbox,omitzero"`
	SelectedCapabilityRoots    Optional[[]SelectedCapabilityRoot]   `json:"selectedCapabilityRoots,omitzero"`
	ServiceName                Optional[string]                     `json:"serviceName,omitzero"`
	ServiceTier                Optional[string]                     `json:"serviceTier,omitzero"`
	SessionStartSource         Optional[ThreadStartSource]          `json:"sessionStartSource,omitzero"`
	ThreadSource               Optional[ThreadSource]               `json:"threadSource,omitzero"`
}

var threadStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "allowProviderModelFallback", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "approvalPolicy"},
		{name: "approvalsReviewer"},
		{name: "baseInstructions"},
		{name: "config"},
		{name: "cwd"},
		{name: "daybreakEnabled"},
		{name: "developerInstructions"},
		{name: "dynamicTools", flags: fieldCustom, defaultJSON: "null"},
		{name: "environments"},
		{name: "ephemeral"},
		{name: "experimentalRawEvents", flags: fieldNonNullOptional, defaultJSON: "false"},
		{name: "historyMode"},
		{name: "mockExperimentalField"},
		{name: "model"},
		{name: "modelProvider"},
		{name: "multiAgentMode"},
		{name: "permissions"},
		{name: "personality"},
		{name: "projectId"},
		{name: "runtimeWorkspaceRoots"},
		{name: "sandbox"},
		{name: "selectedCapabilityRoots"},
		{name: "serviceName"},
		{name: "serviceTier"},
		{name: "sessionStartSource"},
		{name: "threadSource"},
	},
}

func (v ThreadStartParams) MarshalJSON() ([]byte, error) {
	type wire ThreadStartParams
	w := wire(v)
	w.AllowProviderModelFallback = nonNullWithoutFalse(w.AllowProviderModelFallback)
	w.ExperimentalRawEvents = nonNullWithoutFalse(w.ExperimentalRawEvents)
	return json.Marshal(w)
}

func (v *ThreadStartParams) UnmarshalJSON(data []byte) error {
	type wire ThreadStartParams
	var s objectScan
	if _, err := s.begin(data, &threadStartParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if raw, ok := s.value("dynamicTools"); ok {
		if bytes.Equal(raw, []byte("null")) {
			v.DynamicTools = Null[[]DynamicToolSpec]()
		} else {
			value, err := normalizeDynamicToolSpecsJSON(raw)
			if err != nil {
				return fmt.Errorf("field dynamicTools: %w", err)
			}
			v.DynamicTools = Some(value)
		}
	}
	return nil
}

type ThreadStartResponse struct {
	ActivePermissionProfile Optional[ActivePermissionProfile]      `json:"activePermissionProfile,omitzero"`
	ApprovalPolicy          AskForApproval                         `json:"approvalPolicy"`
	ApprovalsReviewer       ApprovalsReviewer                      `json:"approvalsReviewer"`
	Cwd                     AbsolutePathBuf                        `json:"cwd"`
	DisabledPluginIDs       OptionalNonNull[[]string]              `json:"disabledPluginIds,omitzero"`
	InstructionSources      OptionalNonNull[[]LegacyAppPathString] `json:"instructionSources,omitzero"`
	Model                   string                                 `json:"model"`
	ModelProvider           string                                 `json:"modelProvider"`
	MultiAgentMode          OptionalNonNull[MultiAgentMode]        `json:"multiAgentMode,omitzero"`
	ReasoningEffort         Optional[ReasoningEffort]              `json:"reasoningEffort,omitzero"`
	RuntimeWorkspaceRoots   OptionalNonNull[[]AbsolutePathBuf]     `json:"runtimeWorkspaceRoots,omitzero"`
	Sandbox                 SandboxPolicy                          `json:"sandbox"`
	ServiceTier             Optional[string]                       `json:"serviceTier,omitzero"`
	Thread                  Thread                                 `json:"thread"`
}

var threadStartResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activePermissionProfile", defaultJSON: "null"},
		{name: "approvalPolicy", flags: fieldRequired | fieldRejectNull},
		{name: "approvalsReviewer", flags: fieldRequired | fieldRejectNull},
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "disabledPluginIds", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "instructionSources", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "model", flags: fieldRequired | fieldRejectNull},
		{name: "modelProvider", flags: fieldRequired | fieldRejectNull},
		{name: "multiAgentMode", flags: fieldNonNullOptional, defaultJSON: "\"explicitRequestOnly\""},
		{name: "reasoningEffort"},
		{name: "runtimeWorkspaceRoots", flags: fieldNonNullOptional, defaultJSON: "[]"},
		{name: "sandbox", flags: fieldRequired | fieldRejectNull},
		{name: "serviceTier"},
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadStartResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadStartResponse
	return json.Marshal(wire(v))
}

func (v *ThreadStartResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadStartResponse
	return decodeObject(data, &threadStartResponseSpec, (*wire)(v))
}

type ThreadStartSource string

const (
	ThreadStartSourceStartup ThreadStartSource = "startup"
	ThreadStartSourceClear   ThreadStartSource = "clear"
)

type ThreadStartedNotification struct {
	Thread Thread `json:"thread"`
}

var threadStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadStartedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadStartedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadStartedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadStartedNotification
	return decodeObject(data, &threadStartedNotificationSpec, (*wire)(v))
}

type ThreadStatus struct {
	ActiveFlags OptionalNonNull[[]ThreadActiveFlag] `json:"activeFlags,omitzero"`
	TypeValue   string                              `json:"type"`
	RawJSON     json.RawMessage                     `json:"-"`
}

var threadStatusSpec = objectSpec{
	fields: []objectField{
		{name: "activeFlags", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"notLoaded", "idle", "systemError", "active"},
}

func (v ThreadStatus) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "notLoaded":
	case "idle":
	case "systemError":
	case "active":
		if !v.ActiveFlags.IsSet() {
			return nil, DecodeError{Field: "activeFlags", Reason: "missing required field for type active"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadStatus
	return json.Marshal(wire(v))
}

func (v *ThreadStatus) UnmarshalJSON(data []byte) error {
	type wire ThreadStatus
	var s objectScan
	known, err := s.begin(data, &threadStatusSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "notLoaded":
	case "idle":
	case "systemError":
	case "active":
		if !s.has("activeFlags") {
			return DecodeError{Field: "activeFlags", Reason: "missing required field for type active"}
		}
		if s.isNull("activeFlags") {
			return DecodeError{Field: "activeFlags", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadStatusChangedNotification struct {
	Status   ThreadStatus `json:"status"`
	ThreadID string       `json:"threadId"`
}

var threadStatusChangedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadStatusChangedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadStatusChangedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadStatusChangedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadStatusChangedNotification
	return decodeObject(data, &threadStatusChangedNotificationSpec, (*wire)(v))
}

type ThreadTimelineEntry struct {
	CompletedAt Optional[int64]             `json:"completedAt,omitzero"`
	DurationMs  Optional[int64]             `json:"durationMs,omitzero"`
	Error       Optional[TurnError]         `json:"error,omitzero"`
	Item        json.RawMessage             `json:"item,omitempty"`
	Position    OptionalNonNull[uint64]     `json:"position,omitzero"`
	StartedAt   Optional[int64]             `json:"startedAt,omitzero"`
	Status      OptionalNonNull[TurnStatus] `json:"status,omitzero"`
	TurnID      OptionalNonNull[string]     `json:"turnId,omitzero"`
	TypeValue   string                      `json:"type"`
	RawJSON     json.RawMessage             `json:"-"`
}

var threadTimelineEntrySpec = objectSpec{
	fields: []objectField{
		{name: "completedAt"},
		{name: "durationMs"},
		{name: "error"},
		{name: "item", flags: fieldRejectNull},
		{name: "position", flags: fieldNonNullOptional},
		{name: "startedAt"},
		{name: "status", flags: fieldNonNullOptional},
		{name: "turnId", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  8,
	tags: []string{"item", "realtime", "turnStarted", "turnCompleted"},
}

func (v ThreadTimelineEntry) MarshalJSON() ([]byte, error) {
	if bytes.Equal(v.Item, []byte("null")) {
		return nil, DecodeError{Field: "item", Reason: "cannot be null"}
	}
	switch v.TypeValue {
	case "item":
		if len(v.Item) == 0 {
			return nil, DecodeError{Field: "item", Reason: "missing required field for type item"}
		}
		if bytes.Equal(v.Item, []byte("null")) {
			return nil, DecodeError{Field: "item", Reason: "cannot be null"}
		}
		if !v.Position.IsSet() {
			return nil, DecodeError{Field: "position", Reason: "missing required field for type item"}
		}
		if !v.TurnID.IsSet() {
			return nil, DecodeError{Field: "turnId", Reason: "missing required field for type item"}
		}
	case "realtime":
		if len(v.Item) == 0 {
			return nil, DecodeError{Field: "item", Reason: "missing required field for type realtime"}
		}
		if bytes.Equal(v.Item, []byte("null")) {
			return nil, DecodeError{Field: "item", Reason: "cannot be null"}
		}
		if !v.Position.IsSet() {
			return nil, DecodeError{Field: "position", Reason: "missing required field for type realtime"}
		}
	case "turnStarted":
		if !v.Position.IsSet() {
			return nil, DecodeError{Field: "position", Reason: "missing required field for type turnStarted"}
		}
		if !v.TurnID.IsSet() {
			return nil, DecodeError{Field: "turnId", Reason: "missing required field for type turnStarted"}
		}
	case "turnCompleted":
		if !v.Position.IsSet() {
			return nil, DecodeError{Field: "position", Reason: "missing required field for type turnCompleted"}
		}
		if !v.Status.IsSet() {
			return nil, DecodeError{Field: "status", Reason: "missing required field for type turnCompleted"}
		}
		if !v.TurnID.IsSet() {
			return nil, DecodeError{Field: "turnId", Reason: "missing required field for type turnCompleted"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadTimelineEntry
	return json.Marshal(wire(v))
}

func (v *ThreadTimelineEntry) UnmarshalJSON(data []byte) error {
	type wire ThreadTimelineEntry
	var s objectScan
	known, err := s.begin(data, &threadTimelineEntrySpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valuePosition, ok := v.Position.Value(); ok {
		if valuePosition < 0 {
			return DecodeError{Field: "position", Reason: "below minimum 0"}
		}
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "item":
		if !s.has("item") {
			return DecodeError{Field: "item", Reason: "missing required field for type item"}
		}
		if s.isNull("item") {
			return DecodeError{Field: "item", Reason: "cannot be null"}
		}
		if !s.has("position") {
			return DecodeError{Field: "position", Reason: "missing required field for type item"}
		}
		if s.isNull("position") {
			return DecodeError{Field: "position", Reason: "cannot be null"}
		}
		if !s.has("turnId") {
			return DecodeError{Field: "turnId", Reason: "missing required field for type item"}
		}
		if s.isNull("turnId") {
			return DecodeError{Field: "turnId", Reason: "cannot be null"}
		}
	case "realtime":
		if !s.has("item") {
			return DecodeError{Field: "item", Reason: "missing required field for type realtime"}
		}
		if s.isNull("item") {
			return DecodeError{Field: "item", Reason: "cannot be null"}
		}
		if !s.has("position") {
			return DecodeError{Field: "position", Reason: "missing required field for type realtime"}
		}
		if s.isNull("position") {
			return DecodeError{Field: "position", Reason: "cannot be null"}
		}
	case "turnStarted":
		if !s.has("position") {
			return DecodeError{Field: "position", Reason: "missing required field for type turnStarted"}
		}
		if s.isNull("position") {
			return DecodeError{Field: "position", Reason: "cannot be null"}
		}
		if !s.has("turnId") {
			return DecodeError{Field: "turnId", Reason: "missing required field for type turnStarted"}
		}
		if s.isNull("turnId") {
			return DecodeError{Field: "turnId", Reason: "cannot be null"}
		}
	case "turnCompleted":
		if !s.has("position") {
			return DecodeError{Field: "position", Reason: "missing required field for type turnCompleted"}
		}
		if s.isNull("position") {
			return DecodeError{Field: "position", Reason: "cannot be null"}
		}
		if !s.has("status") {
			return DecodeError{Field: "status", Reason: "missing required field for type turnCompleted"}
		}
		if s.isNull("status") {
			return DecodeError{Field: "status", Reason: "cannot be null"}
		}
		if !s.has("turnId") {
			return DecodeError{Field: "turnId", Reason: "missing required field for type turnCompleted"}
		}
		if s.isNull("turnId") {
			return DecodeError{Field: "turnId", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadTimelineListParams struct {
	Cursor   Optional[string] `json:"cursor,omitzero"`
	Limit    Optional[uint32] `json:"limit,omitzero"`
	ThreadID string           `json:"threadId"`
}

var threadTimelineListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "limit"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadTimelineListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadTimelineListParams
	return json.Marshal(wire(v))
}

func (v *ThreadTimelineListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadTimelineListParams
	var s objectScan
	if _, err := s.begin(data, &threadTimelineListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadTimelineListResponse struct {
	ActiveRealtimeSessionAtPageStart Optional[string]      `json:"activeRealtimeSessionAtPageStart,omitzero"`
	Data                             []ThreadTimelineEntry `json:"data"`
	NextCursor                       Optional[string]      `json:"nextCursor,omitzero"`
}

var threadTimelineListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "activeRealtimeSessionAtPageStart"},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadTimelineListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadTimelineListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadTimelineListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadTimelineListResponse
	return decodeObject(data, &threadTimelineListResponseSpec, (*wire)(v))
}

type ThreadTokenUsage struct {
	Last               TokenUsageBreakdown `json:"last"`
	ModelContextWindow Optional[int64]     `json:"modelContextWindow,omitzero"`
	Total              TokenUsageBreakdown `json:"total"`
}

var threadTokenUsageSpec = objectSpec{
	fields: []objectField{
		{name: "last", flags: fieldRequired | fieldRejectNull},
		{name: "modelContextWindow"},
		{name: "total", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadTokenUsage) MarshalJSON() ([]byte, error) {
	type wire ThreadTokenUsage
	return json.Marshal(wire(v))
}

func (v *ThreadTokenUsage) UnmarshalJSON(data []byte) error {
	type wire ThreadTokenUsage
	return decodeObject(data, &threadTokenUsageSpec, (*wire)(v))
}

type ThreadTokenUsageUpdatedNotification struct {
	ThreadID   string           `json:"threadId"`
	TokenUsage ThreadTokenUsage `json:"tokenUsage"`
	TurnID     string           `json:"turnId"`
}

var threadTokenUsageUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "tokenUsage", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadTokenUsageUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadTokenUsageUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadTokenUsageUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadTokenUsageUpdatedNotification
	return decodeObject(data, &threadTokenUsageUpdatedNotificationSpec, (*wire)(v))
}

type ThreadTurnsListParams struct {
	Cursor        Optional[string]        `json:"cursor,omitzero"`
	ItemsView     Optional[TurnItemsView] `json:"itemsView,omitzero"`
	Limit         Optional[uint32]        `json:"limit,omitzero"`
	SortDirection Optional[SortDirection] `json:"sortDirection,omitzero"`
	ThreadID      string                  `json:"threadId"`
}

var threadTurnsListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cursor"},
		{name: "itemsView"},
		{name: "limit"},
		{name: "sortDirection"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadTurnsListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadTurnsListParams
	return json.Marshal(wire(v))
}

func (v *ThreadTurnsListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadTurnsListParams
	var s objectScan
	if _, err := s.begin(data, &threadTurnsListParamsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueLimit, ok := v.Limit.Value(); ok {
		if valueLimit < 0 {
			return DecodeError{Field: "limit", Reason: "below minimum 0"}
		}
	}
	return nil
}

type ThreadTurnsListResponse struct {
	BackwardsCursor Optional[string] `json:"backwardsCursor,omitzero"`
	Data            []Turn           `json:"data"`
	NextCursor      Optional[string] `json:"nextCursor,omitzero"`
}

var threadTurnsListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "backwardsCursor"},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v ThreadTurnsListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadTurnsListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadTurnsListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadTurnsListResponse
	return decodeObject(data, &threadTurnsListResponseSpec, (*wire)(v))
}

type ThreadUnarchiveParams struct {
	ThreadID string `json:"threadId"`
}

var threadUnarchiveParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadUnarchiveParams) MarshalJSON() ([]byte, error) {
	type wire ThreadUnarchiveParams
	return json.Marshal(wire(v))
}

func (v *ThreadUnarchiveParams) UnmarshalJSON(data []byte) error {
	type wire ThreadUnarchiveParams
	return decodeObject(data, &threadUnarchiveParamsSpec, (*wire)(v))
}

type ThreadUnarchiveResponse struct {
	Thread Thread `json:"thread"`
}

var threadUnarchiveResponseSpec = objectSpec{
	fields: []objectField{
		{name: "thread", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadUnarchiveResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadUnarchiveResponse
	return json.Marshal(wire(v))
}

func (v *ThreadUnarchiveResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadUnarchiveResponse
	return decodeObject(data, &threadUnarchiveResponseSpec, (*wire)(v))
}

type ThreadUnarchivedNotification struct {
	ThreadID string `json:"threadId"`
}

var threadUnarchivedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadUnarchivedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadUnarchivedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadUnarchivedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadUnarchivedNotification
	return decodeObject(data, &threadUnarchivedNotificationSpec, (*wire)(v))
}

type ThreadUnreadPosition struct {
	TurnID    OptionalNonNull[string] `json:"turnId,omitzero"`
	TypeValue string                  `json:"type"`
	RawJSON   json.RawMessage         `json:"-"`
}

var threadUnreadPositionSpec = objectSpec{
	fields: []objectField{
		{name: "turnId", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"threadStart", "turn"},
}

func (v ThreadUnreadPosition) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "threadStart":
	case "turn":
		if !v.TurnID.IsSet() {
			return nil, DecodeError{Field: "turnId", Reason: "missing required field for type turn"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire ThreadUnreadPosition
	return json.Marshal(wire(v))
}

func (v *ThreadUnreadPosition) UnmarshalJSON(data []byte) error {
	type wire ThreadUnreadPosition
	var s objectScan
	known, err := s.begin(data, &threadUnreadPositionSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "threadStart":
	case "turn":
		if !s.has("turnId") {
			return DecodeError{Field: "turnId", Reason: "missing required field for type turn"}
		}
		if s.isNull("turnId") {
			return DecodeError{Field: "turnId", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type ThreadUnsubscribeParams struct {
	ThreadID string `json:"threadId"`
}

var threadUnsubscribeParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadUnsubscribeParams) MarshalJSON() ([]byte, error) {
	type wire ThreadUnsubscribeParams
	return json.Marshal(wire(v))
}

func (v *ThreadUnsubscribeParams) UnmarshalJSON(data []byte) error {
	type wire ThreadUnsubscribeParams
	return decodeObject(data, &threadUnsubscribeParamsSpec, (*wire)(v))
}

type ThreadUnsubscribeResponse struct {
	Status ThreadUnsubscribeStatus `json:"status"`
}

var threadUnsubscribeResponseSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadUnsubscribeResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadUnsubscribeResponse
	return json.Marshal(wire(v))
}

func (v *ThreadUnsubscribeResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadUnsubscribeResponse
	return decodeObject(data, &threadUnsubscribeResponseSpec, (*wire)(v))
}

type ThreadUnsubscribeStatus string

const (
	ThreadUnsubscribeStatusNotLoaded     ThreadUnsubscribeStatus = "notLoaded"
	ThreadUnsubscribeStatusNotSubscribed ThreadUnsubscribeStatus = "notSubscribed"
	ThreadUnsubscribeStatusUnsubscribed  ThreadUnsubscribeStatus = "unsubscribed"
)

type ThreadUsage struct {
	EstimatedUsageCreditsMicros int64                       `json:"estimatedUsageCreditsMicros"`
	EstimatedUsageUsdMicros     Optional[int64]             `json:"estimatedUsageUsdMicros,omitzero"`
	Groups                      []ThreadUsageBreakdownGroup `json:"groups"`
	NativeUsageUsdMicros        Optional[int64]             `json:"nativeUsageUsdMicros,omitzero"`
	ThreadID                    string                      `json:"threadId"`
}

var threadUsageSpec = objectSpec{
	fields: []objectField{
		{name: "estimatedUsageCreditsMicros", flags: fieldRequired | fieldRejectNull},
		{name: "estimatedUsageUsdMicros"},
		{name: "groups", flags: fieldRequired | fieldRejectNull},
		{name: "nativeUsageUsdMicros"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadUsage) MarshalJSON() ([]byte, error) {
	type wire ThreadUsage
	return json.Marshal(wire(v))
}

func (v *ThreadUsage) UnmarshalJSON(data []byte) error {
	type wire ThreadUsage
	return decodeObject(data, &threadUsageSpec, (*wire)(v))
}

type ThreadUsageBreakdownGroup struct {
	CachedInputTokens           Optional[int64]  `json:"cachedInputTokens,omitzero"`
	EstimatedUsageCreditsMicros int64            `json:"estimatedUsageCreditsMicros"`
	InputTokens                 Optional[int64]  `json:"inputTokens,omitzero"`
	Model                       Optional[string] `json:"model,omitzero"`
	NetNewInputTokens           Optional[int64]  `json:"netNewInputTokens,omitzero"`
	OutputTokens                Optional[int64]  `json:"outputTokens,omitzero"`
	ReasoningEffort             Optional[string] `json:"reasoningEffort,omitzero"`
	Speed                       Optional[string] `json:"speed,omitzero"`
	TotalTokens                 Optional[int64]  `json:"totalTokens,omitzero"`
}

var threadUsageBreakdownGroupSpec = objectSpec{
	fields: []objectField{
		{name: "cachedInputTokens"},
		{name: "estimatedUsageCreditsMicros", flags: fieldRequired | fieldRejectNull},
		{name: "inputTokens"},
		{name: "model"},
		{name: "netNewInputTokens"},
		{name: "outputTokens"},
		{name: "reasoningEffort"},
		{name: "speed"},
		{name: "totalTokens"},
	},
}

func (v ThreadUsageBreakdownGroup) MarshalJSON() ([]byte, error) {
	type wire ThreadUsageBreakdownGroup
	return json.Marshal(wire(v))
}

func (v *ThreadUsageBreakdownGroup) UnmarshalJSON(data []byte) error {
	type wire ThreadUsageBreakdownGroup
	return decodeObject(data, &threadUsageBreakdownGroupSpec, (*wire)(v))
}

type ThreadWorkflowCancelParams struct {
	RunID    string `json:"runId"`
	ThreadID string `json:"threadId"`
}

var threadWorkflowCancelParamsSpec = objectSpec{
	fields: []objectField{
		{name: "runId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowCancelParams) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowCancelParams
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowCancelParams) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowCancelParams
	return decodeObject(data, &threadWorkflowCancelParamsSpec, (*wire)(v))
}

type ThreadWorkflowCancelResponse struct {
	Run WorkflowRun `json:"run"`
}

var threadWorkflowCancelResponseSpec = objectSpec{
	fields: []objectField{
		{name: "run", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowCancelResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowCancelResponse
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowCancelResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowCancelResponse
	return decodeObject(data, &threadWorkflowCancelResponseSpec, (*wire)(v))
}

type ThreadWorkflowListParams struct {
	ThreadID string `json:"threadId"`
}

var threadWorkflowListParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowListParams) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowListParams
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowListParams) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowListParams
	return decodeObject(data, &threadWorkflowListParamsSpec, (*wire)(v))
}

type ThreadWorkflowListResponse struct {
	Runs []WorkflowRun `json:"runs"`
}

var threadWorkflowListResponseSpec = objectSpec{
	fields: []objectField{
		{name: "runs", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowListResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowListResponse
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowListResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowListResponse
	return decodeObject(data, &threadWorkflowListResponseSpec, (*wire)(v))
}

type ThreadWorkflowReadParams struct {
	IncludeResult OptionalNonNull[bool] `json:"includeResult,omitzero"`
	RunID         string                `json:"runId"`
	ThreadID      string                `json:"threadId"`
}

var threadWorkflowReadParamsSpec = objectSpec{
	fields: []objectField{
		{name: "includeResult", flags: fieldNonNullOptional},
		{name: "runId", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowReadParams) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowReadParams
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowReadParams) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowReadParams
	return decodeObject(data, &threadWorkflowReadParamsSpec, (*wire)(v))
}

type ThreadWorkflowReadResponse struct {
	Result Optional[string] `json:"result,omitzero"`
	Run    WorkflowRun      `json:"run"`
}

var threadWorkflowReadResponseSpec = objectSpec{
	fields: []objectField{
		{name: "result"},
		{name: "run", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowReadResponse) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowReadResponse
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowReadResponse) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowReadResponse
	return decodeObject(data, &threadWorkflowReadResponseSpec, (*wire)(v))
}

type ThreadWorkflowUpdatedNotification struct {
	Run      WorkflowRun `json:"run"`
	ThreadID string      `json:"threadId"`
}

var threadWorkflowUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "run", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v ThreadWorkflowUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire ThreadWorkflowUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *ThreadWorkflowUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire ThreadWorkflowUpdatedNotification
	return decodeObject(data, &threadWorkflowUpdatedNotificationSpec, (*wire)(v))
}

type TokenUsageBreakdown struct {
	CacheWriteInputTokens OptionalNonNull[int64] `json:"cacheWriteInputTokens,omitzero"`
	CachedInputTokens     int64                  `json:"cachedInputTokens"`
	InputTokens           int64                  `json:"inputTokens"`
	OutputTokens          int64                  `json:"outputTokens"`
	ReasoningOutputTokens int64                  `json:"reasoningOutputTokens"`
	TotalTokens           int64                  `json:"totalTokens"`
}

var tokenUsageBreakdownSpec = objectSpec{
	fields: []objectField{
		{name: "cacheWriteInputTokens", flags: fieldNonNullOptional},
		{name: "cachedInputTokens", flags: fieldRequired | fieldRejectNull},
		{name: "inputTokens", flags: fieldRequired | fieldRejectNull},
		{name: "outputTokens", flags: fieldRequired | fieldRejectNull},
		{name: "reasoningOutputTokens", flags: fieldRequired | fieldRejectNull},
		{name: "totalTokens", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TokenUsageBreakdown) MarshalJSON() ([]byte, error) {
	type wire TokenUsageBreakdown
	return json.Marshal(wire(v))
}

func (v *TokenUsageBreakdown) UnmarshalJSON(data []byte) error {
	type wire TokenUsageBreakdown
	return decodeObject(data, &tokenUsageBreakdownSpec, (*wire)(v))
}

type Tool struct {
	Meta         json.RawMessage             `json:"_meta,omitempty"`
	Annotations  json.RawMessage             `json:"annotations,omitempty"`
	Description  Optional[string]            `json:"description,omitzero"`
	Icons        Optional[[]json.RawMessage] `json:"icons,omitzero"`
	InputSchema  json.RawMessage             `json:"inputSchema"`
	Name         string                      `json:"name"`
	OutputSchema json.RawMessage             `json:"outputSchema,omitempty"`
	Title        Optional[string]            `json:"title,omitzero"`
}

var toolSpec = objectSpec{
	fields: []objectField{
		{name: "_meta"},
		{name: "annotations"},
		{name: "description"},
		{name: "icons"},
		{name: "inputSchema", flags: fieldRequired},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "outputSchema"},
		{name: "title"},
	},
}

func (v Tool) MarshalJSON() ([]byte, error) {
	type wire Tool
	return json.Marshal(wire(v))
}

func (v *Tool) UnmarshalJSON(data []byte) error {
	type wire Tool
	return decodeObject(data, &toolSpec, (*wire)(v))
}

type ToolExposureSurface string

const (
	ToolExposureSurfaceCodeMode ToolExposureSurface = "code_mode"
	ToolExposureSurfaceDeferred ToolExposureSurface = "deferred"
	ToolExposureSurfaceDirect   ToolExposureSurface = "direct"
)

type ToolsV2 struct {
	WebSearch Optional[WebSearchToolConfig] `json:"web_search,omitzero"`
}

var toolsV2Spec = objectSpec{
	fields: []objectField{
		{name: "web_search"},
	},
}

func (v ToolsV2) MarshalJSON() ([]byte, error) {
	type wire ToolsV2
	return json.Marshal(wire(v))
}

func (v *ToolsV2) UnmarshalJSON(data []byte) error {
	type wire ToolsV2
	return decodeObject(data, &toolsV2Spec, (*wire)(v))
}

type Turn struct {
	CompletedAt Optional[int64]                `json:"completedAt,omitzero"`
	DurationMs  Optional[int64]                `json:"durationMs,omitzero"`
	Error       Optional[TurnError]            `json:"error,omitzero"`
	ID          string                         `json:"id"`
	Items       []ThreadItem                   `json:"items"`
	ItemsView   OptionalNonNull[TurnItemsView] `json:"itemsView,omitzero"`
	RootTurnID  Optional[string]               `json:"rootTurnId,omitzero"`
	StartedAt   Optional[int64]                `json:"startedAt,omitzero"`
	Status      TurnStatus                     `json:"status"`
}

var turnSpec = objectSpec{
	fields: []objectField{
		{name: "completedAt"},
		{name: "durationMs"},
		{name: "error"},
		{name: "id", flags: fieldRequired | fieldRejectNull},
		{name: "items", flags: fieldRequired | fieldRejectNull},
		{name: "itemsView", flags: fieldNonNullOptional, defaultJSON: "\"visible\""},
		{name: "rootTurnId"},
		{name: "startedAt"},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v Turn) MarshalJSON() ([]byte, error) {
	type wire Turn
	return json.Marshal(wire(v))
}

func (v *Turn) UnmarshalJSON(data []byte) error {
	type wire Turn
	return decodeObject(data, &turnSpec, (*wire)(v))
}

type TurnCompletedNotification struct {
	ThreadID string `json:"threadId"`
	Turn     Turn   `json:"turn"`
}

var turnCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turn", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire TurnCompletedNotification
	return json.Marshal(wire(v))
}

func (v *TurnCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire TurnCompletedNotification
	return decodeObject(data, &turnCompletedNotificationSpec, (*wire)(v))
}

type TurnDiffUpdatedNotification struct {
	Diff     string `json:"diff"`
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var turnDiffUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "diff", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnDiffUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire TurnDiffUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *TurnDiffUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire TurnDiffUpdatedNotification
	return decodeObject(data, &turnDiffUpdatedNotificationSpec, (*wire)(v))
}

type TurnEnvironmentParams struct {
	Cwd                   LegacyAppPathString             `json:"cwd"`
	EnvironmentID         string                          `json:"environmentId"`
	RuntimeWorkspaceRoots Optional[[]LegacyAppPathString] `json:"runtimeWorkspaceRoots,omitzero"`
}

var turnEnvironmentParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwd", flags: fieldRequired | fieldRejectNull},
		{name: "environmentId", flags: fieldRequired | fieldRejectNull},
		{name: "runtimeWorkspaceRoots"},
	},
}

func (v TurnEnvironmentParams) MarshalJSON() ([]byte, error) {
	type wire TurnEnvironmentParams
	return json.Marshal(wire(v))
}

func (v *TurnEnvironmentParams) UnmarshalJSON(data []byte) error {
	type wire TurnEnvironmentParams
	return decodeObject(data, &turnEnvironmentParamsSpec, (*wire)(v))
}

type TurnError struct {
	AdditionalDetails Optional[string]                   `json:"additionalDetails,omitzero"`
	CodexErrorInfo    Optional[CodexErrorInfo]           `json:"codexErrorInfo,omitzero"`
	Message           string                             `json:"message"`
	Misalignment      Optional[MisalignmentErrorDetails] `json:"misalignment,omitzero"`
}

var turnErrorSpec = objectSpec{
	fields: []objectField{
		{name: "additionalDetails", defaultJSON: "null"},
		{name: "codexErrorInfo"},
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "misalignment"},
	},
}

func (v TurnError) MarshalJSON() ([]byte, error) {
	type wire TurnError
	return json.Marshal(wire(v))
}

func (v *TurnError) UnmarshalJSON(data []byte) error {
	type wire TurnError
	return decodeObject(data, &turnErrorSpec, (*wire)(v))
}

type TurnInterruptParams struct {
	ThreadID string `json:"threadId"`
	TurnID   string `json:"turnId"`
}

var turnInterruptParamsSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnInterruptParams) MarshalJSON() ([]byte, error) {
	type wire TurnInterruptParams
	return json.Marshal(wire(v))
}

func (v *TurnInterruptParams) UnmarshalJSON(data []byte) error {
	type wire TurnInterruptParams
	return decodeObject(data, &turnInterruptParamsSpec, (*wire)(v))
}

type TurnInterruptResponse map[string]json.RawMessage

func (v TurnInterruptResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *TurnInterruptResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = TurnInterruptResponse(raw)
	return nil
}

type TurnItemsView string

const (
	TurnItemsViewNotLoaded TurnItemsView = "notLoaded"
	TurnItemsViewSummary   TurnItemsView = "summary"
	TurnItemsViewFull      TurnItemsView = "full"
)

type TurnModerationMetadataNotification struct {
	Metadata json.RawMessage `json:"metadata"`
	ThreadID string          `json:"threadId"`
	TurnID   string          `json:"turnId"`
}

var turnModerationMetadataNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "metadata", flags: fieldRequired},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnModerationMetadataNotification) MarshalJSON() ([]byte, error) {
	type wire TurnModerationMetadataNotification
	return json.Marshal(wire(v))
}

func (v *TurnModerationMetadataNotification) UnmarshalJSON(data []byte) error {
	type wire TurnModerationMetadataNotification
	return decodeObject(data, &turnModerationMetadataNotificationSpec, (*wire)(v))
}

type TurnPlanStep struct {
	Status TurnPlanStepStatus `json:"status"`
	Step   string             `json:"step"`
}

var turnPlanStepSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
		{name: "step", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnPlanStep) MarshalJSON() ([]byte, error) {
	type wire TurnPlanStep
	return json.Marshal(wire(v))
}

func (v *TurnPlanStep) UnmarshalJSON(data []byte) error {
	type wire TurnPlanStep
	return decodeObject(data, &turnPlanStepSpec, (*wire)(v))
}

type TurnPlanStepStatus string

const (
	TurnPlanStepStatusPending    TurnPlanStepStatus = "pending"
	TurnPlanStepStatusInProgress TurnPlanStepStatus = "inProgress"
	TurnPlanStepStatusCompleted  TurnPlanStepStatus = "completed"
)

type TurnPlanUpdatedNotification struct {
	Explanation Optional[string] `json:"explanation,omitzero"`
	Plan        []TurnPlanStep   `json:"plan"`
	ThreadID    string           `json:"threadId"`
	TurnID      string           `json:"turnId"`
}

var turnPlanUpdatedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "explanation"},
		{name: "plan", flags: fieldRequired | fieldRejectNull},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnPlanUpdatedNotification) MarshalJSON() ([]byte, error) {
	type wire TurnPlanUpdatedNotification
	return json.Marshal(wire(v))
}

func (v *TurnPlanUpdatedNotification) UnmarshalJSON(data []byte) error {
	type wire TurnPlanUpdatedNotification
	return decodeObject(data, &turnPlanUpdatedNotificationSpec, (*wire)(v))
}

type TurnSettingsUpdateParams struct {
	ApprovalsReviewer Optional[ApprovalsReviewer] `json:"approvalsReviewer,omitzero"`
	Effort            Optional[ReasoningEffort]   `json:"effort,omitzero"`
	Model             Optional[string]            `json:"model,omitzero"`
	ServiceTier       Optional[string]            `json:"serviceTier,omitzero"`
	Summary           Optional[ReasoningSummary]  `json:"summary,omitzero"`
	ThreadID          string                      `json:"threadId"`
	TurnID            string                      `json:"turnId"`
}

var turnSettingsUpdateParamsSpec = objectSpec{
	fields: []objectField{
		{name: "approvalsReviewer"},
		{name: "effort"},
		{name: "model"},
		{name: "serviceTier"},
		{name: "summary"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnSettingsUpdateParams) MarshalJSON() ([]byte, error) {
	type wire TurnSettingsUpdateParams
	return json.Marshal(wire(v))
}

func (v *TurnSettingsUpdateParams) UnmarshalJSON(data []byte) error {
	type wire TurnSettingsUpdateParams
	return decodeObject(data, &turnSettingsUpdateParamsSpec, (*wire)(v))
}

type TurnSettingsUpdateResponse struct {
	Status TurnSettingsUpdateStatus `json:"status"`
}

var turnSettingsUpdateResponseSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnSettingsUpdateResponse) MarshalJSON() ([]byte, error) {
	type wire TurnSettingsUpdateResponse
	return json.Marshal(wire(v))
}

func (v *TurnSettingsUpdateResponse) UnmarshalJSON(data []byte) error {
	type wire TurnSettingsUpdateResponse
	return decodeObject(data, &turnSettingsUpdateResponseSpec, (*wire)(v))
}

type TurnSettingsUpdateStatus string

const (
	TurnSettingsUpdateStatusApplied           TurnSettingsUpdateStatus = "applied"
	TurnSettingsUpdateStatusTargetUnavailable TurnSettingsUpdateStatus = "targetUnavailable"
)

type TurnStartParams struct {
	AdditionalContext               Optional[map[string]AdditionalContextEntry] `json:"additionalContext,omitzero"`
	ApprovalPolicy                  Optional[AskForApproval]                    `json:"approvalPolicy,omitzero"`
	ApprovalsReviewer               Optional[ApprovalsReviewer]                 `json:"approvalsReviewer,omitzero"`
	ClientUserMessageID             Optional[string]                            `json:"clientUserMessageId,omitzero"`
	CollaborationMode               Optional[CollaborationMode]                 `json:"collaborationMode,omitzero"`
	Cwd                             Optional[string]                            `json:"cwd,omitzero"`
	CyberAccessProgram              Optional[CyberAccessProgram]                `json:"cyberAccessProgram,omitzero"`
	DisabledPluginIDs               Optional[[]string]                          `json:"disabledPluginIds,omitzero"`
	Effort                          Optional[ReasoningEffort]                   `json:"effort,omitzero"`
	Environments                    Optional[[]TurnEnvironmentParams]           `json:"environments,omitzero"`
	Input                           []UserInput                                 `json:"input"`
	Model                           Optional[string]                            `json:"model,omitzero"`
	MultiAgentMode                  Optional[MultiAgentMode]                    `json:"multiAgentMode,omitzero"`
	OutputSchema                    json.RawMessage                             `json:"outputSchema,omitempty"`
	ParentTurnID                    Optional[string]                            `json:"parentTurnId,omitzero"`
	Permissions                     Optional[string]                            `json:"permissions,omitzero"`
	Personality                     Optional[Personality]                       `json:"personality,omitzero"`
	ResponsesapiClientMetadata      Optional[map[string]string]                 `json:"responsesapiClientMetadata,omitzero"`
	ResponsesapiClientMetadataPatch Optional[map[string]string]                 `json:"responsesapiClientMetadataPatch,omitzero"`
	RootTurnID                      Optional[string]                            `json:"rootTurnId,omitzero"`
	RuntimeWorkspaceRoots           Optional[[]AbsolutePathBuf]                 `json:"runtimeWorkspaceRoots,omitzero"`
	SandboxPolicy                   Optional[SandboxPolicy]                     `json:"sandboxPolicy,omitzero"`
	ServiceTier                     Optional[string]                            `json:"serviceTier,omitzero"`
	ServiceTierForTurn              Optional[string]                            `json:"serviceTierForTurn,omitzero"`
	Summary                         Optional[ReasoningSummary]                  `json:"summary,omitzero"`
	ThreadID                        string                                      `json:"threadId"`
	ToolOutput                      Optional[TurnToolOutput]                    `json:"toolOutput,omitzero"`
	TurnTrigger                     Optional[string]                            `json:"turnTrigger,omitzero"`
}

var turnStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "additionalContext"},
		{name: "approvalPolicy"},
		{name: "approvalsReviewer"},
		{name: "clientUserMessageId"},
		{name: "collaborationMode"},
		{name: "cwd"},
		{name: "cyberAccessProgram"},
		{name: "disabledPluginIds"},
		{name: "effort"},
		{name: "environments"},
		{name: "input", flags: fieldRequired | fieldRejectNull},
		{name: "model"},
		{name: "multiAgentMode"},
		{name: "outputSchema"},
		{name: "parentTurnId"},
		{name: "permissions"},
		{name: "personality"},
		{name: "responsesapiClientMetadata"},
		{name: "responsesapiClientMetadataPatch"},
		{name: "rootTurnId"},
		{name: "runtimeWorkspaceRoots"},
		{name: "sandboxPolicy"},
		{name: "serviceTier"},
		{name: "serviceTierForTurn"},
		{name: "summary"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "toolOutput"},
		{name: "turnTrigger"},
	},
}

func (v TurnStartParams) MarshalJSON() ([]byte, error) {
	type wire TurnStartParams
	return json.Marshal(wire(v))
}

func (v *TurnStartParams) UnmarshalJSON(data []byte) error {
	type wire TurnStartParams
	return decodeObject(data, &turnStartParamsSpec, (*wire)(v))
}

type TurnStartResponse struct {
	Turn Turn `json:"turn"`
}

var turnStartResponseSpec = objectSpec{
	fields: []objectField{
		{name: "turn", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnStartResponse) MarshalJSON() ([]byte, error) {
	type wire TurnStartResponse
	return json.Marshal(wire(v))
}

func (v *TurnStartResponse) UnmarshalJSON(data []byte) error {
	type wire TurnStartResponse
	return decodeObject(data, &turnStartResponseSpec, (*wire)(v))
}

type TurnStartedNotification struct {
	ThreadID string `json:"threadId"`
	Turn     Turn   `json:"turn"`
}

var turnStartedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
		{name: "turn", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnStartedNotification) MarshalJSON() ([]byte, error) {
	type wire TurnStartedNotification
	return json.Marshal(wire(v))
}

func (v *TurnStartedNotification) UnmarshalJSON(data []byte) error {
	type wire TurnStartedNotification
	return decodeObject(data, &turnStartedNotificationSpec, (*wire)(v))
}

type TurnStatus string

const (
	TurnStatusCompleted   TurnStatus = "completed"
	TurnStatusInterrupted TurnStatus = "interrupted"
	TurnStatusFailed      TurnStatus = "failed"
	TurnStatusInProgress  TurnStatus = "inProgress"
)

type TurnSteerParams struct {
	AdditionalContext               Optional[map[string]AdditionalContextEntry] `json:"additionalContext,omitzero"`
	ClientUserMessageID             Optional[string]                            `json:"clientUserMessageId,omitzero"`
	ExpectedTurnID                  string                                      `json:"expectedTurnId"`
	Input                           []UserInput                                 `json:"input"`
	ResponsesapiClientMetadata      Optional[map[string]string]                 `json:"responsesapiClientMetadata,omitzero"`
	ResponsesapiClientMetadataPatch Optional[map[string]string]                 `json:"responsesapiClientMetadataPatch,omitzero"`
	ThreadID                        string                                      `json:"threadId"`
}

var turnSteerParamsSpec = objectSpec{
	fields: []objectField{
		{name: "additionalContext"},
		{name: "clientUserMessageId"},
		{name: "expectedTurnId", flags: fieldRequired | fieldRejectNull},
		{name: "input", flags: fieldRequired | fieldRejectNull},
		{name: "responsesapiClientMetadata"},
		{name: "responsesapiClientMetadataPatch"},
		{name: "threadId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnSteerParams) MarshalJSON() ([]byte, error) {
	type wire TurnSteerParams
	return json.Marshal(wire(v))
}

func (v *TurnSteerParams) UnmarshalJSON(data []byte) error {
	type wire TurnSteerParams
	return decodeObject(data, &turnSteerParamsSpec, (*wire)(v))
}

type TurnSteerResponse struct {
	TurnID string `json:"turnId"`
}

var turnSteerResponseSpec = objectSpec{
	fields: []objectField{
		{name: "turnId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v TurnSteerResponse) MarshalJSON() ([]byte, error) {
	type wire TurnSteerResponse
	return json.Marshal(wire(v))
}

func (v *TurnSteerResponse) UnmarshalJSON(data []byte) error {
	type wire TurnSteerResponse
	return decodeObject(data, &turnSteerResponseSpec, (*wire)(v))
}

type TurnToolOutput struct {
	Name      string                 `json:"name"`
	Namespace Optional[string]       `json:"namespace,omitzero"`
	Output    FunctionCallOutputBody `json:"output"`
	Retain    OptionalNonNull[bool]  `json:"retain,omitzero"`
}

var turnToolOutputSpec = objectSpec{
	fields: []objectField{
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "namespace"},
		{name: "output", flags: fieldRequired | fieldRejectNull},
		{name: "retain", flags: fieldNonNullOptional},
	},
}

func (v TurnToolOutput) MarshalJSON() ([]byte, error) {
	type wire TurnToolOutput
	return json.Marshal(wire(v))
}

func (v *TurnToolOutput) UnmarshalJSON(data []byte) error {
	type wire TurnToolOutput
	return decodeObject(data, &turnToolOutputSpec, (*wire)(v))
}

type TurnsPage struct {
	BackwardsCursor Optional[string] `json:"backwardsCursor,omitzero"`
	Data            []Turn           `json:"data"`
	NextCursor      Optional[string] `json:"nextCursor,omitzero"`
}

var turnsPageSpec = objectSpec{
	fields: []objectField{
		{name: "backwardsCursor"},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "nextCursor"},
	},
}

func (v TurnsPage) MarshalJSON() ([]byte, error) {
	type wire TurnsPage
	return json.Marshal(wire(v))
}

func (v *TurnsPage) UnmarshalJSON(data []byte) error {
	type wire TurnsPage
	return decodeObject(data, &turnsPageSpec, (*wire)(v))
}

type UserInput struct {
	Detail       Optional[ImageDetail]          `json:"detail,omitzero"`
	FileID       OptionalNonNull[string]        `json:"fileId,omitzero"`
	Name         OptionalNonNull[string]        `json:"name,omitzero"`
	Path         OptionalNonNull[string]        `json:"path,omitzero"`
	Text         OptionalNonNull[string]        `json:"text,omitzero"`
	TextElements OptionalNonNull[[]TextElement] `json:"text_elements,omitzero"`
	TypeValue    string                         `json:"type"`
	URL          OptionalNonNull[string]        `json:"url,omitzero"`
	RawJSON      json.RawMessage                `json:"-"`
}

var userInputSpec = objectSpec{
	fields: []objectField{
		{name: "detail"},
		{name: "fileId", flags: fieldNonNullOptional, anyOfTags: []string{"\"image\""}},
		{name: "name", flags: fieldNonNullOptional},
		{name: "path", flags: fieldNonNullOptional},
		{name: "text", flags: fieldNonNullOptional},
		{name: "text_elements", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "url", flags: fieldNonNullOptional, anyOfTags: []string{"\"image\""}},
	},
	tag:  6,
	tags: []string{"text", "image", "localImage", "audio", "localAudio", "skill", "mention"},
}

func (v UserInput) MarshalJSON() ([]byte, error) {
	type wire UserInput
	w := wire(v)
	switch v.TypeValue {
	case "text":
		if !v.Text.IsSet() {
			return nil, DecodeError{Field: "text", Reason: "missing required field for type text"}
		}
	case "image":
		taggedAnyOfVariant1Matches := (w.URL.IsSet() && marshalsNonNull(w.URL)) || (w.FileID.IsSet() && marshalsNonNull(w.FileID))
		if !taggedAnyOfVariant1Matches {
			return nil, DecodeError{Field: "", Reason: "does not match any anyOf variant for type image"}
		}
	case "localImage":
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type localImage"}
		}
	case "audio":
		if !v.URL.IsSet() {
			return nil, DecodeError{Field: "url", Reason: "missing required field for type audio"}
		}
	case "localAudio":
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type localAudio"}
		}
	case "skill":
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type skill"}
		}
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type skill"}
		}
	case "mention":
		if !v.Name.IsSet() {
			return nil, DecodeError{Field: "name", Reason: "missing required field for type mention"}
		}
		if !v.Path.IsSet() {
			return nil, DecodeError{Field: "path", Reason: "missing required field for type mention"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	return json.Marshal(w)
}

func (v *UserInput) UnmarshalJSON(data []byte) error {
	type wire UserInput
	var s objectScan
	known, err := s.begin(data, &userInputSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "text":
		if !s.has("text") {
			return DecodeError{Field: "text", Reason: "missing required field for type text"}
		}
		if s.isNull("text") {
			return DecodeError{Field: "text", Reason: "cannot be null"}
		}
	case "image":
		taggedAnyOfVariant1Matches := (v.URL.IsSet()) || (v.FileID.IsSet())
		if !taggedAnyOfVariant1Matches {
			return DecodeError{Field: "", Reason: "does not match any anyOf variant for type image"}
		}
	case "localImage":
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type localImage"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "audio":
		if !s.has("url") {
			return DecodeError{Field: "url", Reason: "missing required field for type audio"}
		}
		if s.isNull("url") {
			return DecodeError{Field: "url", Reason: "cannot be null"}
		}
	case "localAudio":
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type localAudio"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "skill":
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type skill"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type skill"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	case "mention":
		if !s.has("name") {
			return DecodeError{Field: "name", Reason: "missing required field for type mention"}
		}
		if s.isNull("name") {
			return DecodeError{Field: "name", Reason: "cannot be null"}
		}
		if !s.has("path") {
			return DecodeError{Field: "path", Reason: "missing required field for type mention"}
		}
		if s.isNull("path") {
			return DecodeError{Field: "path", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type UserVerificationCancelParams struct {
	RequestID RequestID `json:"requestId"`
}

var userVerificationCancelParamsSpec = objectSpec{
	fields: []objectField{
		{name: "requestId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v UserVerificationCancelParams) MarshalJSON() ([]byte, error) {
	type wire UserVerificationCancelParams
	return json.Marshal(wire(v))
}

func (v *UserVerificationCancelParams) UnmarshalJSON(data []byte) error {
	type wire UserVerificationCancelParams
	return decodeObject(data, &userVerificationCancelParamsSpec, (*wire)(v))
}

type UserVerificationCancelResponse map[string]json.RawMessage

func (v UserVerificationCancelResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *UserVerificationCancelResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = UserVerificationCancelResponse(raw)
	return nil
}

type UserVerificationCancellationReason string

const (
	UserVerificationCancellationReasonUserCancelled UserVerificationCancellationReason = "userCancelled"
	UserVerificationCancellationReasonInterrupted   UserVerificationCancellationReason = "interrupted"
)

type UserVerificationDeleteParams struct {
}

var userVerificationDeleteParamsSpec = objectSpec{
	fields: []objectField{},
}

func (v UserVerificationDeleteParams) MarshalJSON() ([]byte, error) {
	type wire UserVerificationDeleteParams
	return json.Marshal(wire(v))
}

func (v *UserVerificationDeleteParams) UnmarshalJSON(data []byte) error {
	type wire UserVerificationDeleteParams
	return decodeObject(data, &userVerificationDeleteParamsSpec, (*wire)(v))
}

type UserVerificationDeleteResponse map[string]json.RawMessage

func (v UserVerificationDeleteResponse) MarshalJSON() ([]byte, error) {
	if v == nil {
		return []byte("{}"), nil
	}
	return json.Marshal(map[string]json.RawMessage(v))
}

func (v *UserVerificationDeleteResponse) UnmarshalJSON(data []byte) error {
	if bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		return DecodeError{Field: "", Reason: "cannot be null"}
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*v = UserVerificationDeleteResponse(raw)
	return nil
}

type UserVerificationEnrollParams struct {
}

var userVerificationEnrollParamsSpec = objectSpec{
	fields: []objectField{},
}

func (v UserVerificationEnrollParams) MarshalJSON() ([]byte, error) {
	type wire UserVerificationEnrollParams
	return json.Marshal(wire(v))
}

func (v *UserVerificationEnrollParams) UnmarshalJSON(data []byte) error {
	type wire UserVerificationEnrollParams
	return decodeObject(data, &userVerificationEnrollParamsSpec, (*wire)(v))
}

type UserVerificationEnrollResponse struct {
	Algorithm    Optional[string] `json:"algorithm,omitzero"`
	CredentialID string           `json:"credentialId"`
	PublicKey    Optional[string] `json:"publicKey,omitzero"`
}

var userVerificationEnrollResponseSpec = objectSpec{
	fields: []objectField{
		{name: "algorithm"},
		{name: "credentialId", flags: fieldRequired | fieldRejectNull},
		{name: "publicKey"},
	},
}

func (v UserVerificationEnrollResponse) MarshalJSON() ([]byte, error) {
	type wire UserVerificationEnrollResponse
	return json.Marshal(wire(v))
}

func (v *UserVerificationEnrollResponse) UnmarshalJSON(data []byte) error {
	type wire UserVerificationEnrollResponse
	return decodeObject(data, &userVerificationEnrollResponseSpec, (*wire)(v))
}

type UserVerificationErrorDetails struct {
	Reason    OptionalNonNull[UserVerificationInvalidRequestReason] `json:"reason,omitzero"`
	TypeValue string                                                `json:"type"`
	RawJSON   json.RawMessage                                       `json:"-"`
}

var userVerificationErrorDetailsSpec = objectSpec{
	fields: []objectField{
		{name: "reason", flags: fieldNonNullOptional},
		{name: "type", flags: fieldRequired | fieldRejectNull},
	},
	tag:  1,
	tags: []string{"invalidRequest", "unavailable", "cancelled", "failed"},
}

func (v UserVerificationErrorDetails) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "invalidRequest":
		if !v.Reason.IsSet() {
			return nil, DecodeError{Field: "reason", Reason: "missing required field for type invalidRequest"}
		}
	case "unavailable":
		if !v.Reason.IsSet() {
			return nil, DecodeError{Field: "reason", Reason: "missing required field for type unavailable"}
		}
	case "cancelled":
		if !v.Reason.IsSet() {
			return nil, DecodeError{Field: "reason", Reason: "missing required field for type cancelled"}
		}
	case "failed":
		if !v.Reason.IsSet() {
			return nil, DecodeError{Field: "reason", Reason: "missing required field for type failed"}
		}
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire UserVerificationErrorDetails
	return json.Marshal(wire(v))
}

func (v *UserVerificationErrorDetails) UnmarshalJSON(data []byte) error {
	type wire UserVerificationErrorDetails
	var s objectScan
	known, err := s.begin(data, &userVerificationErrorDetailsSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "invalidRequest":
		if !s.has("reason") {
			return DecodeError{Field: "reason", Reason: "missing required field for type invalidRequest"}
		}
		if s.isNull("reason") {
			return DecodeError{Field: "reason", Reason: "cannot be null"}
		}
	case "unavailable":
		if !s.has("reason") {
			return DecodeError{Field: "reason", Reason: "missing required field for type unavailable"}
		}
		if s.isNull("reason") {
			return DecodeError{Field: "reason", Reason: "cannot be null"}
		}
	case "cancelled":
		if !s.has("reason") {
			return DecodeError{Field: "reason", Reason: "missing required field for type cancelled"}
		}
		if s.isNull("reason") {
			return DecodeError{Field: "reason", Reason: "cannot be null"}
		}
	case "failed":
		if !s.has("reason") {
			return DecodeError{Field: "reason", Reason: "missing required field for type failed"}
		}
		if s.isNull("reason") {
			return DecodeError{Field: "reason", Reason: "cannot be null"}
		}
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type UserVerificationFailureReason string

const (
	UserVerificationFailureReasonAuthenticationFailed UserVerificationFailureReason = "authenticationFailed"
	UserVerificationFailureReasonTimeout              UserVerificationFailureReason = "timeout"
	UserVerificationFailureReasonProviderError        UserVerificationFailureReason = "providerError"
	UserVerificationFailureReasonServiceError         UserVerificationFailureReason = "serviceError"
)

type UserVerificationInvalidRequestReason string

const (
	UserVerificationInvalidRequestReasonInvalidParams UserVerificationInvalidRequestReason = "invalidParams"
)

type UserVerificationProof struct {
	CredentialID string `json:"credentialId"`
	Signature    string `json:"signature"`
}

var userVerificationProofSpec = objectSpec{
	fields: []objectField{
		{name: "credentialId", flags: fieldRequired | fieldRejectNull},
		{name: "signature", flags: fieldRequired | fieldRejectNull},
	},
}

func (v UserVerificationProof) MarshalJSON() ([]byte, error) {
	type wire UserVerificationProof
	return json.Marshal(wire(v))
}

func (v *UserVerificationProof) UnmarshalJSON(data []byte) error {
	type wire UserVerificationProof
	return decodeObject(data, &userVerificationProofSpec, (*wire)(v))
}

type UserVerificationRPCError struct {
	Code    int64                        `json:"code"`
	Data    UserVerificationErrorDetails `json:"data"`
	Message string                       `json:"message"`
}

var userVerificationRPCErrorSpec = objectSpec{
	fields: []objectField{
		{name: "code", flags: fieldRequired | fieldRejectNull},
		{name: "data", flags: fieldRequired | fieldRejectNull},
		{name: "message", flags: fieldRequired | fieldRejectNull},
	},
}

func (v UserVerificationRPCError) MarshalJSON() ([]byte, error) {
	type wire UserVerificationRPCError
	return json.Marshal(wire(v))
}

func (v *UserVerificationRPCError) UnmarshalJSON(data []byte) error {
	type wire UserVerificationRPCError
	return decodeObject(data, &userVerificationRPCErrorSpec, (*wire)(v))
}

type UserVerificationStatusParams struct {
}

var userVerificationStatusParamsSpec = objectSpec{
	fields: []objectField{},
}

func (v UserVerificationStatusParams) MarshalJSON() ([]byte, error) {
	type wire UserVerificationStatusParams
	return json.Marshal(wire(v))
}

func (v *UserVerificationStatusParams) UnmarshalJSON(data []byte) error {
	type wire UserVerificationStatusParams
	return decodeObject(data, &userVerificationStatusParamsSpec, (*wire)(v))
}

type UserVerificationStatusResponse struct {
	CredentialID       Optional[string]                            `json:"credentialId,omitzero"`
	UnavailableMessage Optional[string]                            `json:"unavailableMessage,omitzero"`
	UnavailableReason  Optional[UserVerificationUnavailableReason] `json:"unavailableReason,omitzero"`
}

var userVerificationStatusResponseSpec = objectSpec{
	fields: []objectField{
		{name: "credentialId"},
		{name: "unavailableMessage"},
		{name: "unavailableReason"},
	},
}

func (v UserVerificationStatusResponse) MarshalJSON() ([]byte, error) {
	type wire UserVerificationStatusResponse
	return json.Marshal(wire(v))
}

func (v *UserVerificationStatusResponse) UnmarshalJSON(data []byte) error {
	type wire UserVerificationStatusResponse
	return decodeObject(data, &userVerificationStatusResponseSpec, (*wire)(v))
}

type UserVerificationUnavailableReason string

const (
	UserVerificationUnavailableReasonCredentialMissing     UserVerificationUnavailableReason = "credentialMissing"
	UserVerificationUnavailableReasonBiometricsUnavailable UserVerificationUnavailableReason = "biometricsUnavailable"
	UserVerificationUnavailableReasonProviderUnavailable   UserVerificationUnavailableReason = "providerUnavailable"
)

type UserVerificationVerifyParams struct {
	Challenge   string `json:"challenge"`
	Description string `json:"description"`
	Title       string `json:"title"`
}

var userVerificationVerifyParamsSpec = objectSpec{
	fields: []objectField{
		{name: "challenge", flags: fieldRequired | fieldRejectNull},
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "title", flags: fieldRequired | fieldRejectNull},
	},
}

func (v UserVerificationVerifyParams) MarshalJSON() ([]byte, error) {
	type wire UserVerificationVerifyParams
	return json.Marshal(wire(v))
}

func (v *UserVerificationVerifyParams) UnmarshalJSON(data []byte) error {
	type wire UserVerificationVerifyParams
	return decodeObject(data, &userVerificationVerifyParamsSpec, (*wire)(v))
}

type UserVerificationVerifyResponse struct {
	Proof UserVerificationProof `json:"proof"`
}

var userVerificationVerifyResponseSpec = objectSpec{
	fields: []objectField{
		{name: "proof", flags: fieldRequired | fieldRejectNull},
	},
}

func (v UserVerificationVerifyResponse) MarshalJSON() ([]byte, error) {
	type wire UserVerificationVerifyResponse
	return json.Marshal(wire(v))
}

func (v *UserVerificationVerifyResponse) UnmarshalJSON(data []byte) error {
	type wire UserVerificationVerifyResponse
	return decodeObject(data, &userVerificationVerifyResponseSpec, (*wire)(v))
}

type Verbosity string

const (
	VerbosityLow    Verbosity = "low"
	VerbosityMedium Verbosity = "medium"
	VerbosityHigh   Verbosity = "high"
)

type WarningNotification struct {
	Message  string           `json:"message"`
	ThreadID Optional[string] `json:"threadId,omitzero"`
}

var warningNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "message", flags: fieldRequired | fieldRejectNull},
		{name: "threadId"},
	},
}

func (v WarningNotification) MarshalJSON() ([]byte, error) {
	type wire WarningNotification
	return json.Marshal(wire(v))
}

func (v *WarningNotification) UnmarshalJSON(data []byte) error {
	type wire WarningNotification
	return decodeObject(data, &warningNotificationSpec, (*wire)(v))
}

type WebSearchAction struct {
	Pattern   Optional[string]   `json:"pattern,omitzero"`
	Queries   Optional[[]string] `json:"queries,omitzero"`
	Query     Optional[string]   `json:"query,omitzero"`
	TypeValue string             `json:"type"`
	URL       Optional[string]   `json:"url,omitzero"`
	RawJSON   json.RawMessage    `json:"-"`
}

var webSearchActionSpec = objectSpec{
	fields: []objectField{
		{name: "pattern"},
		{name: "queries"},
		{name: "query"},
		{name: "type", flags: fieldRequired | fieldRejectNull},
		{name: "url"},
	},
	tag:  3,
	tags: []string{"search", "openPage", "findInPage", "other"},
}

func (v WebSearchAction) MarshalJSON() ([]byte, error) {
	switch v.TypeValue {
	case "search":
	case "openPage":
	case "findInPage":
	case "other":
	default:
		if len(v.RawJSON) > 0 {
			return append([]byte(nil), v.RawJSON...), nil
		}
		return nil, DecodeError{Field: "type", Reason: fmt.Sprintf("unsupported discriminator value %q", v.TypeValue)}
	}
	type wire WebSearchAction
	return json.Marshal(wire(v))
}

func (v *WebSearchAction) UnmarshalJSON(data []byte) error {
	type wire WebSearchAction
	var s objectScan
	known, err := s.begin(data, &webSearchActionSpec)
	if err != nil {
		return err
	}
	if !known {
		v.TypeValue = s.tag
		v.RawJSON = append(v.RawJSON[:0], data...)
		return nil
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	v.RawJSON = nil
	switch v.TypeValue {
	case "search":
	case "openPage":
	case "findInPage":
	case "other":
	default:
		v.RawJSON = append(v.RawJSON[:0], data...)
	}
	return nil
}

type WebSearchContextSize string

const (
	WebSearchContextSizeLow    WebSearchContextSize = "low"
	WebSearchContextSizeMedium WebSearchContextSize = "medium"
	WebSearchContextSizeHigh   WebSearchContextSize = "high"
)

type WebSearchLocation struct {
	City     Optional[string] `json:"city,omitzero"`
	Country  Optional[string] `json:"country,omitzero"`
	Region   Optional[string] `json:"region,omitzero"`
	Timezone Optional[string] `json:"timezone,omitzero"`
}

var webSearchLocationSpec = objectSpec{
	fields: []objectField{
		{name: "city"},
		{name: "country"},
		{name: "region"},
		{name: "timezone"},
	},
}

func (v WebSearchLocation) MarshalJSON() ([]byte, error) {
	type wire WebSearchLocation
	return json.Marshal(wire(v))
}

func (v *WebSearchLocation) UnmarshalJSON(data []byte) error {
	type wire WebSearchLocation
	return decodeObject(data, &webSearchLocationSpec, (*wire)(v))
}

type WebSearchMode string

const (
	WebSearchModeDisabled WebSearchMode = "disabled"
	WebSearchModeCached   WebSearchMode = "cached"
	WebSearchModeIndexed  WebSearchMode = "indexed"
	WebSearchModeLive     WebSearchMode = "live"
)

type WebSearchToolConfig struct {
	AllowedDomains Optional[[]string]             `json:"allowed_domains,omitzero"`
	ContextSize    Optional[WebSearchContextSize] `json:"context_size,omitzero"`
	Location       Optional[WebSearchLocation]    `json:"location,omitzero"`
}

var webSearchToolConfigSpec = objectSpec{
	fields: []objectField{
		{name: "allowed_domains"},
		{name: "context_size"},
		{name: "location"},
	},
}

func (v WebSearchToolConfig) MarshalJSON() ([]byte, error) {
	type wire WebSearchToolConfig
	return json.Marshal(wire(v))
}

func (v *WebSearchToolConfig) UnmarshalJSON(data []byte) error {
	type wire WebSearchToolConfig
	return decodeObject(data, &webSearchToolConfigSpec, (*wire)(v))
}

type WindowsSandboxImplementation string

const (
	WindowsSandboxImplementationElevated   WindowsSandboxImplementation = "elevated"
	WindowsSandboxImplementationUnelevated WindowsSandboxImplementation = "unelevated"
	WindowsSandboxImplementationMxc        WindowsSandboxImplementation = "mxc"
)

type WindowsSandboxReadiness string

const (
	WindowsSandboxReadinessReady          WindowsSandboxReadiness = "ready"
	WindowsSandboxReadinessNotConfigured  WindowsSandboxReadiness = "notConfigured"
	WindowsSandboxReadinessUpdateRequired WindowsSandboxReadiness = "updateRequired"
)

type WindowsSandboxReadinessResponse struct {
	Status WindowsSandboxReadiness `json:"status"`
}

var windowsSandboxReadinessResponseSpec = objectSpec{
	fields: []objectField{
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WindowsSandboxReadinessResponse) MarshalJSON() ([]byte, error) {
	type wire WindowsSandboxReadinessResponse
	return json.Marshal(wire(v))
}

func (v *WindowsSandboxReadinessResponse) UnmarshalJSON(data []byte) error {
	type wire WindowsSandboxReadinessResponse
	return decodeObject(data, &windowsSandboxReadinessResponseSpec, (*wire)(v))
}

type WindowsSandboxSetupCompletedNotification struct {
	Error   Optional[string]        `json:"error,omitzero"`
	Mode    WindowsSandboxSetupMode `json:"mode"`
	Success bool                    `json:"success"`
}

var windowsSandboxSetupCompletedNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "error"},
		{name: "mode", flags: fieldRequired | fieldRejectNull},
		{name: "success", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WindowsSandboxSetupCompletedNotification) MarshalJSON() ([]byte, error) {
	type wire WindowsSandboxSetupCompletedNotification
	return json.Marshal(wire(v))
}

func (v *WindowsSandboxSetupCompletedNotification) UnmarshalJSON(data []byte) error {
	type wire WindowsSandboxSetupCompletedNotification
	return decodeObject(data, &windowsSandboxSetupCompletedNotificationSpec, (*wire)(v))
}

type WindowsSandboxSetupMode string

const (
	WindowsSandboxSetupModeElevated   WindowsSandboxSetupMode = "elevated"
	WindowsSandboxSetupModeUnelevated WindowsSandboxSetupMode = "unelevated"
)

type WindowsSandboxSetupStartParams struct {
	Cwd  Optional[AbsolutePathBuf] `json:"cwd,omitzero"`
	Mode WindowsSandboxSetupMode   `json:"mode"`
}

var windowsSandboxSetupStartParamsSpec = objectSpec{
	fields: []objectField{
		{name: "cwd"},
		{name: "mode", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WindowsSandboxSetupStartParams) MarshalJSON() ([]byte, error) {
	type wire WindowsSandboxSetupStartParams
	return json.Marshal(wire(v))
}

func (v *WindowsSandboxSetupStartParams) UnmarshalJSON(data []byte) error {
	type wire WindowsSandboxSetupStartParams
	return decodeObject(data, &windowsSandboxSetupStartParamsSpec, (*wire)(v))
}

type WindowsSandboxSetupStartResponse struct {
	Started bool `json:"started"`
}

var windowsSandboxSetupStartResponseSpec = objectSpec{
	fields: []objectField{
		{name: "started", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WindowsSandboxSetupStartResponse) MarshalJSON() ([]byte, error) {
	type wire WindowsSandboxSetupStartResponse
	return json.Marshal(wire(v))
}

func (v *WindowsSandboxSetupStartResponse) UnmarshalJSON(data []byte) error {
	type wire WindowsSandboxSetupStartResponse
	return decodeObject(data, &windowsSandboxSetupStartResponseSpec, (*wire)(v))
}

type WindowsWorldWritableWarningNotification struct {
	ExtraCount  uint64   `json:"extraCount"`
	FailedScan  bool     `json:"failedScan"`
	SamplePaths []string `json:"samplePaths"`
}

var windowsWorldWritableWarningNotificationSpec = objectSpec{
	fields: []objectField{
		{name: "extraCount", flags: fieldRequired | fieldRejectNull},
		{name: "failedScan", flags: fieldRequired | fieldRejectNull},
		{name: "samplePaths", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WindowsWorldWritableWarningNotification) MarshalJSON() ([]byte, error) {
	type wire WindowsWorldWritableWarningNotification
	return json.Marshal(wire(v))
}

func (v *WindowsWorldWritableWarningNotification) UnmarshalJSON(data []byte) error {
	type wire WindowsWorldWritableWarningNotification
	var s objectScan
	if _, err := s.begin(data, &windowsWorldWritableWarningNotificationSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.ExtraCount < 0 {
		return DecodeError{Field: "extraCount", Reason: "below minimum 0"}
	}
	return nil
}

type WorkflowAgent struct {
	AgentPath     Optional[string]      `json:"agentPath,omitzero"`
	Cached        OptionalNonNull[bool] `json:"cached,omitzero"`
	CachedTokens  uint64                `json:"cachedTokens"`
	ChargedTokens uint64                `json:"chargedTokens"`
	Label         Optional[string]      `json:"label,omitzero"`
	Phase         Optional[string]      `json:"phase,omitzero"`
	Reason        Optional[string]      `json:"reason,omitzero"`
	Seq           uint64                `json:"seq"`
	State         WorkflowAgentState    `json:"state"`
	ThreadID      Optional[string]      `json:"threadId,omitzero"`
}

var workflowAgentSpec = objectSpec{
	fields: []objectField{
		{name: "agentPath"},
		{name: "cached", flags: fieldNonNullOptional},
		{name: "cachedTokens", flags: fieldRequired | fieldRejectNull},
		{name: "chargedTokens", flags: fieldRequired | fieldRejectNull},
		{name: "label"},
		{name: "phase"},
		{name: "reason"},
		{name: "seq", flags: fieldRequired | fieldRejectNull},
		{name: "state", flags: fieldRequired | fieldRejectNull},
		{name: "threadId"},
	},
}

func (v WorkflowAgent) MarshalJSON() ([]byte, error) {
	type wire WorkflowAgent
	return json.Marshal(wire(v))
}

func (v *WorkflowAgent) UnmarshalJSON(data []byte) error {
	type wire WorkflowAgent
	var s objectScan
	if _, err := s.begin(data, &workflowAgentSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.CachedTokens < 0 {
		return DecodeError{Field: "cachedTokens", Reason: "below minimum 0"}
	}
	if v.ChargedTokens < 0 {
		return DecodeError{Field: "chargedTokens", Reason: "below minimum 0"}
	}
	if v.Seq < 0 {
		return DecodeError{Field: "seq", Reason: "below minimum 0"}
	}
	return nil
}

type WorkflowAgentCounts struct {
	AwaitingApproval uint64 `json:"awaitingApproval"`
	Completed        uint64 `json:"completed"`
	Null             uint64 `json:"null"`
	Queued           uint64 `json:"queued"`
	Running          uint64 `json:"running"`
}

var workflowAgentCountsSpec = objectSpec{
	fields: []objectField{
		{name: "awaitingApproval", flags: fieldRequired | fieldRejectNull},
		{name: "completed", flags: fieldRequired | fieldRejectNull},
		{name: "null", flags: fieldRequired | fieldRejectNull},
		{name: "queued", flags: fieldRequired | fieldRejectNull},
		{name: "running", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WorkflowAgentCounts) MarshalJSON() ([]byte, error) {
	type wire WorkflowAgentCounts
	return json.Marshal(wire(v))
}

func (v *WorkflowAgentCounts) UnmarshalJSON(data []byte) error {
	type wire WorkflowAgentCounts
	var s objectScan
	if _, err := s.begin(data, &workflowAgentCountsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.AwaitingApproval < 0 {
		return DecodeError{Field: "awaitingApproval", Reason: "below minimum 0"}
	}
	if v.Completed < 0 {
		return DecodeError{Field: "completed", Reason: "below minimum 0"}
	}
	if v.Null < 0 {
		return DecodeError{Field: "null", Reason: "below minimum 0"}
	}
	if v.Queued < 0 {
		return DecodeError{Field: "queued", Reason: "below minimum 0"}
	}
	if v.Running < 0 {
		return DecodeError{Field: "running", Reason: "below minimum 0"}
	}
	return nil
}

type WorkflowAgentState = json.RawMessage

type WorkflowPhase struct {
	Finished uint64 `json:"finished"`
	Started  uint64 `json:"started"`
	Title    string `json:"title"`
}

var workflowPhaseSpec = objectSpec{
	fields: []objectField{
		{name: "finished", flags: fieldRequired | fieldRejectNull},
		{name: "started", flags: fieldRequired | fieldRejectNull},
		{name: "title", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WorkflowPhase) MarshalJSON() ([]byte, error) {
	type wire WorkflowPhase
	return json.Marshal(wire(v))
}

func (v *WorkflowPhase) UnmarshalJSON(data []byte) error {
	type wire WorkflowPhase
	var s objectScan
	if _, err := s.begin(data, &workflowPhaseSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Finished < 0 {
		return DecodeError{Field: "finished", Reason: "below minimum 0"}
	}
	if v.Started < 0 {
		return DecodeError{Field: "started", Reason: "below minimum 0"}
	}
	return nil
}

type WorkflowResumeStats struct {
	From          string `json:"from"`
	Served        uint64 `json:"served"`
	SourceResults uint64 `json:"sourceResults"`
	Unused        uint64 `json:"unused"`
}

var workflowResumeStatsSpec = objectSpec{
	fields: []objectField{
		{name: "from", flags: fieldRequired | fieldRejectNull},
		{name: "served", flags: fieldRequired | fieldRejectNull},
		{name: "sourceResults", flags: fieldRequired | fieldRejectNull},
		{name: "unused", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WorkflowResumeStats) MarshalJSON() ([]byte, error) {
	type wire WorkflowResumeStats
	return json.Marshal(wire(v))
}

func (v *WorkflowResumeStats) UnmarshalJSON(data []byte) error {
	type wire WorkflowResumeStats
	var s objectScan
	if _, err := s.begin(data, &workflowResumeStatsSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if v.Served < 0 {
		return DecodeError{Field: "served", Reason: "below minimum 0"}
	}
	if v.SourceResults < 0 {
		return DecodeError{Field: "sourceResults", Reason: "below minimum 0"}
	}
	if v.Unused < 0 {
		return DecodeError{Field: "unused", Reason: "below minimum 0"}
	}
	return nil
}

type WorkflowRun struct {
	AgentCounts   WorkflowAgentCounts           `json:"agentCounts"`
	Agents        Optional[[]WorkflowAgent]     `json:"agents,omitzero"`
	BudgetTokens  Optional[uint64]              `json:"budgetTokens,omitzero"`
	CachedTokens  uint64                        `json:"cachedTokens"`
	ChargedTokens uint64                        `json:"chargedTokens"`
	CurrentPhase  Optional[string]              `json:"currentPhase,omitzero"`
	Description   string                        `json:"description"`
	Error         Optional[string]              `json:"error,omitzero"`
	FinishedAtMs  Optional[int64]               `json:"finishedAtMs,omitzero"`
	LogsTail      []string                      `json:"logsTail"`
	Name          string                        `json:"name"`
	NullReasons   map[string]uint64             `json:"nullReasons"`
	Phases        []WorkflowPhase               `json:"phases"`
	ResultPreview Optional[string]              `json:"resultPreview,omitzero"`
	Resume        Optional[WorkflowResumeStats] `json:"resume,omitzero"`
	Revision      uint64                        `json:"revision"`
	RunID         string                        `json:"runId"`
	StartedAtMs   int64                         `json:"startedAtMs"`
	Status        WorkflowRunStatus             `json:"status"`
}

var workflowRunSpec = objectSpec{
	fields: []objectField{
		{name: "agentCounts", flags: fieldRequired | fieldRejectNull},
		{name: "agents"},
		{name: "budgetTokens"},
		{name: "cachedTokens", flags: fieldRequired | fieldRejectNull},
		{name: "chargedTokens", flags: fieldRequired | fieldRejectNull},
		{name: "currentPhase"},
		{name: "description", flags: fieldRequired | fieldRejectNull},
		{name: "error"},
		{name: "finishedAtMs"},
		{name: "logsTail", flags: fieldRequired | fieldRejectNull},
		{name: "name", flags: fieldRequired | fieldRejectNull},
		{name: "nullReasons", flags: fieldRequired | fieldRejectNull},
		{name: "phases", flags: fieldRequired | fieldRejectNull},
		{name: "resultPreview"},
		{name: "resume"},
		{name: "revision", flags: fieldRequired | fieldRejectNull},
		{name: "runId", flags: fieldRequired | fieldRejectNull},
		{name: "startedAtMs", flags: fieldRequired | fieldRejectNull},
		{name: "status", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WorkflowRun) MarshalJSON() ([]byte, error) {
	type wire WorkflowRun
	return json.Marshal(wire(v))
}

func (v *WorkflowRun) UnmarshalJSON(data []byte) error {
	type wire WorkflowRun
	var s objectScan
	if _, err := s.begin(data, &workflowRunSpec); err != nil {
		return err
	}
	if err := s.finish((*wire)(v)); err != nil {
		return err
	}
	if valueBudgetTokens, ok := v.BudgetTokens.Value(); ok {
		if valueBudgetTokens < 0 {
			return DecodeError{Field: "budgetTokens", Reason: "below minimum 0"}
		}
	}
	if v.CachedTokens < 0 {
		return DecodeError{Field: "cachedTokens", Reason: "below minimum 0"}
	}
	if v.ChargedTokens < 0 {
		return DecodeError{Field: "chargedTokens", Reason: "below minimum 0"}
	}
	if v.Revision < 0 {
		return DecodeError{Field: "revision", Reason: "below minimum 0"}
	}
	return nil
}

type WorkflowRunStatus string

const (
	WorkflowRunStatusRunning     WorkflowRunStatus = "running"
	WorkflowRunStatusCompleted   WorkflowRunStatus = "completed"
	WorkflowRunStatusFailed      WorkflowRunStatus = "failed"
	WorkflowRunStatusCancelled   WorkflowRunStatus = "cancelled"
	WorkflowRunStatusInterrupted WorkflowRunStatus = "interrupted"
)

type WorkspaceMessage struct {
	ArchivedAt  Optional[int64]      `json:"archivedAt,omitzero"`
	CreatedAt   Optional[int64]      `json:"createdAt,omitzero"`
	MessageBody string               `json:"messageBody"`
	MessageID   string               `json:"messageId"`
	MessageType WorkspaceMessageType `json:"messageType"`
}

var workspaceMessageSpec = objectSpec{
	fields: []objectField{
		{name: "archivedAt"},
		{name: "createdAt"},
		{name: "messageBody", flags: fieldRequired | fieldRejectNull},
		{name: "messageId", flags: fieldRequired | fieldRejectNull},
		{name: "messageType", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WorkspaceMessage) MarshalJSON() ([]byte, error) {
	type wire WorkspaceMessage
	return json.Marshal(wire(v))
}

func (v *WorkspaceMessage) UnmarshalJSON(data []byte) error {
	type wire WorkspaceMessage
	return decodeObject(data, &workspaceMessageSpec, (*wire)(v))
}

type WorkspaceMessageType string

const (
	WorkspaceMessageTypeHeadline     WorkspaceMessageType = "headline"
	WorkspaceMessageTypeAnnouncement WorkspaceMessageType = "announcement"
	WorkspaceMessageTypeUnknown      WorkspaceMessageType = "unknown"
)

type WorkspaceRouting struct {
	AccountRoutingOverride AccountRoutingOverride `json:"accountRoutingOverride"`
	BackendOrigin          string                 `json:"backendOrigin"`
	ChatgptAccountID       string                 `json:"chatgptAccountId"`
}

var workspaceRoutingSpec = objectSpec{
	fields: []objectField{
		{name: "accountRoutingOverride", flags: fieldRequired | fieldRejectNull},
		{name: "backendOrigin", flags: fieldRequired | fieldRejectNull},
		{name: "chatgptAccountId", flags: fieldRequired | fieldRejectNull},
	},
}

func (v WorkspaceRouting) MarshalJSON() ([]byte, error) {
	type wire WorkspaceRouting
	return json.Marshal(wire(v))
}

func (v *WorkspaceRouting) UnmarshalJSON(data []byte) error {
	type wire WorkspaceRouting
	return decodeObject(data, &workspaceRoutingSpec, (*wire)(v))
}

type WriteStatus string

const (
	WriteStatusOk           WriteStatus = "ok"
	WriteStatusOkOverridden WriteStatus = "okOverridden"
)
