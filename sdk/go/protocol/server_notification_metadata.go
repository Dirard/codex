package protocol

import "encoding/json"

type ServerNotificationIdentityExtractor struct {
	FieldPath             string
	IdentityName          string
	Optional              bool
	TerminalPredicateJSON string
}

type ServerNotificationRouteMetadata struct {
	ResourceDomain     string
	WireIdentitySource string
	IdentityExtractors []ServerNotificationIdentityExtractor
}

type ServerNotificationRoutingMetadata struct {
	Method                  string
	PayloadType             string
	PayloadSchemaRef        string
	Visibility              string
	SchemaExcludedReason    string
	ManualPayloadConversion string
	RoutingKind             string
	Routes                  []ServerNotificationRouteMetadata
	Experimental            bool
}

type LifecycleTriggerMetadata struct {
	Kind      string
	Method    string
	Predicate string
}

type RoutingLifecycleMetadata struct {
	ResourceDomain                 string
	StartMethod                    string
	WireIdentitySource             string
	StartCompletion                LifecycleTriggerMetadata
	CleanupTriggers                []LifecycleTriggerMetadata
	NotificationOptOutDependencies []string
}

var ServerNotificationRoutingByMethod = map[string]ServerNotificationRoutingMetadata{
	"error":                                     {Method: "error", PayloadType: "ErrorNotification", PayloadSchemaRef: "#/definitions/ErrorNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "error", WireIdentitySource: "error", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/started":                            {Method: "thread/started", PayloadType: "ThreadStartedNotification", PayloadSchemaRef: "#/definitions/ThreadStartedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/started", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "thread.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/status/changed":                     {Method: "thread/status/changed", PayloadType: "ThreadStatusChangedNotification", PayloadSchemaRef: "#/definitions/ThreadStatusChangedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/status/changed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/archived":                           {Method: "thread/archived", PayloadType: "ThreadArchivedNotification", PayloadSchemaRef: "#/definitions/ThreadArchivedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/archived", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/deleted":                            {Method: "thread/deleted", PayloadType: "ThreadDeletedNotification", PayloadSchemaRef: "#/definitions/ThreadDeletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/deleted", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/unarchived":                         {Method: "thread/unarchived", PayloadType: "ThreadUnarchivedNotification", PayloadSchemaRef: "#/definitions/ThreadUnarchivedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/unarchived", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/closed":                             {Method: "thread/closed", PayloadType: "ThreadClosedNotification", PayloadSchemaRef: "#/definitions/ThreadClosedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/closed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"skills/changed":                            {Method: "skills/changed", PayloadType: "SkillsChangedNotification", PayloadSchemaRef: "#/definitions/SkillsChangedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"thread/name/updated":                       {Method: "thread/name/updated", PayloadType: "ThreadNameUpdatedNotification", PayloadSchemaRef: "#/definitions/ThreadNameUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/name/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/goal/updated":                       {Method: "thread/goal/updated", PayloadType: "ThreadGoalUpdatedNotification", PayloadSchemaRef: "#/definitions/ThreadGoalUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/goal/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: true, TerminalPredicateJSON: ""}}}}},
	"thread/goal/cleared":                       {Method: "thread/goal/cleared", PayloadType: "ThreadGoalClearedNotification", PayloadSchemaRef: "#/definitions/ThreadGoalClearedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/goal/cleared", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/settings/updated":                   {Method: "thread/settings/updated", PayloadType: "ThreadSettingsUpdatedNotification", PayloadSchemaRef: "#/definitions/ThreadSettingsUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/settings/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/tokenUsage/updated":                 {Method: "thread/tokenUsage/updated", PayloadType: "ThreadTokenUsageUpdatedNotification", PayloadSchemaRef: "#/definitions/ThreadTokenUsageUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/tokenUsage/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"turn/started":                              {Method: "turn/started", PayloadType: "TurnStartedNotification", PayloadSchemaRef: "#/definitions/TurnStartedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "turn", WireIdentitySource: "turn/started", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turn.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"hook/started":                              {Method: "hook/started", PayloadType: "HookStartedNotification", PayloadSchemaRef: "#/definitions/HookStartedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "hook", WireIdentitySource: "hook/started", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "run.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"turn/completed":                            {Method: "turn/completed", PayloadType: "TurnCompletedNotification", PayloadSchemaRef: "#/definitions/TurnCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "turn", WireIdentitySource: "turn/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turn.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"hook/completed":                            {Method: "hook/completed", PayloadType: "HookCompletedNotification", PayloadSchemaRef: "#/definitions/HookCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "hook", WireIdentitySource: "hook/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "run.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"turn/diff/updated":                         {Method: "turn/diff/updated", PayloadType: "TurnDiffUpdatedNotification", PayloadSchemaRef: "#/definitions/TurnDiffUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "turn", WireIdentitySource: "turn/diff/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"turn/plan/updated":                         {Method: "turn/plan/updated", PayloadType: "TurnPlanUpdatedNotification", PayloadSchemaRef: "#/definitions/TurnPlanUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "turn", WireIdentitySource: "turn/plan/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/started":                              {Method: "item/started", PayloadType: "ItemStartedNotification", PayloadSchemaRef: "#/definitions/ItemStartedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/started", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "item.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/autoApprovalReview/started":           {Method: "item/autoApprovalReview/started", PayloadType: "ItemGuardianApprovalReviewStartedNotification", PayloadSchemaRef: "#/definitions/ItemGuardianApprovalReviewStartedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/autoApprovalReview/started", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "reviewId", IdentityName: "reviewId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "targetItemId", IdentityName: "targetItemId", Optional: true, TerminalPredicateJSON: ""}}}}},
	"item/autoApprovalReview/completed":         {Method: "item/autoApprovalReview/completed", PayloadType: "ItemGuardianApprovalReviewCompletedNotification", PayloadSchemaRef: "#/definitions/ItemGuardianApprovalReviewCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/autoApprovalReview/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "reviewId", IdentityName: "reviewId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "targetItemId", IdentityName: "targetItemId", Optional: true, TerminalPredicateJSON: ""}}}}},
	"item/completed":                            {Method: "item/completed", PayloadType: "ItemCompletedNotification", PayloadSchemaRef: "#/definitions/ItemCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "item.id", IdentityName: "id", Optional: false, TerminalPredicateJSON: ""}}}}},
	"rawResponseItem/completed":                 {Method: "rawResponseItem/completed", PayloadType: "RawResponseItemCompletedNotification", PayloadSchemaRef: "#/definitions/RawResponseItemCompletedNotification", Visibility: "generatedOnly", SchemaExcludedReason: "raw response item completion is stripped from the generated JSON ServerNotification method union", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "rawResponseItem", WireIdentitySource: "rawResponseItem/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/agentMessage/delta":                   {Method: "item/agentMessage/delta", PayloadType: "AgentMessageDeltaNotification", PayloadSchemaRef: "#/definitions/AgentMessageDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/agentMessage/delta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/plan/delta":                           {Method: "item/plan/delta", PayloadType: "PlanDeltaNotification", PayloadSchemaRef: "#/definitions/PlanDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/plan/delta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"command/exec/outputDelta":                  {Method: "command/exec/outputDelta", PayloadType: "CommandExecOutputDeltaNotification", PayloadSchemaRef: "#/definitions/CommandExecOutputDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "command", WireIdentitySource: "command/exec/outputDelta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "processId", IdentityName: "processId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"process/outputDelta":                       {Method: "process/outputDelta", PayloadType: "ProcessOutputDeltaNotification", PayloadSchemaRef: "#/definitions/ProcessOutputDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "process", WireIdentitySource: "process/outputDelta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "processHandle", IdentityName: "processHandle", Optional: false, TerminalPredicateJSON: ""}}}}},
	"process/exited":                            {Method: "process/exited", PayloadType: "ProcessExitedNotification", PayloadSchemaRef: "#/definitions/ProcessExitedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "process", WireIdentitySource: "process/exited", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "processHandle", IdentityName: "processHandle", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/commandExecution/outputDelta":         {Method: "item/commandExecution/outputDelta", PayloadType: "CommandExecutionOutputDeltaNotification", PayloadSchemaRef: "#/definitions/CommandExecutionOutputDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/commandExecution/outputDelta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/commandExecution/terminalInteraction": {Method: "item/commandExecution/terminalInteraction", PayloadType: "TerminalInteractionNotification", PayloadSchemaRef: "#/definitions/TerminalInteractionNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/commandExecution/terminalInteraction", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "processId", IdentityName: "processId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/fileChange/outputDelta":               {Method: "item/fileChange/outputDelta", PayloadType: "FileChangeOutputDeltaNotification", PayloadSchemaRef: "#/definitions/FileChangeOutputDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/fileChange/outputDelta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/fileChange/patchUpdated":              {Method: "item/fileChange/patchUpdated", PayloadType: "FileChangePatchUpdatedNotification", PayloadSchemaRef: "#/definitions/FileChangePatchUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/fileChange/patchUpdated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"serverRequest/resolved":                    {Method: "serverRequest/resolved", PayloadType: "ServerRequestResolvedNotification", PayloadSchemaRef: "#/definitions/ServerRequestResolvedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "serverRequest", WireIdentitySource: "serverRequest/resolved", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "requestId", IdentityName: "requestId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/mcpToolCall/progress":                 {Method: "item/mcpToolCall/progress", PayloadType: "McpToolCallProgressNotification", PayloadSchemaRef: "#/definitions/McpToolCallProgressNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/mcpToolCall/progress", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"mcpServer/oauthLogin/completed":            {Method: "mcpServer/oauthLogin/completed", PayloadType: "McpServerOauthLoginCompletedNotification", PayloadSchemaRef: "#/definitions/McpServerOauthLoginCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "mcpServer", WireIdentitySource: "mcpServer/oauthLogin/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "name", IdentityName: "name", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "threadId", IdentityName: "threadId", Optional: true, TerminalPredicateJSON: ""}}}}},
	"mcpServer/startupStatus/updated":           {Method: "mcpServer/startupStatus/updated", PayloadType: "McpServerStatusUpdatedNotification", PayloadSchemaRef: "#/definitions/McpServerStatusUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "mcpServer", WireIdentitySource: "mcpServer/startupStatus/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "name", IdentityName: "name", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "threadId", IdentityName: "threadId", Optional: true, TerminalPredicateJSON: ""}}}}},
	"account/updated":                           {Method: "account/updated", PayloadType: "AccountUpdatedNotification", PayloadSchemaRef: "#/definitions/AccountUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"account/rateLimits/updated":                {Method: "account/rateLimits/updated", PayloadType: "AccountRateLimitsUpdatedNotification", PayloadSchemaRef: "#/definitions/AccountRateLimitsUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"app/list/updated":                          {Method: "app/list/updated", PayloadType: "AppListUpdatedNotification", PayloadSchemaRef: "#/definitions/AppListUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"remoteControl/status/changed":              {Method: "remoteControl/status/changed", PayloadType: "RemoteControlStatusChangedNotification", PayloadSchemaRef: "#/definitions/RemoteControlStatusChangedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "remoteControl", WireIdentitySource: "remoteControl/status/changed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "environmentId", IdentityName: "environmentId", Optional: true, TerminalPredicateJSON: ""}, {FieldPath: "installationId", IdentityName: "installationId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "serverName", IdentityName: "serverName", Optional: false, TerminalPredicateJSON: ""}}}}},
	"externalAgentConfig/import/progress":       {Method: "externalAgentConfig/import/progress", PayloadType: "ExternalAgentConfigImportProgressNotification", PayloadSchemaRef: "#/definitions/ExternalAgentConfigImportProgressNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "externalAgentConfig", WireIdentitySource: "externalAgentConfig/import/progress", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "importId", IdentityName: "importId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"externalAgentConfig/import/completed":      {Method: "externalAgentConfig/import/completed", PayloadType: "ExternalAgentConfigImportCompletedNotification", PayloadSchemaRef: "#/definitions/ExternalAgentConfigImportCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "externalAgentConfig", WireIdentitySource: "externalAgentConfig/import/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "importId", IdentityName: "importId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"fs/changed":                                {Method: "fs/changed", PayloadType: "FsChangedNotification", PayloadSchemaRef: "#/definitions/FsChangedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "fs", WireIdentitySource: "fs/changed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "watchId", IdentityName: "watchId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/reasoning/summaryTextDelta":           {Method: "item/reasoning/summaryTextDelta", PayloadType: "ReasoningSummaryTextDeltaNotification", PayloadSchemaRef: "#/definitions/ReasoningSummaryTextDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/reasoning/summaryTextDelta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "summaryIndex", IdentityName: "summaryIndex", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/reasoning/summaryPartAdded":           {Method: "item/reasoning/summaryPartAdded", PayloadType: "ReasoningSummaryPartAddedNotification", PayloadSchemaRef: "#/definitions/ReasoningSummaryPartAddedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/reasoning/summaryPartAdded", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "summaryIndex", IdentityName: "summaryIndex", Optional: false, TerminalPredicateJSON: ""}}}}},
	"item/reasoning/textDelta":                  {Method: "item/reasoning/textDelta", PayloadType: "ReasoningTextDeltaNotification", PayloadSchemaRef: "#/definitions/ReasoningTextDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "item", WireIdentitySource: "item/reasoning/textDelta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "itemId", IdentityName: "itemId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "contentIndex", IdentityName: "contentIndex", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/compacted":                          {Method: "thread/compacted", PayloadType: "ContextCompactedNotification", PayloadSchemaRef: "#/definitions/ContextCompactedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/compacted", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"model/rerouted":                            {Method: "model/rerouted", PayloadType: "ModelReroutedNotification", PayloadSchemaRef: "#/definitions/ModelReroutedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "model", WireIdentitySource: "model/rerouted", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"model/verification":                        {Method: "model/verification", PayloadType: "ModelVerificationNotification", PayloadSchemaRef: "#/definitions/ModelVerificationNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "model", WireIdentitySource: "model/verification", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"turn/moderationMetadata":                   {Method: "turn/moderationMetadata", PayloadType: "TurnModerationMetadataNotification", PayloadSchemaRef: "#/definitions/TurnModerationMetadataNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "turn", WireIdentitySource: "turn/moderationMetadata", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"model/safetyBuffering/updated":             {Method: "model/safetyBuffering/updated", PayloadType: "ModelSafetyBufferingUpdatedNotification", PayloadSchemaRef: "#/definitions/ModelSafetyBufferingUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "model", WireIdentitySource: "model/safetyBuffering/updated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "turnId", IdentityName: "turnId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"warning":                                   {Method: "warning", PayloadType: "WarningNotification", PayloadSchemaRef: "#/definitions/WarningNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routedWithGlobalFallback", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "warning", WireIdentitySource: "warning", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: true, TerminalPredicateJSON: ""}}}}},
	"guardianWarning":                           {Method: "guardianWarning", PayloadType: "GuardianWarningNotification", PayloadSchemaRef: "#/definitions/GuardianWarningNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "guardianWarning", WireIdentitySource: "guardianWarning", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"deprecationNotice":                         {Method: "deprecationNotice", PayloadType: "DeprecationNoticeNotification", PayloadSchemaRef: "#/definitions/DeprecationNoticeNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"configWarning":                             {Method: "configWarning", PayloadType: "ConfigWarningNotification", PayloadSchemaRef: "#/definitions/ConfigWarningNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"fuzzyFileSearch/sessionUpdated":            {Method: "fuzzyFileSearch/sessionUpdated", PayloadType: "FuzzyFileSearchSessionUpdatedNotification", PayloadSchemaRef: "#/definitions/FuzzyFileSearchSessionUpdatedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "fuzzyFileSearch", WireIdentitySource: "fuzzyFileSearch/sessionUpdated", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "sessionId", IdentityName: "sessionId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"fuzzyFileSearch/sessionCompleted":          {Method: "fuzzyFileSearch/sessionCompleted", PayloadType: "FuzzyFileSearchSessionCompletedNotification", PayloadSchemaRef: "#/definitions/FuzzyFileSearchSessionCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "fuzzyFileSearch", WireIdentitySource: "fuzzyFileSearch/sessionCompleted", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "sessionId", IdentityName: "sessionId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/started":                   {Method: "thread/realtime/started", PayloadType: "ThreadRealtimeStartedNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeStartedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/started", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/itemAdded":                 {Method: "thread/realtime/itemAdded", PayloadType: "ThreadRealtimeItemAddedNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeItemAddedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/itemAdded", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/transcript/delta":          {Method: "thread/realtime/transcript/delta", PayloadType: "ThreadRealtimeTranscriptDeltaNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeTranscriptDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/transcript/delta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "role", IdentityName: "role", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/transcript/done":           {Method: "thread/realtime/transcript/done", PayloadType: "ThreadRealtimeTranscriptDoneNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeTranscriptDoneNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/transcript/done", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}, {FieldPath: "role", IdentityName: "role", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/outputAudio/delta":         {Method: "thread/realtime/outputAudio/delta", PayloadType: "ThreadRealtimeOutputAudioDeltaNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeOutputAudioDeltaNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/outputAudio/delta", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/sdp":                       {Method: "thread/realtime/sdp", PayloadType: "ThreadRealtimeSdpNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeSdpNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/sdp", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/error":                     {Method: "thread/realtime/error", PayloadType: "ThreadRealtimeErrorNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeErrorNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/error", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"thread/realtime/closed":                    {Method: "thread/realtime/closed", PayloadType: "ThreadRealtimeClosedNotification", PayloadSchemaRef: "#/definitions/ThreadRealtimeClosedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routed", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "thread", WireIdentitySource: "thread/realtime/closed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "threadId", IdentityName: "threadId", Optional: false, TerminalPredicateJSON: ""}}}}},
	"windows/worldWritableWarning":              {Method: "windows/worldWritableWarning", PayloadType: "WindowsWorldWritableWarningNotification", PayloadSchemaRef: "#/definitions/WindowsWorldWritableWarningNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"windowsSandbox/setupCompleted":             {Method: "windowsSandbox/setupCompleted", PayloadType: "WindowsSandboxSetupCompletedNotification", PayloadSchemaRef: "#/definitions/WindowsSandboxSetupCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "globalOnly", Experimental: false, Routes: []ServerNotificationRouteMetadata{}},
	"account/login/completed":                   {Method: "account/login/completed", PayloadType: "AccountLoginCompletedNotification", PayloadSchemaRef: "#/definitions/AccountLoginCompletedNotification", Visibility: "public", SchemaExcludedReason: "", ManualPayloadConversion: "", RoutingKind: "routedWithGlobalFallback", Experimental: false, Routes: []ServerNotificationRouteMetadata{{ResourceDomain: "account", WireIdentitySource: "account/login/completed", IdentityExtractors: []ServerNotificationIdentityExtractor{{FieldPath: "loginId", IdentityName: "loginId", Optional: true, TerminalPredicateJSON: ""}}}}},
}

func DecodeServerNotificationPayload(method string, params json.RawMessage) (any, error) {
	switch method {
	case "error":
		var payload ErrorNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/started":
		var payload ThreadStartedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/status/changed":
		var payload ThreadStatusChangedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/archived":
		var payload ThreadArchivedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/deleted":
		var payload ThreadDeletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/unarchived":
		var payload ThreadUnarchivedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/closed":
		var payload ThreadClosedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "skills/changed":
		var payload SkillsChangedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/name/updated":
		var payload ThreadNameUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/goal/updated":
		var payload ThreadGoalUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/goal/cleared":
		var payload ThreadGoalClearedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/settings/updated":
		var payload ThreadSettingsUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/tokenUsage/updated":
		var payload ThreadTokenUsageUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "turn/started":
		var payload TurnStartedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "hook/started":
		var payload HookStartedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "turn/completed":
		var payload TurnCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "hook/completed":
		var payload HookCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "turn/diff/updated":
		var payload TurnDiffUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "turn/plan/updated":
		var payload TurnPlanUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/started":
		var payload ItemStartedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/autoApprovalReview/started":
		var payload ItemGuardianApprovalReviewStartedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/autoApprovalReview/completed":
		var payload ItemGuardianApprovalReviewCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/completed":
		var payload ItemCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "rawResponseItem/completed":
		var payload RawResponseItemCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/agentMessage/delta":
		var payload AgentMessageDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/plan/delta":
		var payload PlanDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "command/exec/outputDelta":
		var payload CommandExecOutputDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "process/outputDelta":
		var payload ProcessOutputDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "process/exited":
		var payload ProcessExitedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/commandExecution/outputDelta":
		var payload CommandExecutionOutputDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/commandExecution/terminalInteraction":
		var payload TerminalInteractionNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/fileChange/outputDelta":
		var payload FileChangeOutputDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/fileChange/patchUpdated":
		var payload FileChangePatchUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "serverRequest/resolved":
		var payload ServerRequestResolvedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/mcpToolCall/progress":
		var payload McpToolCallProgressNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "mcpServer/oauthLogin/completed":
		var payload McpServerOauthLoginCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "mcpServer/startupStatus/updated":
		var payload McpServerStatusUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "account/updated":
		var payload AccountUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "account/rateLimits/updated":
		var payload AccountRateLimitsUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "app/list/updated":
		var payload AppListUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "remoteControl/status/changed":
		var payload RemoteControlStatusChangedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "externalAgentConfig/import/progress":
		var payload ExternalAgentConfigImportProgressNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "externalAgentConfig/import/completed":
		var payload ExternalAgentConfigImportCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "fs/changed":
		var payload FsChangedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/reasoning/summaryTextDelta":
		var payload ReasoningSummaryTextDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/reasoning/summaryPartAdded":
		var payload ReasoningSummaryPartAddedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "item/reasoning/textDelta":
		var payload ReasoningTextDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/compacted":
		var payload ContextCompactedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "model/rerouted":
		var payload ModelReroutedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "model/verification":
		var payload ModelVerificationNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "turn/moderationMetadata":
		var payload TurnModerationMetadataNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "model/safetyBuffering/updated":
		var payload ModelSafetyBufferingUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "warning":
		var payload WarningNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "guardianWarning":
		var payload GuardianWarningNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "deprecationNotice":
		var payload DeprecationNoticeNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "configWarning":
		var payload ConfigWarningNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "fuzzyFileSearch/sessionUpdated":
		var payload FuzzyFileSearchSessionUpdatedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "fuzzyFileSearch/sessionCompleted":
		var payload FuzzyFileSearchSessionCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/started":
		var payload ThreadRealtimeStartedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/itemAdded":
		var payload ThreadRealtimeItemAddedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/transcript/delta":
		var payload ThreadRealtimeTranscriptDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/transcript/done":
		var payload ThreadRealtimeTranscriptDoneNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/outputAudio/delta":
		var payload ThreadRealtimeOutputAudioDeltaNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/sdp":
		var payload ThreadRealtimeSdpNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/error":
		var payload ThreadRealtimeErrorNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "thread/realtime/closed":
		var payload ThreadRealtimeClosedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "windows/worldWritableWarning":
		var payload WindowsWorldWritableWarningNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "windowsSandbox/setupCompleted":
		var payload WindowsSandboxSetupCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	case "account/login/completed":
		var payload AccountLoginCompletedNotification
		if err := json.Unmarshal(params, &payload); err != nil {
			return nil, err
		}
		return payload, nil
	}
	return append(json.RawMessage(nil), params...), nil
}

var RoutingLifecycleByStartMethod = map[string]RoutingLifecycleMetadata{
	"thread/start":                 {ResourceDomain: "thread", StartMethod: "thread/start", WireIdentitySource: "thread.id", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "thread/start", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "thread/closed", Predicate: "threadId matches thread.id"}, LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "thread/deleted", Predicate: "threadId matches thread.id"}}, NotificationOptOutDependencies: []string{"thread/closed", "thread/deleted"}},
	"turn/start":                   {ResourceDomain: "turn", StartMethod: "turn/start", WireIdentitySource: "turn.id", StartCompletion: LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "turn/started", Predicate: "threadId and turn.id are present"}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "turn/completed", Predicate: "threadId and turn.id match"}}, NotificationOptOutDependencies: []string{"turn/started", "turn/completed"}},
	"account/login/start":          {ResourceDomain: "accountLogin", StartMethod: "account/login/start", WireIdentitySource: "loginId", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "account/login/start", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "account/login/completed", Predicate: "success or error completed login flow"}}, NotificationOptOutDependencies: []string{"account/login/completed"}},
	"review/start":                 {ResourceDomain: "review", StartMethod: "review/start", WireIdentitySource: "reviewThreadId + turn.id", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "review/start", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "turn/completed", Predicate: "threadId matches reviewThreadId and turn.id matches review turn"}}, NotificationOptOutDependencies: []string{"turn/started", "turn/completed"}},
	"remoteControl/pairing/start":  {ResourceDomain: "remoteControlPairing", StartMethod: "remoteControl/pairing/start", WireIdentitySource: "pairingCode or manualPairingCode", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "remoteControl/pairing/start", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "explicitMethodResponse", Method: "remoteControl/pairing/status", Predicate: ""}}, NotificationOptOutDependencies: nil},
	"command/exec":                 {ResourceDomain: "commandExec", StartMethod: "command/exec", WireIdentitySource: "processId", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "command/exec", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "command/exec", Predicate: ""}}, NotificationOptOutDependencies: []string{"command/exec/outputDelta"}},
	"process/spawn":                {ResourceDomain: "process", StartMethod: "process/spawn", WireIdentitySource: "processHandle", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "process/spawn", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "process/exited", Predicate: "processHandle matches spawned process"}}, NotificationOptOutDependencies: []string{"process/outputDelta", "process/exited"}},
	"fs/watch":                     {ResourceDomain: "fs", StartMethod: "fs/watch", WireIdentitySource: "watchId", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "fs/watch", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "explicitMethodResponse", Method: "fs/unwatch", Predicate: ""}}, NotificationOptOutDependencies: []string{"fs/changed"}},
	"mcpServer/oauth/login":        {ResourceDomain: "mcpServer", StartMethod: "mcpServer/oauth/login", WireIdentitySource: "name", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "mcpServer/oauth/login", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "mcpServer/oauthLogin/completed", Predicate: "name matches OAuth server"}}, NotificationOptOutDependencies: []string{"mcpServer/oauthLogin/completed"}},
	"fuzzyFileSearch/sessionStart": {ResourceDomain: "fuzzyFileSearch", StartMethod: "fuzzyFileSearch/sessionStart", WireIdentitySource: "sessionId", StartCompletion: LifecycleTriggerMetadata{Kind: "jsonRpcResponse", Method: "fuzzyFileSearch/sessionStart", Predicate: ""}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "explicitMethodResponse", Method: "fuzzyFileSearch/sessionStop", Predicate: ""}}, NotificationOptOutDependencies: []string{"fuzzyFileSearch/sessionUpdated", "fuzzyFileSearch/sessionCompleted"}},
	"thread/realtime/start":        {ResourceDomain: "realtime", StartMethod: "thread/realtime/start", WireIdentitySource: "threadId", StartCompletion: LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "thread/realtime/started", Predicate: "threadId matches realtime thread"}, CleanupTriggers: []LifecycleTriggerMetadata{LifecycleTriggerMetadata{Kind: "explicitMethodResponse", Method: "thread/realtime/stop", Predicate: ""}, LifecycleTriggerMetadata{Kind: "terminalNotification", Method: "thread/realtime/closed", Predicate: "threadId matches realtime thread"}}, NotificationOptOutDependencies: []string{"thread/realtime/started", "thread/realtime/itemAdded", "thread/realtime/transcript/delta", "thread/realtime/transcript/done", "thread/realtime/outputAudio/delta", "thread/realtime/sdp", "thread/realtime/error", "thread/realtime/closed"}},
}
