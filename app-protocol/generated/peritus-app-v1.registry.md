# Peritus application protocol v1 registry

Generated from Rust metadata. Numeric and semantic allocations are append-only.

## Families

| Tag | Family | Schema | Payloads |
|---:|---|---:|---|
| 94 | `app-client-hello` | 1 | `1:client-hello` |
| 95 | `app-server-hello` | 1 | `1:compatible`, `2:downgraded`, `3:incompatible` |
| 96 | `app-request` | 1 | `1:submit-command`, `2:subscribe`, `3:open-artifact`, `4:cancel-artifact`, `5:answer-prompt`, `6:cancel-prompt`, `7:attach-terminal`, `8:terminal-input`, `9:terminal-resize`, `10:detach-terminal`, `11:cancel-terminal`, `12:daemon-status`, `13:shutdown`, `14:begin-artifact-upload`, `15:upload-artifact-chunk`, `16:complete-artifact-upload`, `17:start-product-run`, `18:control-product-run`, `19:query-product-runs`, `20:continue-product-run`, `21:query-product-run-conversation`, `22:interact`, `23:query-interaction`, `24:query-models`, `25:update-models`, `26:interact-with-effort`, `27:update-models-with-effort`, `28:doctor`, `29:workbench-command`, `30:query-workbench`, `31:query-workbench-receipt`, `32:query-workbench-queue`, `33:query-workbench-context`, `34:query-workbench-brief`, `35:begin-workbench-image-upload`, `36:preview-workbench-image`, `37:query-workbench-images`, `38:preview-workbench-file`, `39:query-workbench-files`, `40:begin-workbench-file-upload`, `41:preview-workbench-file-import`, `42:preview-workbench-compaction`, `60:query-workbench-goal`, `80:query-workbench-review`, `100:query-workbench-result`, `120:preview-workbench-rewind`, `121:inspect-workbench-checkpoint`, `140:query-conversation-library`, `160:query-workbench-permissions`, `161:query-workbench-memory`, `162:discover-init` |
| 97 | `app-response` | 1 | `1:command-result`, `2:subscription-started`, `3:artifact-opened`, `4:prompt-accepted`, `5:terminal-attached`, `6:acknowledged`, `7:daemon-status`, `8:shutdown-accepted`, `9:error`, `10:product-run-accepted`, `11:product-runs`, `12:product-run-conversation`, `13:product-run-settled`, `14:product-run-settlements`, `15:interaction`, `16:models`, `17:interaction-with-effort`, `18:doctor-report`, `19:workbench-snapshot`, `20:workbench-receipt`, `21:workbench-queue`, `22:workbench-context`, `23:workbench-brief`, `24:workbench-image-preview`, `25:workbench-images`, `26:workbench-file-preview`, `27:workbench-files`, `28:workbench-file-import-preview`, `29:workbench-compaction-preview`, `60:workbench-goal`, `80:workbench-review`, `100:workbench-result`, `120:workbench-checkpoint`, `121:workbench-rewind-preview`, `122:workbench-restore`, `140:conversation-library`, `160:workbench-permissions`, `161:workbench-memory`, `162:init-proposal` |
| 98 | `app-event` | 1 | `1:domain-event`, `2:subscription-gap`, `3:backpressure`, `4:artifact-metadata`, `5:artifact-chunk`, `6:artifact-complete`, `7:prompt-requested`, `8:terminal-output`, `9:terminal-exited`, `10:readiness-changed`, `11:diagnostic`, `12:heartbeat`, `13:shutdown-progress`, `14:shutdown-complete` |
| 99 | `app-control` | 1 | `1:acknowledge`, `2:cancel-subscription`, `3:cancel-artifact`, `4:cancel-prompt`, `5:cancel-terminal`, `6:subscription`, `7:heartbeat-reply` |

## Typed fields

### `ClientHelloData`

Rust type: `ClientHello`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `protocolId` | yes | `fixed[16]` | `ProtocolId` | `ProtocolId` | `nonzero` |
| `requestedSessionId` | no | `option+value` | `Option<SessionId>` | `SessionId` | `nonzero` |
| `versions` | yes | `len+items` | `Vec<VersionRange>` | `readonly VersionRange[]` | `nonzero`, `app.max-versions`, `strictly-sorted-unique` |
| `requiredFeatures` | yes | `len+items` | `ProtocolFeatureSet` | `readonly string[]` | `app.max-features`, `strictly-sorted-unique` |
| `optionalFeatures` | yes | `len+items` | `ProtocolFeatureSet` | `readonly string[]` | `app.max-features`, `strictly-sorted-unique` |
| `receiveLimits` | yes | `ordered-fields` | `AppProtocolLimits` | `AppProtocolLimits` | — |
| `implementation` | yes | `len+utf8` | `ImplementationMetadata` | `string` | `codec.max-string-bytes` |
| `establishedSessionId` | no | `option+value` | `Option<SessionId>` | `SessionId` | `nonzero` |

### `ServerHelloPreamble`

Rust type: `ServerHello`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `protocolId` | yes | `fixed[16]` | `ProtocolId` | `ProtocolId` | `nonzero` |
| `implementation` | yes | `len+utf8` | `ImplementationMetadata` | `string` | `codec.max-string-bytes` |

### `RequestEnvelopeFields`

Rust type: `AppRequestEnvelope`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `context` | yes | `ordered-fields` | `ProtocolContext` | `ProtocolContext` | — |
| `requestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero` |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero` |

### `ResponseEnvelopeFields`

Rust type: `AppResponseEnvelope`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `context` | yes | `ordered-fields` | `ProtocolContext` | `ProtocolContext` | — |
| `requestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero` |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero` |

### `EventEnvelopeFields`

Rust type: `AppEventEnvelope`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `context` | yes | `ordered-fields` | `ProtocolContext` | `ProtocolContext` | — |

### `ControlEnvelopeFields`

Rust type: `ControlEnvelope`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `context` | yes | `ordered-fields` | `ProtocolContext` | `ProtocolContext` | — |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero` |

### `ProtocolVersion`

Rust type: `ProtocolVersion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `major` | yes | `u16-be` | `u16` | `number` | `nonzero` |
| `minor` | yes | `u16-be` | `u16` | `number` | — |

### `VersionRange`

Rust type: `VersionRange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `major` | yes | `u16-be` | `u16` | `number` | `nonzero` |
| `minorMin` | yes | `u16-be` | `u16` | `number` | — |
| `minorMax` | yes | `u16-be` | `u16` | `number` | — |

### `ProtocolContext`

Rust type: `ProtocolContext`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `protocolId` | yes | `fixed[16]` | `ProtocolId` | `ProtocolId` | `nonzero` |
| `version` | yes | `ordered-fields` | `ProtocolVersion` | `ProtocolVersion` | — |
| `sessionId` | yes | `fixed[16]` | `SessionId` | `SessionId` | `nonzero` |

### `CodecLimits`

Rust type: `CodecLimits`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `maxFrameBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero` |
| `maxPayloadBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero` |
| `maxCollectionItems` | yes | `u64-be` | `usize` | `UInt64` | `nonzero` |
| `maxStringBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero` |
| `maxOpaqueBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero` |
| `maxNestingDepth` | yes | `u16-be` | `u16` | `number` | `nonzero` |

### `AppProtocolLimits`

Rust type: `AppProtocolLimits`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `codec` | yes | `ordered-fields` | `CodecLimits` | `CodecLimits` | — |
| `maxVersions` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-versions` |
| `maxFeatures` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-features` |
| `maxIdempotencyEntries` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `codec.max-collection-items` |
| `maxTopics` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-topics` |
| `maxInFlightEvents` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-in-flight-events` |
| `maxArtifactChunkBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-artifact-chunk-bytes` |
| `maxPromptChoices` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-prompt-choices` |
| `maxTerminalChunkBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-terminal-chunk-bytes` |
| `maxDiagnosticBytes` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-diagnostic-bytes` |
| `maxRemainingWorkItems` | yes | `u64-be` | `usize` | `UInt64` | `nonzero`, `app.max-remaining-work-items` |

### `NegotiatedProtocol`

Rust type: `NegotiatedProtocol`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `version` | yes | `ordered-fields` | `ProtocolVersion` | `ProtocolVersion` | — |
| `features` | yes | `len+items` | `ProtocolFeatureSet` | `readonly string[]` | `app.max-features`, `strictly-sorted-unique` |
| `limits` | yes | `ordered-fields` | `AppProtocolLimits` | `AppProtocolLimits` | — |

### `IncompatibilityReason`

Rust type: `IncompatibilityReason`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `reason` | yes | `u8` | `IncompatibilityReason` | `"no-common-version" | "missing-required-features"` | — |
| `missingRequiredFeatures` | no | `len+items` | `ProtocolFeatureSet` | `readonly string[]` | `app.max-features`, `strictly-sorted-unique` |

### `RevisionTuple`

Rust type: `RevisionTuple`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `acceptanceSpecId` | yes | `fixed[16]` | `AcceptanceSpecId` | `AcceptanceSpecId` | `nonzero` |
| `harnessId` | yes | `fixed[16]` | `HarnessId` | `HarnessId` | `nonzero` |
| `workspaceId` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `workspaceGeneration` | yes | `u64-be` | `Generation` | `UInt64` | `nonzero` |
| `workspaceRevision` | yes | `u64-be` | `RevisionNumber` | `UInt64` | `nonzero` |
| `policyId` | yes | `fixed[16]` | `PolicyId` | `PolicyId` | `nonzero` |
| `providerProfileId` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |

### `CommandBinding`

Rust type: `CommandBinding`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `actorId` | yes | `fixed[16]` | `ActorId` | `ActorId` | `nonzero` |
| `sessionId` | yes | `fixed[16]` | `SessionId` | `SessionId` | `nonzero`, `envelope-binding` |
| `requestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero`, `envelope-binding` |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero`, `envelope-binding` |
| `idempotencyKey` | yes | `len+bytes` | `IdempotencyKey` | `Base64Bytes` | `nonzero`, `128 bytes` |
| `expectedRevision` | no | `option+value` | `Option<RevisionTuple>` | `RevisionTuple` | — |
| `envelopeFrame` | yes | `len+bytes` | `ExactB3Frame` | `Base64Bytes` | `codec.max-frame-bytes` |
| `commandFrame` | yes | `len+bytes` | `ExactB3Frame` | `Base64Bytes` | `codec.max-frame-bytes` |

### `CommittedEventRange`

Rust type: `CommittedEventRange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `first` | yes | `u64-be` | `EventCursor` | `UInt64` | `nonzero`, `contiguous` |
| `last` | yes | `u64-be` | `EventCursor` | `UInt64` | `nonzero`, `contiguous` |

### `AppProtocolError`

Rust type: `AppProtocolError`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `code` | yes | `u16-be` | `AppErrorCode` | `AppErrorCode` | — |
| `retry` | yes | `u8` | `RetryDisposition` | `RetryDisposition` | — |
| `subsystem` | yes | `u8` | `ResponsibleSubsystem` | `ResponsibleSubsystem` | — |
| `diagnostic` | no | `option+value` | `Option<AppDiagnostic>` | `string` | `app.max-diagnostic-bytes` |

### `CommandResult`

Rust type: `CommandResult`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `originalRequestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero`, `envelope-binding` |
| `disposition` | yes | `u8` | `CommandDisposition` | `"committed" | "replayed" | "rejected"` | — |
| `committedEvents` | no | `ordered-fields` | `Option<CommittedEventRange>` | `CommittedEventRange` | `contiguous` |
| `error` | no | `ordered-fields` | `Option<AppProtocolError>` | `AppProtocolError` | — |

### `SubscriptionFilter`

Rust type: `SubscriptionFilter`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `topics` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `app.max-topics`, `codec.max-string-bytes`, `strictly-sorted-unique` |

### `SubscriptionRequest`

Rust type: `SubscriptionRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `filter` | yes | `ordered-fields` | `SubscriptionFilter` | `SubscriptionFilter` | — |
| `after` | yes | `u64-be` | `EventCursor` | `UInt64` | — |
| `maximumInFlight` | yes | `u32-be` | `u32` | `number` | `nonzero`, `app.max-in-flight-events` |
| `snapshotAcceptable` | yes | `bool/u8` | `bool` | `boolean` | — |

### `SubscriptionStarted`

Rust type: `SubscriptionStarted`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `after` | yes | `u64-be` | `EventCursor` | `UInt64` | — |
| `maximumInFlight` | yes | `u32-be` | `u32` | `number` | `nonzero`, `app.max-in-flight-events` |

### `Delivery`

Rust type: `Delivery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `eventId` | yes | `fixed[16]` | `EventId` | `EventId` | `nonzero` |
| `cursor` | yes | `u64-be` | `EventCursor` | `UInt64` | `nonzero`, `contiguous` |
| `attemptId` | yes | `fixed[16]` | `DeliveryAttemptId` | `DeliveryAttemptId` | `nonzero` |
| `attempt` | yes | `u32-be` | `u32` | `number` | `nonzero` |
| `frame` | yes | `len+bytes` | `RegisteredEventFrame` | `Base64Bytes` | `codec.max-frame-bytes` |

### `SubscriptionGap`

Rust type: `SubscriptionGap`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `requested` | yes | `u64-be` | `EventCursor` | `UInt64` | — |
| `earliest` | yes | `u64-be` | `EventCursor` | `UInt64` | `contiguous` |
| `latest` | yes | `u64-be` | `EventCursor` | `UInt64` | `contiguous` |

### `SubscriptionGapEvent`

Rust type: `AppEventPayload::SubscriptionGap`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `gap` | yes | `ordered-fields` | `SubscriptionGap` | `SubscriptionGap` | — |

### `SubscriptionBackpressure`

Rust type: `SubscriptionBackpressure`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `lastDelivered` | yes | `u64-be` | `EventCursor` | `UInt64` | — |
| `lastAcknowledged` | yes | `u64-be` | `EventCursor` | `UInt64` | `contiguous` |
| `maximumInFlight` | yes | `u32-be` | `u32` | `number` | `nonzero`, `app.max-in-flight-events` |

### `Acknowledgement`

Rust type: `Acknowledgement`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `cursor` | yes | `u64-be` | `EventCursor` | `UInt64` | `contiguous` |

### `SubscriptionCancellation`

Rust type: `SubscriptionCancellation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero`, `envelope-binding` |
| `source` | yes | `u8` | `SubscriptionCancellationSource` | `"client" | "server"` | — |

### `SubscriptionControl`

Rust type: `SubscriptionControl`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `action` | yes | `u8` | `SubscriptionControl` | `"pause" | "resume"` | — |
| `subscriptionId` | yes | `fixed[16]` | `SubscriptionId` | `SubscriptionId` | `nonzero` |
| `reason` | no | `u8` | `Option<PauseReason>` | `"client" | "slow-consumer"` | — |

### `ArtifactOpenRequest`

Rust type: `ArtifactOpenRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `transferId` | yes | `fixed[16]` | `TransferId` | `TransferId` | `nonzero` |
| `artifactId` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |

### `ArtifactMetadata`

Rust type: `ArtifactMetadata`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `transferId` | yes | `fixed[16]` | `TransferId` | `TransferId` | `nonzero` |
| `artifactId` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `byteSize` | yes | `u64-be` | `u64` | `UInt64` | `declared-artifact-size` |
| `mediaType` | yes | `len+utf8` | `CanonicalMediaType` | `string` | `codec.max-string-bytes` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `preferredChunkSize` | yes | `u32-be` | `u32` | `number` | `nonzero`, `app.max-artifact-chunk-bytes` |

### `ArtifactChunk`

Rust type: `ArtifactChunk`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `transferId` | yes | `fixed[16]` | `TransferId` | `TransferId` | `nonzero` |
| `artifactId` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `ordinal` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `offset` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `bytes` | yes | `len+bytes` | `Vec<u8>` | `Base64Bytes` | `app.max-artifact-chunk-bytes`, `declared-artifact-size` |

### `ArtifactCompletion`

Rust type: `ArtifactCompletion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `transferId` | yes | `fixed[16]` | `TransferId` | `TransferId` | `nonzero` |
| `artifactId` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `byteSize` | yes | `u64-be` | `u64` | `UInt64` | `declared-artifact-size` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `ArtifactCancellation`

Rust type: `ArtifactCancellation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `transferId` | yes | `fixed[16]` | `TransferId` | `TransferId` | `nonzero` |
| `artifactId` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero`, `envelope-binding` |

### `PromptCorrelation`

Rust type: `PromptCorrelation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `originatingRequestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero` |
| `promptId` | yes | `fixed[16]` | `PromptId` | `PromptId` | `nonzero` |
| `sessionId` | yes | `fixed[16]` | `SessionId` | `SessionId` | `nonzero` |
| `actorId` | yes | `fixed[16]` | `ActorId` | `ActorId` | `nonzero` |
| `revision` | yes | `ordered-fields` | `RevisionTuple` | `RevisionTuple` | — |
| `freshnessDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `cancellationGeneration` | yes | `u64-be` | `Generation` | `UInt64` | `nonzero` |

### `PromptChoice`

Rust type: `PromptChoice`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `len+utf8` | `String` | `string` | `nonzero`, `codec.max-string-bytes` |
| `label` | yes | `len+utf8` | `String` | `string` | `nonzero`, `codec.max-string-bytes` |

### `PromptConstraint`

Rust type: `PromptConstraint`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u8` | `PromptConstraint` | `"non-empty" | "maximum-text-bytes" | "bound-choice-only" | "secret-reference"` | — |
| `maximumTextBytes` | no | `u32-be` | `Option<u32>` | `number` | `codec.max-string-bytes` |

### `ApprovalChallenge`

Rust type: `ApprovalChallenge`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `decisionCommandId` | yes | `fixed[16]` | `CommandId` | `CommandId` | `nonzero` |
| `registryRevision` | yes | `u64-be` | `RevisionNumber` | `UInt64` | `nonzero` |
| `requestFrame` | yes | `len+bytes` | `Vec<u8>` | `Base64Bytes` | `nonzero`, `codec.max-opaque-bytes` |

### `SignedApprovalDecisionFrame`

Rust type: `SignedApprovalDecisionFrame`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `bytes` | yes | `len+bytes` | `Vec<u8>` | `Base64Bytes` | `nonzero`, `codec.max-opaque-bytes` |

### `PromptBinding`

Rust type: `PromptBinding`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u8` | `PromptKind` | `"approval" | "user-input"` | — |
| `approvalChallenge` | no | `option+value` | `Option<ApprovalChallenge>` | `ApprovalChallenge` | — |
| `correlation` | yes | `ordered-fields` | `PromptCorrelation` | `PromptCorrelation` | — |
| `choices` | yes | `len+items` | `Vec<PromptChoice>` | `readonly PromptChoice[]` | `app.max-prompt-choices` |
| `constraints` | yes | `len+items` | `Vec<PromptConstraint>` | `readonly PromptConstraint[]` | `codec.max-collection-items` |

### `PromptAnswer`

Rust type: `PromptAnswer`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `correlation` | yes | `ordered-fields` | `PromptCorrelation` | `PromptCorrelation` | — |
| `answerKind` | yes | `u8` | `PromptAnswerPayload` | `"approval" | "user-input"` | — |
| `approvalAnswerKind` | no | `u8` | `Option<ApprovalAnswer>` | `"signed-decision" | "cancel"` | — |
| `signedDecisionFrame` | no | `option+value` | `Option<SignedApprovalDecisionFrame>` | `Base64Bytes` | `codec.max-opaque-bytes` |
| `rationale` | no | `option+value` | `Option<String>` | `string` | `codec.max-string-bytes` |
| `inputKind` | no | `u8` | `Option<UserInputValue>` | `"text" | "selection" | "confirmation" | "secret-reference"` | — |
| `textValue` | no | `len+utf8` | `Option<String>` | `string` | `codec.max-string-bytes` |
| `confirmationValue` | no | `bool/u8` | `Option<bool>` | `boolean` | — |

### `PromptCancellation`

Rust type: `PromptCancellation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `correlation` | yes | `ordered-fields` | `PromptCorrelation` | `PromptCorrelation` | — |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero`, `envelope-binding` |

### `TerminalBinding`

Rust type: `TerminalBinding`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `attachmentId` | yes | `fixed[16]` | `TerminalAttachmentId` | `TerminalAttachmentId` | `nonzero` |
| `processId` | yes | `fixed[16]` | `ProcessId` | `ProcessId` | `nonzero` |
| `originatingRequestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero`, `envelope-binding` |

### `TerminalInput`

Rust type: `TerminalInput`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `binding` | yes | `ordered-fields` | `TerminalBinding` | `TerminalBinding` | — |
| `bytes` | yes | `len+bytes` | `Vec<u8>` | `Base64Bytes` | `app.max-terminal-chunk-bytes` |

### `TerminalResize`

Rust type: `TerminalResize`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `binding` | yes | `ordered-fields` | `TerminalBinding` | `TerminalBinding` | — |
| `columns` | yes | `u16-be` | `u16` | `number` | `nonzero` |
| `rows` | yes | `u16-be` | `u16` | `number` | `nonzero` |

### `TerminalDetach`

Rust type: `TerminalDetach`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `binding` | yes | `ordered-fields` | `TerminalBinding` | `TerminalBinding` | — |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero` |

### `TerminalCancellation`

Rust type: `TerminalCancellation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `binding` | yes | `ordered-fields` | `TerminalBinding` | `TerminalBinding` | — |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero`, `envelope-binding` |

### `TerminalOutput`

Rust type: `TerminalOutput`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `binding` | yes | `ordered-fields` | `TerminalBinding` | `TerminalBinding` | — |
| `sequence` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `offset` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `stream` | yes | `u8` | `TerminalStream` | `"stdout" | "stderr" | "terminal"` | — |
| `bytes` | yes | `len+bytes` | `Vec<u8>` | `Base64Bytes` | `app.max-terminal-chunk-bytes` |

### `TerminalExit`

Rust type: `TerminalExit`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `binding` | yes | `ordered-fields` | `TerminalBinding` | `TerminalBinding` | — |
| `nextSequence` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `finalOffset` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `disposition` | yes | `u8` | `TerminalExitDisposition` | `TerminalExitDisposition` | — |

### `TerminalExitDisposition`

Rust type: `TerminalExitDisposition`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u8` | `TerminalExitDisposition` | `"code" | "signal" | "unknown"` | — |
| `value` | no | `i32-be` | `Option<i32>` | `number` | — |

### `DaemonStatus`

Rust type: `DaemonStatus`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `readiness` | yes | `u8` | `DaemonReadiness` | `DaemonReadiness` | — |
| `diagnostic` | no | `option+value` | `Option<String>` | `string` | `app.max-diagnostic-bytes` |

### `DaemonHeartbeat`

Rust type: `DaemonHeartbeat`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `heartbeatId` | yes | `fixed[16]` | `HeartbeatId` | `HeartbeatId` | `nonzero` |
| `sequence` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |
| `status` | yes | `ordered-fields` | `DaemonStatus` | `DaemonStatus` | — |

### `ShutdownRequest`

Rust type: `ShutdownRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `requestId` | yes | `fixed[16]` | `RequestId` | `RequestId` | `nonzero`, `envelope-binding` |
| `correlationId` | yes | `fixed[16]` | `CorrelationId` | `CorrelationId` | `nonzero`, `envelope-binding` |

### `RemainingWork`

Rust type: `RemainingWork`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u8` | `RemainingWorkKind` | `"request" | "subscription" | "artifact-transfer" | "terminal-attachment" | "other"` | — |
| `descriptor` | yes | `len+utf8` | `String` | `string` | `app.max-diagnostic-bytes` |

### `ShutdownProgress`

Rust type: `ShutdownProgress`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `ShutdownRequest` | `ShutdownRequest` | — |
| `completedSteps` | yes | `u32-be` | `u32` | `number` | — |
| `totalSteps` | yes | `u32-be` | `u32` | `number` | — |
| `remaining` | yes | `len+items` | `Vec<RemainingWork>` | `readonly RemainingWork[]` | `app.max-remaining-work-items` |

### `ShutdownComplete`

Rust type: `ShutdownComplete`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `ShutdownRequest` | `ShutdownRequest` | — |
| `disposition` | yes | `u8` | `ShutdownCompletionDisposition` | `"clean" | "unclean"` | — |
| `remaining` | yes | `len+items` | `Vec<RemainingWork>` | `readonly RemainingWork[]` | `app.max-remaining-work-items` |

### `HeartbeatReply`

Rust type: `HeartbeatReply`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `heartbeatId` | yes | `fixed[16]` | `HeartbeatId` | `HeartbeatId` | `nonzero` |
| `sequence` | yes | `u64-be` | `u64` | `UInt64` | `contiguous` |

### `DoctorQuery`

Rust type: `DoctorQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `workspace` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `provider` | no | `option+value` | `Option<ProviderProfileId>` | `ProviderProfileId` | `nonzero` |

### `DoctorFinding`

Rust type: `DoctorFinding`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `check` | yes | `len+utf8` | `String` | `string` | `doctor.max-check-bytes (64)` |
| `status` | yes | `u16-be` | `DoctorStatus` | `"healthy" | "warning" | "blocked" | "unsupported"` | — |
| `observation` | yes | `len+utf8` | `String` | `string` | `doctor.max-text-bytes (1024)` |
| `action` | yes | `len+utf8` | `String` | `string` | `doctor.max-text-bytes (1024)` |

### `DoctorReport`

Rust type: `DoctorReport`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `DoctorQuery` | `DoctorQuery` | — |
| `findings` | yes | `len+items` | `Vec<DoctorFinding>` | `readonly DoctorFinding[]` | `doctor.max-findings (32)` |

### `WorkbenchReviewRange`

Rust type: `WorkbenchReviewRange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `oldStart` | yes | `u32-be` | `u32` | `number` | — |
| `oldLines` | yes | `u32-be` | `u32` | `number` | — |
| `newStart` | yes | `u32-be` | `u32` | `number` | — |
| `newLines` | yes | `u32-be` | `u32` | `number` | — |

### `WorkbenchReviewAnchor`

Rust type: `WorkbenchReviewAnchor`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `run` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `workspace` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `candidateDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `diffDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `beforeBlobDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `afterBlobDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `contextDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `path` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `target` | yes | `u16-be` | `WorkbenchReviewTarget` | `"file" | "hunk"` | — |
| `range` | yes | `ordered-fields` | `WorkbenchReviewRange` | `WorkbenchReviewRange` | — |

### `WorkbenchAddReviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"addReview"` | — |
| `anchor` | yes | `ordered-fields` | `WorkbenchReviewAnchor` | `WorkbenchReviewAnchor` | — |
| `feedback` | yes | `u16-be` | `WorkbenchReviewFeedback` | `"explain" | "requestRevision" | "keepBehavior" | "leaveAlone"` | — |
| `message` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchRebindReviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"rebindReview"` | — |
| `comment` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `anchor` | yes | `ordered-fields` | `WorkbenchReviewAnchor` | `WorkbenchReviewAnchor` | — |

### `WorkbenchDismissReviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"dismissReview"` | — |
| `comment` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |

### `WorkbenchExecutionSettings`

Rust type: `WorkbenchExecutionSettings`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `run` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `providers` | yes | `ordered-fields` | `ProductProviderSelection` | `ProductProviderSelection` | — |
| `mode` | yes | `u16-be` | `ProductInteractionMode` | `"chat" | "plan" | "review" | "build"` | — |
| `hasEffort` | yes | `bool/u8` | `bool` | `boolean` | — |
| `models` | yes | `ordered-fields` | `ProductRoleModels` | `ProductRoleModels` | — |

### `WorkbenchStartIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"startExecution"` | — |
| `settings` | yes | `ordered-fields` | `WorkbenchExecutionSettings` | `WorkbenchExecutionSettings` | — |

### `WorkbenchQuery`

Rust type: `WorkbenchQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `conversation` | yes | `fixed[16]` | `ConversationId` | `ConversationId` | `nonzero` |
| `workspace` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |

### `WorkbenchTitleIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"createConversation" | "renameConversation"` | — |
| `title` | yes | `len+utf8` | `ConversationTitle` | `string` | `workbench.max-title-bytes (256)` |

### `WorkbenchFlagIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"pinConversation" | "archiveConversation"` | — |
| `value` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchCommand`

Rust type: `WorkbenchCommand`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `expectedRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `intent` | yes | `ordered-fields` | `WorkbenchIntent` | `WorkbenchTitleIntent | WorkbenchFlagIntent | WorkbenchForkIntent | WorkbenchQueueControlIntent | WorkbenchStartIntent | WorkbenchBriefIntent | WorkbenchBriefAcceptIntent | WorkbenchSetContextIntent | WorkbenchCreateCheckpointIntent | WorkbenchApplyRewindIntent | WorkbenchApplyCompactionIntent | WorkbenchAttachImageIntent | WorkbenchSelectImageIntent | WorkbenchAttachFileIntent | WorkbenchAttachFileImportIntent | WorkbenchSelectFileIntent | WorkbenchStartGoalIntent | WorkbenchPauseGoalIntent | WorkbenchResumeOrClearGoalIntent | WorkbenchUpdateGoalBudgetIntent | WorkbenchAddReviewIntent | WorkbenchRebindReviewIntent | WorkbenchDismissReviewIntent | WorkbenchStartPreviewIntent | WorkbenchInteractPreviewIntent | WorkbenchCapturePreviewIntent | WorkbenchStopPreviewIntent | WorkbenchCheckPreviewIntent | WorkbenchArtifactFeedbackIntent | WorkbenchPermissionIntent | WorkbenchSaveGuidanceIntent | WorkbenchReviseGuidanceIntent | WorkbenchPinGuidanceIntent | WorkbenchScopeGuidanceIntent | WorkbenchForgetGuidanceIntent | WorkbenchInitApplyIntent` | — |

### `WorkbenchSnapshot`

Rust type: `WorkbenchSnapshot`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `title` | yes | `len+utf8` | `ConversationTitle` | `string` | `workbench.max-title-bytes (256)` |
| `pinned` | yes | `bool/u8` | `bool` | `boolean` | — |
| `archived` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchReceipt`

Rust type: `WorkbenchReceipt`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `acceptedRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `payloadDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchResultQuery`

Rust type: `WorkbenchResultQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `run` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |

### `WorkbenchLaunchSource`

Rust type: `WorkbenchLaunchSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchLaunchSourceKind` | `"managedCandidate" | "plainFolderFile"` | — |
| `path` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchBuildIdentity`

Rust type: `WorkbenchBuildIdentity`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `path` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchLaunchProfile`

Rust type: `WorkbenchLaunchProfile`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `run` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `executable` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |
| `arguments` | yes | `len+items` | `Vec<WorkbenchLaunchText>` | `readonly string[]` | `workbench.max-launch-arguments (256)`, `workbench.max-launch-text-bytes (4096)` |
| `workingDirectory` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |
| `environment` | yes | `len+items` | `Vec<WorkbenchLaunchText>` | `readonly string[]` | `workbench.max-launch-environment (64)`, `workbench.max-launch-text-bytes (4096)`, `strictly-sorted-unique` |
| `source` | yes | `ordered-fields` | `WorkbenchLaunchSource` | `WorkbenchLaunchSource` | — |
| `build` | no | `option+value` | `Option<WorkbenchBuildIdentity>` | `WorkbenchBuildIdentity` | — |
| `readinessMillis` | yes | `u64-be` | `u64` | `UInt64` | `nonzero`, `workbench.max-launch-wall-millis (600000)` |
| `wallMillis` | yes | `u64-be` | `u64` | `UInt64` | `nonzero`, `workbench.max-launch-wall-millis (600000)` |
| `interactive` | yes | `bool/u8` | `bool` | `boolean` | — |
| `network` | yes | `u16-be` | `WorkbenchPreviewNetwork` | `"inheritedHost"` | — |
| `stopPolicy` | yes | `u16-be` | `WorkbenchPreviewStopPolicy` | `"terminateOwnedTree"` | — |

### `WorkbenchCaptureTarget`

Rust type: `WorkbenchCaptureTarget`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchCaptureTarget` | `"x11Window"` | — |
| `window` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchCaptureRequest`

Rust type: `WorkbenchCaptureRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `launch` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `target` | yes | `ordered-fields` | `WorkbenchCaptureTarget` | `WorkbenchCaptureTarget` | — |
| `consent` | yes | `u16-be` | `WorkbenchCaptureConsent` | `"granted" | "denied"` | — |

### `WorkbenchArtifactRegion`

Rust type: `WorkbenchArtifactRegion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `x` | yes | `u32-be` | `u32` | `number` | — |
| `y` | yes | `u32-be` | `u32` | `number` | — |
| `width` | yes | `u32-be` | `u32` | `number` | `nonzero` |
| `height` | yes | `u32-be` | `u32` | `number` | `nonzero` |

### `WorkbenchStartPreviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"startPreview"` | — |
| `profile` | yes | `ordered-fields` | `WorkbenchLaunchProfile` | `WorkbenchLaunchProfile` | — |

### `WorkbenchInteractPreviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"interactPreview"` | — |
| `launch` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `input` | yes | `len+bytes` | `WorkbenchPreviewInput` | `Base64Bytes` | `workbench.max-preview-input-bytes (65536)` |

### `WorkbenchCapturePreviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"capturePreview"` | — |
| `request` | yes | `ordered-fields` | `WorkbenchCaptureRequest` | `WorkbenchCaptureRequest` | — |

### `WorkbenchStopPreviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"stopPreview"` | — |
| `launch` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |

### `WorkbenchCheckPreviewIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"checkPreviewBehavior"` | — |
| `launch` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `observed` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |
| `note` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchArtifactFeedbackIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"addArtifactFeedback"` | — |
| `capture` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `feedback` | yes | `u16-be` | `WorkbenchReviewFeedback` | `"explain" | "requestRevision" | "keepBehavior" | "leaveAlone"` | — |
| `message` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `region` | no | `option+value` | `Option<WorkbenchArtifactRegion>` | `WorkbenchArtifactRegion` | — |

### `WorkbenchInteractionReceipt`

Rust type: `WorkbenchInteractionReceipt`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `observed` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchCaptureReceipt`

Rust type: `WorkbenchCaptureReceipt`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `state` | yes | `u16-be` | `WorkbenchCaptureState` | `"denied" | "unavailable" | "failed" | "captured"` | — |
| `target` | yes | `ordered-fields` | `WorkbenchCaptureTarget` | `WorkbenchCaptureTarget` | — |
| `artifact` | no | `option+value` | `Option<ArtifactId>` | `ArtifactId` | — |
| `imageDigest` | no | `option+value` | `Option<Sha256Digest>` | `Sha256Digest` | — |
| `width` | no | `option+value` | `Option<u32>` | `number` | `nonzero` |
| `height` | no | `option+value` | `Option<u32>` | `number` | `nonzero` |
| `capturedUnixMillis` | no | `option+value` | `Option<u64>` | `UInt64` | — |
| `detail` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |

### `WorkbenchArtifactFeedback`

Rust type: `WorkbenchArtifactFeedback`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `capture` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `feedback` | yes | `u16-be` | `WorkbenchReviewFeedback` | `"explain" | "requestRevision" | "keepBehavior" | "leaveAlone"` | — |
| `message` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `region` | no | `option+value` | `Option<WorkbenchArtifactRegion>` | `WorkbenchArtifactRegion` | — |

### `WorkbenchCaptureAvailable`

Rust type: `WorkbenchCaptureCapability`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchCaptureCapability` | `"x11SelectedWindow"` | — |

### `WorkbenchCaptureUnavailable`

Rust type: `WorkbenchCaptureCapability`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchCaptureCapability` | `"unavailable"` | — |
| `reason` | yes | `len+utf8` | `WorkbenchLaunchText` | `string` | `workbench.max-launch-text-bytes (4096)` |

### `WorkbenchLaunchResult`

Rust type: `WorkbenchLaunchResult`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `launch` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `profile` | yes | `ordered-fields` | `WorkbenchLaunchProfile` | `WorkbenchLaunchProfile` | — |
| `process` | no | `option+value` | `Option<ProcessId>` | `ProcessId` | — |
| `state` | yes | `u16-be` | `WorkbenchLaunchState` | `"accepted" | "running" | "stopped" | "exited" | "failed"` | — |
| `ready` | yes | `bool/u8` | `bool` | `boolean` | — |
| `interactions` | yes | `len+items` | `Vec<WorkbenchInteractionReceipt>` | `readonly WorkbenchInteractionReceipt[]` | `workbench.max-preview-interactions (256)` |
| `captures` | yes | `len+items` | `Vec<WorkbenchCaptureReceipt>` | `readonly WorkbenchCaptureReceipt[]` | `workbench.max-preview-captures (64)` |
| `feedback` | yes | `len+items` | `Vec<WorkbenchArtifactFeedback>` | `readonly WorkbenchArtifactFeedback[]` | `workbench.max-artifact-feedback (256)` |
| `behaviorChecks` | yes | `u32-be` | `u32` | `number` | — |
| `stdoutDigest` | no | `option+value` | `Option<Sha256Digest>` | `Sha256Digest` | — |
| `exitCode` | no | `option+value` | `Option<u32>` | `number` | — |

### `WorkbenchResultPage`

Rust type: `WorkbenchResultPage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchResultQuery` | `WorkbenchResultQuery` | — |
| `controlRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `resultRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `capability` | yes | `ordered-fields` | `WorkbenchCaptureCapability` | `WorkbenchCaptureAvailable | WorkbenchCaptureUnavailable` | — |
| `launches` | yes | `len+items` | `Vec<WorkbenchLaunchResult>` | `readonly WorkbenchLaunchResult[]` | `workbench.max-launches (16)` |

### `WorkbenchPermissionChange`

Rust type: `WorkbenchPermissionChange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `expectedAuthorityRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `capability` | yes | `u16-be` | `WorkbenchPermissionCapability` | `"read" | "write" | "process" | "network"` | — |
| `allowed` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchPermissionIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"setPermissions"` | — |
| `change` | yes | `ordered-fields` | `WorkbenchPermissionChange` | `WorkbenchPermissionChange` | — |

### `WorkbenchPermissionEntry`

Rust type: `WorkbenchPermissionEntry`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `capability` | yes | `u16-be` | `WorkbenchPermissionCapability` | `"read" | "write" | "process" | "network"` | — |
| `hostAllowed` | yes | `bool/u8` | `bool` | `boolean` | — |
| `effectiveAllowed` | yes | `bool/u8` | `bool` | `boolean` | — |
| `provenance` | yes | `u16-be` | `WorkbenchPermissionProvenance` | `"workspaceHostPolicy" | "toolHostPolicy" | "providerHostPolicy" | "userRestriction"` | — |
| `explicitApprovalStillRequired` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchPermissions`

Rust type: `WorkbenchPermissions`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `conversationRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `authorityRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `trust` | yes | `u16-be` | `WorkbenchWorkspaceTrust` | `"managed" | "directReadOnly" | "directWritable"` | — |
| `entries` | yes | `len+items` | `[WorkbenchPermissionEntry; 4]` | `WorkbenchPermissionEntry[]` | `codec.max-collection-items` |

### `InitDiscoveryRequest`

Rust type: `InitDiscoveryRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `InitSourceObservation`

Rust type: `InitSourceObservation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `path` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `kind` | yes | `u16-be` | `InitSourceKind` | `"manifest" | "documentation" | "instructions" | "commandConfig"` | — |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `bytes` | yes | `u64-be` | `u64` | `UInt64` | `codec.max-string-bytes` |

### `InitInstructionPatch`

Rust type: `InitInstructionPatch`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `path` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `hasOriginal` | yes | `bool/u8` | `bool` | `boolean` | — |
| `originalContent` | no | `len+utf8` | `Option<String>` | `string` | `codec.max-string-bytes` |
| `hasPreconditionDigest` | yes | `bool/u8` | `bool` | `boolean` | — |
| `preconditionDigest` | no | `fixed[32]` | `Option<Sha256Digest>` | `Sha256Digest` | — |
| `preconditionBytes` | yes | `u64-be` | `u64` | `UInt64` | `codec.max-string-bytes` |
| `mode` | yes | `u16-be` | `InitFileMode` | `"regular" | "executable"` | — |
| `proposedContent` | yes | `len+utf8` | `String` | `string` | `codec.max-string-bytes` |
| `diff` | yes | `len+utf8` | `String` | `string` | `codec.max-string-bytes` |

### `InitCommand`

Rust type: `InitCommand`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `InitCommandKind` | `"build" | "test" | "lint" | "launch"` | — |
| `source` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `executable` | yes | `len+utf8` | `String` | `string` | `codec.max-string-bytes` |
| `arguments` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `codec.max-collection-items`, `codec.max-string-bytes` |
| `verification` | yes | `u16-be` | `InitCommandVerification` | `"unverified"` | — |

### `InitProposal`

Rust type: `InitProposal`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `folderDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `sources` | yes | `len+items` | `Vec<InitSourceObservation>` | `readonly InitSourceObservation[]` | `codec.max-collection-items`, `strictly-sorted-unique` |
| `patch` | yes | `ordered-fields` | `InitInstructionPatch` | `InitInstructionPatch` | — |
| `commands` | yes | `len+items` | `Vec<InitCommand>` | `readonly InitCommand[]` | `codec.max-collection-items`, `strictly-sorted-unique` |

### `WorkbenchInitApplyIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"applyInitDiff"` | — |
| `proposal` | yes | `ordered-fields` | `InitProposal` | `InitProposal` | — |

### `WorkbenchGuidanceProjectScope`

Rust type: `WorkbenchGuidanceScope`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchGuidanceScope` | `"project"` | — |

### `WorkbenchGuidanceConversationScope`

Rust type: `WorkbenchGuidanceScope`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchGuidanceScope` | `"conversation"` | — |
| `conversation` | yes | `fixed[16]` | `ConversationId` | `ConversationId` | `nonzero` |

### `WorkbenchGuidanceUserSource`

Rust type: `WorkbenchGuidanceSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchGuidanceSource` | `"userAuthored"` | — |

### `WorkbenchGuidanceReplySource`

Rust type: `WorkbenchGuidanceSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchGuidanceSource` | `"acceptedPublicReply"` | — |
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchGuidanceContent`

Rust type: `WorkbenchGuidanceContent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `text` | yes | `len+utf8` | `WorkbenchGuidanceText` | `string` | `codec.max-string-bytes` |
| `source` | yes | `ordered-fields` | `WorkbenchGuidanceSource` | `WorkbenchGuidanceUserSource | WorkbenchGuidanceReplySource` | — |
| `scope` | yes | `ordered-fields` | `WorkbenchGuidanceScope` | `WorkbenchGuidanceProjectScope | WorkbenchGuidanceConversationScope` | — |

### `WorkbenchGuidanceIdentity`

Rust type: `WorkbenchGuidanceIdentity`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `workspace` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |

### `WorkbenchGuidanceVersion`

Rust type: `WorkbenchGuidanceVersion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `record` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `dependency` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchGuidanceValidation`

Rust type: `WorkbenchGuidanceValidation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `contentDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchGuidanceRecord`

Rust type: `WorkbenchGuidanceRecord`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `identity` | yes | `ordered-fields` | `WorkbenchGuidanceIdentity` | `WorkbenchGuidanceIdentity` | — |
| `version` | yes | `ordered-fields` | `WorkbenchGuidanceVersion` | `WorkbenchGuidanceVersion` | — |
| `content` | yes | `ordered-fields` | `WorkbenchGuidanceContent` | `WorkbenchGuidanceContent` | — |
| `pinned` | yes | `bool/u8` | `bool` | `boolean` | — |
| `lastValidation` | yes | `ordered-fields` | `WorkbenchGuidanceValidation` | `WorkbenchGuidanceValidation` | — |

### `WorkbenchGuidancePrior`

Rust type: `WorkbenchGuidancePrior`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `scope` | yes | `ordered-fields` | `WorkbenchGuidanceScope` | `WorkbenchGuidanceProjectScope | WorkbenchGuidanceConversationScope` | — |
| `pinned` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchGuidanceTombstone`

Rust type: `WorkbenchGuidanceTombstone`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `identity` | yes | `ordered-fields` | `WorkbenchGuidanceIdentity` | `WorkbenchGuidanceIdentity` | — |
| `prior` | yes | `ordered-fields` | `WorkbenchGuidancePrior` | `WorkbenchGuidancePrior` | — |
| `forgottenBy` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `reason` | yes | `len+utf8` | `WorkbenchGuidanceReason` | `string` | `codec.max-string-bytes` |
| `dependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchGuidanceSelection`

Rust type: `WorkbenchGuidanceSelection`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `expectedRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchSaveGuidanceIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"saveGuidance"` | — |
| `expectedDependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `content` | yes | `ordered-fields` | `WorkbenchGuidanceContent` | `WorkbenchGuidanceContent` | — |
| `pinned` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchReviseGuidanceIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"reviseGuidance"` | — |
| `selection` | yes | `ordered-fields` | `WorkbenchGuidanceSelection` | `WorkbenchGuidanceSelection` | — |
| `expectedDependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `content` | yes | `ordered-fields` | `WorkbenchGuidanceContent` | `WorkbenchGuidanceContent` | — |

### `WorkbenchPinGuidanceIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"pinGuidance"` | — |
| `selection` | yes | `ordered-fields` | `WorkbenchGuidanceSelection` | `WorkbenchGuidanceSelection` | — |
| `expectedDependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `pinned` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchScopeGuidanceIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"scopeGuidance"` | — |
| `selection` | yes | `ordered-fields` | `WorkbenchGuidanceSelection` | `WorkbenchGuidanceSelection` | — |
| `expectedDependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `scope` | yes | `ordered-fields` | `WorkbenchGuidanceScope` | `WorkbenchGuidanceProjectScope | WorkbenchGuidanceConversationScope` | — |

### `WorkbenchForgetGuidanceIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"forgetGuidance"` | — |
| `selection` | yes | `ordered-fields` | `WorkbenchGuidanceSelection` | `WorkbenchGuidanceSelection` | — |
| `expectedDependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `reason` | yes | `len+utf8` | `WorkbenchGuidanceReason` | `string` | `codec.max-string-bytes` |

### `WorkbenchMemoryQuery`

Rust type: `WorkbenchMemoryQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `dependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `offset` | yes | `u32-be` | `u32` | `number` | — |
| `includeForgotten` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchMemoryActiveRow`

Rust type: `WorkbenchMemoryRow`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchMemoryRow` | `"active"` | — |
| `record` | yes | `ordered-fields` | `WorkbenchGuidanceRecord` | `WorkbenchGuidanceRecord` | — |

### `WorkbenchMemoryForgottenRow`

Rust type: `WorkbenchMemoryRow`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchMemoryRow` | `"forgotten"` | — |
| `tombstone` | yes | `ordered-fields` | `WorkbenchGuidanceTombstone` | `WorkbenchGuidanceTombstone` | — |

### `WorkbenchMemory`

Rust type: `WorkbenchMemory`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchMemoryQuery` | `WorkbenchMemoryQuery` | — |
| `dependencyRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `total` | yes | `u32-be` | `u32` | `number` | — |
| `rows` | yes | `len+items` | `Vec<WorkbenchMemoryRow>` | `readonly (WorkbenchMemoryActiveRow | WorkbenchMemoryForgottenRow)[]` | `codec.max-collection-items` |

### `WorkbenchInputSelection`

Rust type: `WorkbenchInputSelection`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `fixed[16]` | `WorkbenchInputId` | `WorkbenchInputId` | `nonzero` |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchNewInput`

Rust type: `WorkbenchNewInput`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `fixed[16]` | `WorkbenchInputId` | `WorkbenchInputId` | `nonzero` |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `dependencies` | yes | `len+items` | `WorkbenchInputOrder` | `readonly WorkbenchInputId[]` | `workbench.max-input-dependencies (32)` |

### `WorkbenchEnqueueIntent`

Rust type: `WorkbenchQueueIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchQueueIntent` | `"enqueue"` | — |
| `input` | yes | `ordered-fields` | `WorkbenchNewInput` | `WorkbenchNewInput` | — |

### `WorkbenchEditInputIntent`

Rust type: `WorkbenchQueueIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchQueueIntent` | `"edit"` | — |
| `selected` | yes | `ordered-fields` | `WorkbenchInputSelection` | `WorkbenchInputSelection` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchCorrectInputIntent`

Rust type: `WorkbenchQueueIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchQueueIntent` | `"correct"` | — |
| `original` | yes | `ordered-fields` | `WorkbenchInputSelection` | `WorkbenchInputSelection` | — |
| `id` | yes | `fixed[16]` | `WorkbenchInputId` | `WorkbenchInputId` | `nonzero` |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchHoldInputIntent`

Rust type: `WorkbenchQueueIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchQueueIntent` | `"hold"` | — |
| `selected` | yes | `ordered-fields` | `WorkbenchInputSelection` | `WorkbenchInputSelection` | — |
| `held` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchWithdrawInputIntent`

Rust type: `WorkbenchQueueIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchQueueIntent` | `"withdraw"` | — |
| `selected` | yes | `ordered-fields` | `WorkbenchInputSelection` | `WorkbenchInputSelection` | — |

### `WorkbenchReorderInputIntent`

Rust type: `WorkbenchQueueIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchQueueIntent` | `"reorder"` | — |
| `order` | yes | `len+items` | `WorkbenchInputOrder` | `readonly WorkbenchInputId[]` | `workbench.max-inputs (1024)` |

### `WorkbenchQueueControlIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"queue"` | — |
| `queue` | yes | `ordered-fields` | `WorkbenchQueueIntent` | `WorkbenchEnqueueIntent | WorkbenchEditInputIntent | WorkbenchCorrectInputIntent | WorkbenchHoldInputIntent | WorkbenchWithdrawInputIntent | WorkbenchReorderInputIntent` | — |

### `WorkbenchInputRow`

Rust type: `WorkbenchInputRow`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `selected` | yes | `ordered-fields` | `WorkbenchInputSelection` | `WorkbenchInputSelection` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `state` | yes | `u16-be` | `WorkbenchInputState` | `"queued" | "held" | "incorporated" | "superseded" | "withdrawn"` | — |
| `dependencies` | yes | `len+items` | `WorkbenchInputOrder` | `readonly WorkbenchInputId[]` | `workbench.max-input-dependencies (32)` |

### `WorkbenchQueueQuery`

Rust type: `WorkbenchQueueQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `offset` | yes | `u32-be` | `u32` | `number` | `workbench.max-inputs (1024)` |
| `history` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchQueuePage`

Rust type: `WorkbenchQueuePage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQueueQuery` | `WorkbenchQueueQuery` | — |
| `total` | yes | `u32-be` | `u32` | `number` | `workbench.max-inputs (1024)` |
| `rows` | yes | `len+items` | `Vec<WorkbenchInputRow>` | `readonly WorkbenchInputRow[]` | `workbench.max-input-page (32)` |

### `WorkbenchContextNextView`

Rust type: `WorkbenchContextView`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextView` | `"next" | "history"` | — |

### `WorkbenchContextInvocationView`

Rust type: `WorkbenchContextView`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextView` | `"invocation"` | — |
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |

### `WorkbenchContextQuery`

Rust type: `WorkbenchContextQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `offset` | yes | `u32-be` | `u32` | `number` | `workbench.max-context-rows (8192)` |
| `view` | yes | `ordered-fields` | `WorkbenchContextView` | `WorkbenchContextNextView | WorkbenchContextInvocationView` | — |

### `WorkbenchContextInputSource`

Rust type: `WorkbenchContextSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextSource` | `"input"` | — |
| `selected` | yes | `ordered-fields` | `WorkbenchInputSelection` | `WorkbenchInputSelection` | — |

### `WorkbenchContextReplySource`

Rust type: `WorkbenchContextSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextSource` | `"publicReply"` | — |
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |

### `WorkbenchContextMessageSource`

Rust type: `WorkbenchContextSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextSource` | `"message"` | — |
| `ordinal` | yes | `u32-be` | `u32` | `number` | — |
| `role` | yes | `u16-be` | `WorkbenchMessageRole` | `"system" | "developer" | "user" | "assistant" | "tool"` | — |

### `WorkbenchContextInvocationSource`

Rust type: `WorkbenchContextSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextSource` | `"invocation"` | — |
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |
| `manifestDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchContextRow`

Rust type: `WorkbenchContextRow`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `source` | yes | `ordered-fields` | `WorkbenchContextSource` | `WorkbenchContextInputSource | WorkbenchContextReplySource | WorkbenchContextMessageSource | WorkbenchContextInvocationSource | WorkbenchContextImageSource | WorkbenchContextFileSource` | — |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `bytes` | yes | `u64-be` | `u64` | `UInt64` | `workbench.max-context-source-bytes (67108864)` |
| `disposition` | yes | `u16-be` | `WorkbenchContextDisposition` | `"eligible" | "included" | "held" | "withdrawn" | "superseded" | "dependencyBlocked" | "awaitingLaterInput" | "deselected" | "userExcluded"` | — |
| `hasPreference` | yes | `bool/u8` | `bool` | `boolean` | — |
| `preference` | no | `u16-be` | `WorkbenchContextPreference` | `"pinned" | "excluded"` | — |

### `WorkbenchSetContextIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"setContext"` | — |
| `source` | yes | `ordered-fields` | `WorkbenchContextSource` | `WorkbenchContextInputSource | WorkbenchContextReplySource | WorkbenchContextImageSource | WorkbenchContextFileSource` | — |
| `hasPreference` | yes | `bool/u8` | `bool` | `boolean` | — |
| `preference` | no | `u16-be` | `WorkbenchContextPreference` | `"pinned" | "excluded"` | — |

### `WorkbenchContextImageSource`

Rust type: `WorkbenchContextSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextSource` | `"image"` | — |
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `input` | yes | `fixed[16]` | `WorkbenchInputId` | `WorkbenchInputId` | `nonzero` |
| `artifact` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |

### `WorkbenchContextSeal`

Rust type: `WorkbenchContextSeal`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |
| `requestDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `manifestDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `generation` | yes | `u64-be` | `u64` | `UInt64` | — |

### `WorkbenchContextPage`

Rust type: `WorkbenchContextPage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchContextQuery` | `WorkbenchContextQuery` | — |
| `total` | yes | `u32-be` | `u32` | `number` | `workbench.max-context-rows (8192)` |
| `hasSeal` | yes | `bool/u8` | `bool` | `boolean` | — |
| `seal` | no | `ordered-fields` | `WorkbenchContextSeal` | `WorkbenchContextSeal` | — |
| `rows` | yes | `len+items` | `Vec<WorkbenchContextRow>` | `readonly WorkbenchContextRow[]` | `workbench.max-context-page (32)` |

### `WorkbenchCheckpointAbsentVersion`

Rust type: `WorkbenchCheckpointVersion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchCheckpointVersion` | `"absent"` | — |

### `WorkbenchCheckpointPresentVersion`

Rust type: `WorkbenchCheckpointVersion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchCheckpointVersion` | `"present"` | — |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `bytes` | yes | `u64-be` | `u64` | `UInt64` | — |
| `mode` | yes | `u16-be` | `WorkbenchCheckpointFileMode` | `"regular" | "executable"` | — |

### `WorkbenchCheckpointReferences`

Rust type: `WorkbenchCheckpointReferences`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `sourceConversationRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `contextGeneration` | yes | `u64-be` | `u64` | `UInt64` | — |
| `briefRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `hasGoalRevision` | yes | `bool/u8` | `bool` | `boolean` | — |
| `goalRevision` | no | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchCheckpointPath`

Rust type: `WorkbenchCheckpointPath`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `path` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `checkpoint` | yes | `ordered-fields` | `WorkbenchCheckpointVersion` | `WorkbenchCheckpointAbsentVersion | WorkbenchCheckpointPresentVersion` | — |
| `hasExpectedCurrent` | yes | `bool/u8` | `bool` | `boolean` | — |
| `expectedCurrent` | no | `ordered-fields` | `WorkbenchCheckpointVersion` | `WorkbenchCheckpointAbsentVersion | WorkbenchCheckpointPresentVersion` | — |

### `WorkbenchCheckpointReceipt`

Rust type: `WorkbenchCheckpointReceipt`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `checkpoint` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `acceptedRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `name` | yes | `len+utf8` | `WorkbenchCheckpointName` | `string` | `workbench.max-checkpoint-name-bytes (256)` |
| `references` | yes | `ordered-fields` | `WorkbenchCheckpointReferences` | `WorkbenchCheckpointReferences` | — |
| `paths` | yes | `len+items` | `Vec<WorkbenchCheckpointPath>` | `readonly WorkbenchCheckpointPath[]` | `workbench.max-checkpoint-paths (64)`, `strictly-sorted-unique` |
| `exclusions` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-checkpoint-text-bytes (512)` |
| `externalEffects` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-checkpoint-text-bytes (512)` |

### `WorkbenchRewindRequest`

Rust type: `WorkbenchRewindRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `checkpoint` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `mode` | yes | `u16-be` | `WorkbenchRewindMode` | `"files_only" | "conversation_only" | "combined"` | — |
| `child` | no | `fixed[16]` | `Option<ConversationId>` | `ConversationId` | `nonzero` |
| `allocation` | no | `option+value` | `Option<WorkbenchForkBudget>` | `WorkbenchForkBudget` | — |

### `WorkbenchRewindPath`

Rust type: `WorkbenchRewindPath`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `path` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `checkpoint` | yes | `ordered-fields` | `WorkbenchCheckpointVersion` | `WorkbenchCheckpointAbsentVersion | WorkbenchCheckpointPresentVersion` | — |
| `hasExpectedCurrent` | yes | `bool/u8` | `bool` | `boolean` | — |
| `expectedCurrent` | no | `ordered-fields` | `WorkbenchCheckpointVersion` | `WorkbenchCheckpointAbsentVersion | WorkbenchCheckpointPresentVersion` | — |
| `observedCurrent` | yes | `ordered-fields` | `WorkbenchCheckpointVersion` | `WorkbenchCheckpointAbsentVersion | WorkbenchCheckpointPresentVersion` | — |
| `disposition` | yes | `u16-be` | `WorkbenchRewindDisposition` | `"restore" | "unchanged" | "conflict" | "unsealed"` | — |

### `WorkbenchRewindPreview`

Rust type: `WorkbenchRewindPreview`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `WorkbenchRewindRequest` | `WorkbenchRewindRequest` | — |
| `previewDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `paths` | yes | `len+items` | `Vec<WorkbenchRewindPath>` | `readonly WorkbenchRewindPath[]` | `workbench.max-checkpoint-paths (64)`, `strictly-sorted-unique` |
| `exclusions` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-checkpoint-text-bytes (512)` |
| `externalEffects` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-checkpoint-text-bytes (512)` |
| `conversationHistoryPreserved` | yes | `bool/u8` | `bool` | `boolean` | — |
| `accountingPreserved` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchCreateCheckpointIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"createCheckpoint"` | — |
| `name` | yes | `len+utf8` | `WorkbenchCheckpointName` | `string` | `workbench.max-checkpoint-name-bytes (256)` |

### `WorkbenchApplyRewindIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"applyRewind"` | — |
| `preview` | yes | `ordered-fields` | `WorkbenchRewindPreview` | `WorkbenchRewindPreview` | — |

### `WorkbenchRestoreReceipt`

Rust type: `WorkbenchRestoreReceipt`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `restore` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `checkpoint` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `recoveryCheckpoint` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `acceptedRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `status` | yes | `u16-be` | `WorkbenchRestoreStatus` | `"applied" | "conflict" | "recoveryRequired"` | — |
| `restored` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-restore-text-bytes (4096)` |
| `conflicts` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-restore-text-bytes (4096)` |
| `externalEffects` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `workbench.max-checkpoint-paths (64)`, `workbench.max-restore-text-bytes (4096)` |

### `WorkbenchCompactionRequest`

Rust type: `WorkbenchCompactionRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `hasFocus` | yes | `bool/u8` | `bool` | `boolean` | — |
| `focus` | no | `len+utf8` | `WorkbenchCompactionFocus` | `string` | `workbench.max-compaction-focus-bytes (1024)` |

### `WorkbenchCompactionEntry`

Rust type: `WorkbenchCompactionEntry`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |
| `sourceDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `sourceBytes` | yes | `u64-be` | `u64` | `UInt64` | — |
| `replacementDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `replacementBytes` | yes | `u64-be` | `u64` | `UInt64` | — |

### `WorkbenchCompactionPreview`

Rust type: `WorkbenchCompactionPreview`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `WorkbenchCompactionRequest` | `WorkbenchCompactionRequest` | — |
| `generation` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `recentPreserved` | yes | `u32-be` | `u32` | `number` | `workbench.max-compaction-entries (1024)` |
| `pinnedPreserved` | yes | `u32-be` | `u32` | `number` | `workbench.max-compaction-entries (1024)` |
| `unresolvedPreserved` | yes | `u32-be` | `u32` | `number` | `workbench.max-compaction-entries (1024)` |
| `unsavablePreserved` | yes | `u32-be` | `u32` | `number` | `workbench.max-compaction-entries (1024)` |
| `entries` | yes | `len+items` | `Vec<WorkbenchCompactionEntry>` | `readonly WorkbenchCompactionEntry[]` | `workbench.max-compaction-entries (1024)`, `strictly-sorted-unique` |

### `WorkbenchApplyCompactionIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"applyCompaction"` | — |
| `preview` | yes | `ordered-fields` | `WorkbenchCompactionPreview` | `WorkbenchCompactionPreview` | — |

### `WorkbenchBriefIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"setBrief"` | — |
| `field` | yes | `u16-be` | `WorkbenchBriefField` | `"objective" | "acceptance" | "constraints" | "assumptions"` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchBriefAcceptIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"acceptBriefProposal"` | — |
| `field` | yes | `u16-be` | `WorkbenchBriefField` | `"objective" | "acceptance" | "constraints" | "assumptions"` | — |
| `proposal` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchBriefEntry`

Rust type: `WorkbenchBriefEntry`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `field` | yes | `u16-be` | `WorkbenchBriefField` | `"objective" | "acceptance" | "constraints" | "assumptions"` | — |
| `source` | yes | `ordered-fields` | `WorkbenchInputRow` | `WorkbenchInputRow` | — |

### `WorkbenchBriefProposal`

Rust type: `WorkbenchBriefProposal`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `invocation` | yes | `fixed[16]` | `WorkbenchInvocationId` | `WorkbenchInvocationId` | `nonzero` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchBriefObservation`

Rust type: `WorkbenchBriefObservation`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchBriefObservationKind` | `"image" | "file"` | — |
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `hasVersion` | yes | `bool/u8` | `bool` | `boolean` | — |
| `version` | no | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `label` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `bytes` | yes | `u64-be` | `u64` | `UInt64` | `workbench.max-context-source-bytes (67108864)` |
| `selected` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchBrief`

Rust type: `WorkbenchBrief`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `entries` | yes | `len+items` | `Vec<WorkbenchBriefEntry>` | `readonly WorkbenchBriefEntry[]` | `workbench.max-brief-fields (4)` |
| `proposals` | yes | `len+items` | `Vec<WorkbenchBriefProposal>` | `readonly WorkbenchBriefProposal[]` | `workbench.max-brief-proposals (8)`, `strictly-sorted-unique` |
| `observations` | yes | `len+items` | `Vec<WorkbenchBriefObservation>` | `readonly WorkbenchBriefObservation[]` | `workbench.max-brief-observations (32)`, `strictly-sorted-unique` |
| `excludedProposals` | yes | `u32-be` | `u32` | `number` | — |

### `WorkbenchImageUpload`

Rust type: `WorkbenchImageUpload`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `metadata` | yes | `ordered-fields` | `ArtifactMetadata` | `ArtifactMetadata` | — |

### `WorkbenchImageModel`

Rust type: `ProductModelChoice`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |
| `manual` | yes | `bool/u8` | `bool` | `boolean` | — |
| `effort` | yes | `u16-be` | `ProductModelEffort` | `"default" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"` | — |

### `WorkbenchImageRequest`

Rust type: `WorkbenchImageRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `artifact` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `provider` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |
| `model` | yes | `ordered-fields` | `WorkbenchImageModel` | `WorkbenchImageModel` | — |
| `label` | yes | `len+utf8` | `WorkbenchImageLabel` | `string` | `workbench.max-image-label-bytes (1024)` |

### `WorkbenchImageMetadata`

Rust type: `WorkbenchImageMetadata`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `bytes` | yes | `u64-be` | `u64` | `UInt64` | `nonzero`, `workbench.max-image-bytes (4194304)` |
| `format` | yes | `u16-be` | `WorkbenchImageFormat` | `"png" | "jpeg" | "gif" | "webp"` | — |
| `width` | yes | `u32-be` | `u32` | `number` | `nonzero`, `workbench.max-image-side (8192)` |
| `height` | yes | `u32-be` | `u32` | `number` | `nonzero`, `workbench.max-image-side (8192)` |
| `frames` | yes | `u32-be` | `u32` | `number` | `nonzero`, `workbench.max-image-frames (64)` |

### `WorkbenchImagePreview`

Rust type: `WorkbenchImagePreview`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `WorkbenchImageRequest` | `WorkbenchImageRequest` | — |
| `image` | yes | `ordered-fields` | `WorkbenchImageMetadata` | `WorkbenchImageMetadata` | — |
| `providerRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `resolvedModel` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |

### `WorkbenchAttachImageIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"attachImage"` | — |
| `preview` | yes | `ordered-fields` | `WorkbenchImagePreview` | `WorkbenchImagePreview` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchSelectImageIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"selectImage"` | — |
| `attachment` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `selected` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchImageQuery`

Rust type: `WorkbenchImageQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `offset` | yes | `u32-be` | `u32` | `number` | — |

### `WorkbenchImageRow`

Rust type: `WorkbenchImageRow`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `artifact` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `label` | yes | `len+utf8` | `WorkbenchImageLabel` | `string` | `workbench.max-image-label-bytes (1024)` |
| `image` | yes | `ordered-fields` | `WorkbenchImageMetadata` | `WorkbenchImageMetadata` | — |
| `source` | yes | `ordered-fields` | `WorkbenchInputRow` | `WorkbenchInputRow` | — |
| `selected` | yes | `bool/u8` | `bool` | `boolean` | — |
| `eligible` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchImagePage`

Rust type: `WorkbenchImagePage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchImageQuery` | `WorkbenchImageQuery` | — |
| `total` | yes | `u32-be` | `u32` | `number` | — |
| `rows` | yes | `len+items` | `Vec<WorkbenchImageRow>` | `readonly WorkbenchImageRow[]` | `workbench.max-image-page (32)` |

### `WorkbenchFileUpload`

Rust type: `WorkbenchFileUpload`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `metadata` | yes | `ordered-fields` | `ArtifactMetadata` | `ArtifactMetadata` | — |

### `WorkbenchFileRangeAll`

Rust type: `WorkbenchFileRange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchFileRange` | `"all"` | — |

### `WorkbenchFileRangeBytes`

Rust type: `WorkbenchFileRange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchFileRange` | `"bytes"` | — |
| `start` | yes | `u64-be` | `u64` | `UInt64` | — |
| `end` | yes | `u64-be` | `u64` | `UInt64` | — |

### `WorkbenchFileRangeLines`

Rust type: `WorkbenchFileRange`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchFileRange` | `"lines"` | — |
| `first` | yes | `u32-be` | `u32` | `number` | `nonzero` |
| `last` | yes | `u32-be` | `u32` | `number` | `nonzero` |

### `WorkbenchFileRequest`

Rust type: `WorkbenchFileRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `path` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `range` | yes | `ordered-fields` | `WorkbenchFileRange` | `WorkbenchFileRangeAll | WorkbenchFileRangeBytes | WorkbenchFileRangeLines` | — |
| `mode` | yes | `u16-be` | `WorkbenchFileMode` | `"snapshot" | "refreshOnRequest"` | — |
| `provider` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |
| `model` | yes | `ordered-fields` | `WorkbenchImageModel` | `WorkbenchImageModel` | — |

### `WorkbenchFileMetadata`

Rust type: `WorkbenchFileMetadata`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `sourceDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `sourceBytes` | yes | `u64-be` | `u64` | `UInt64` | `workbench.max-context-source-bytes (67108864)` |
| `rangeStart` | yes | `u64-be` | `u64` | `UInt64` | — |
| `rangeEnd` | yes | `u64-be` | `u64` | `UInt64` | — |
| `digest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |

### `WorkbenchFilePreview`

Rust type: `WorkbenchFilePreview`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `WorkbenchFileRequest` | `WorkbenchFileRequest` | — |
| `folder` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `file` | yes | `ordered-fields` | `WorkbenchFileMetadata` | `WorkbenchFileMetadata` | — |
| `providerRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `resolvedModel` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |

### `WorkbenchFileImportRequest`

Rust type: `WorkbenchFileImportRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `selection` | yes | `ordered-fields` | `WorkbenchFileRequest` | `WorkbenchFileRequest` | — |
| `artifact` | yes | `fixed[16]` | `ArtifactId` | `ArtifactId` | `nonzero` |
| `file` | yes | `ordered-fields` | `WorkbenchFileMetadata` | `WorkbenchFileMetadata` | — |

### `WorkbenchFileImportPreview`

Rust type: `WorkbenchFileImportPreview`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `WorkbenchFileImportRequest` | `WorkbenchFileImportRequest` | — |
| `providerRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `resolvedModel` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |

### `WorkbenchFileQuery`

Rust type: `WorkbenchFileQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `offset` | yes | `u32-be` | `u32` | `number` | `workbench.max-file-history (256)` |

### `WorkbenchFileRow`

Rust type: `WorkbenchFileRow`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `attachment` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `version` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `label` | yes | `len+utf8` | `String` | `string` | `workbench.max-file-path-bytes (4096)` |
| `mode` | yes | `u16-be` | `WorkbenchFileMode` | `"snapshot" | "refreshOnRequest"` | — |
| `file` | yes | `ordered-fields` | `WorkbenchFileMetadata` | `WorkbenchFileMetadata` | — |
| `selected` | yes | `bool/u8` | `bool` | `boolean` | — |
| `eligible` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchFilePage`

Rust type: `WorkbenchFilePage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchFileQuery` | `WorkbenchFileQuery` | — |
| `total` | yes | `u32-be` | `u32` | `number` | `workbench.max-file-history (256)` |
| `rows` | yes | `len+items` | `Vec<WorkbenchFileRow>` | `readonly WorkbenchFileRow[]` | `workbench.max-file-page (32)` |

### `WorkbenchAttachFileIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"attachFile"` | — |
| `preview` | yes | `ordered-fields` | `WorkbenchFilePreview` | `WorkbenchFilePreview` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchSelectFileIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"selectFile"` | — |
| `attachment` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `selected` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchAttachFileImportIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"attachFileImport"` | — |
| `preview` | yes | `ordered-fields` | `WorkbenchFileImportPreview` | `WorkbenchFileImportPreview` | — |
| `text` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |

### `WorkbenchContextFileSource`

Rust type: `WorkbenchContextSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchContextSource` | `"file"` | — |
| `attachment` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `version` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |

### `WorkbenchGoalBudget`

Rust type: `WorkbenchGoalBudget`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `maxActiveMillis` | no | `option+value` | `Option<u64>` | `UInt64` | `nonzero` |
| `maxRequests` | no | `option+value` | `Option<u32>` | `number` | `nonzero` |
| `maxToolCalls` | no | `option+value` | `Option<u32>` | `number` | `nonzero` |
| `maxTotalTokens` | no | `option+value` | `Option<u64>` | `UInt64` | `nonzero` |

### `WorkbenchGoalCriterionDefinition`

Rust type: `WorkbenchGoalCriterionDefinition`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchGoalCriterionKind` | `"runnerAcceptance" | "graphicalPlaytest" | "humanValidation"` | — |
| `description` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `mandatory` | yes | `bool/u8` | `bool` | `boolean` | — |

### `WorkbenchGoalDefinition`

Rust type: `WorkbenchGoalDefinition`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `objective` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `criteria` | yes | `len+items` | `Vec<WorkbenchGoalCriterionDefinition>` | `readonly WorkbenchGoalCriterionDefinition[]` | `workbench.max-goal-criteria (16)` |
| `budget` | yes | `ordered-fields` | `WorkbenchGoalBudget` | `WorkbenchGoalBudget` | — |

### `WorkbenchStartGoalIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"startGoal"` | — |
| `definition` | yes | `ordered-fields` | `WorkbenchGoalDefinition` | `WorkbenchGoalDefinition` | — |
| `settings` | yes | `ordered-fields` | `WorkbenchExecutionSettings` | `WorkbenchExecutionSettings` | — |

### `WorkbenchPauseGoalIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"pauseGoal"` | — |
| `goal` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `mode` | yes | `u16-be` | `WorkbenchGoalPauseMode` | `"now" | "afterOperation" | "beforeEdit"` | — |

### `WorkbenchResumeOrClearGoalIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"resumeGoal" | "clearGoal"` | — |
| `goal` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |

### `WorkbenchUpdateGoalBudgetIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"updateGoalBudget"` | — |
| `goal` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `budget` | yes | `ordered-fields` | `WorkbenchGoalBudget` | `WorkbenchGoalBudget` | — |

### `WorkbenchGoalCriterion`

Rust type: `WorkbenchGoalCriterion`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `definition` | yes | `ordered-fields` | `WorkbenchGoalCriterionDefinition` | `WorkbenchGoalCriterionDefinition` | — |
| `state` | yes | `u16-be` | `WorkbenchGoalCriterionState` | `"pending" | "satisfied" | "unavailable" | "stale"` | — |
| `evidenceRevision` | no | `option+value` | `Option<u64>` | `UInt64` | — |

### `WorkbenchGoalRoleUsage`

Rust type: `WorkbenchGoalRoleUsage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `role` | yes | `u16-be` | `WorkbenchGoalRole` | `"writer" | "reviewer" | "fixer"` | — |
| `requests` | yes | `u32-be` | `u32` | `number` | — |
| `completedRequests` | yes | `u32-be` | `u32` | `number` | — |
| `toolCalls` | yes | `u32-be` | `u32` | `number` | — |
| `totalTokens` | no | `option+value` | `Option<u64>` | `UInt64` | — |
| `providerCostMicrounits` | no | `option+value` | `Option<u64>` | `UInt64` | — |

### `WorkbenchGoalUsage`

Rust type: `WorkbenchGoalUsage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `roles` | yes | `ordered-fields` | `[WorkbenchGoalRoleUsage; 3]` | `readonly WorkbenchGoalRoleUsage[]` | `workbench.goal-roles (3)` |
| `activeMillis` | yes | `u64-be` | `u64` | `UInt64` | — |
| `wallMillis` | yes | `u64-be` | `u64` | `UInt64` | — |
| `retries` | yes | `u32-be` | `u32` | `number` | — |
| `providerFailovers` | yes | `u32-be` | `u32` | `number` | — |
| `compactions` | yes | `u32-be` | `u32` | `number` | — |
| `workspaceBytes` | yes | `u64-be` | `u64` | `UInt64` | — |
| `workspaceGrowthBytes` | yes | `u64-be` | `u64` | `UInt64` | — |
| `peakRssBytes` | yes | `u64-be` | `u64` | `UInt64` | — |

### `WorkbenchGoalSnapshot`

Rust type: `WorkbenchGoalSnapshot`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `aggregateRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `goal` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `run` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `objective` | yes | `len+utf8` | `WorkbenchInputText` | `string` | `workbench.max-input-bytes (8192)` |
| `state` | yes | `u16-be` | `WorkbenchGoalState` | `"active" | "waitingForUser" | "pausing" | "paused" | "blocked" | "budgetReached" | "achieved" | "cancelled"` | — |
| `reason` | yes | `len+utf8` | `String` | `string` | `workbench.max-goal-reason-bytes (512)` |
| `userRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `attempt` | yes | `u32-be` | `u32` | `number` | `nonzero` |
| `restartEligible` | yes | `bool/u8` | `bool` | `boolean` | — |
| `pauseMode` | no | `option+value` | `Option<WorkbenchGoalPauseMode>` | `"now" | "afterOperation" | "beforeEdit"` | — |
| `criteria` | yes | `len+items` | `Vec<WorkbenchGoalCriterion>` | `readonly WorkbenchGoalCriterion[]` | `workbench.max-goal-criteria (16)` |
| `budget` | yes | `ordered-fields` | `WorkbenchGoalBudget` | `WorkbenchGoalBudget` | — |
| `usage` | yes | `ordered-fields` | `WorkbenchGoalUsage` | `WorkbenchGoalUsage` | — |

### `ConversationLibraryQuery`

Rust type: `ConversationLibraryQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `workspace` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `literal` | no | `option+value` | `Option<ConversationSearchText>` | `string` | `workbench.max-conversation-search-bytes (256)` |
| `includeArchived` | yes | `bool/u8` | `bool` | `boolean` | — |
| `offset` | yes | `u32-be` | `u32` | `number` | — |
| `limit` | yes | `u16-be` | `u16` | `number` | `nonzero`, `workbench.max-conversation-library-page (64)` |

### `WorkbenchForkBudget`

Rust type: `WorkbenchForkBudget`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `activeMillis` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `requests` | yes | `u32-be` | `u32` | `number` | `nonzero` |
| `toolCalls` | yes | `u32-be` | `u32` | `number` | `nonzero` |
| `totalTokens` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |

### `WorkbenchForkRequest`

Rust type: `WorkbenchForkRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `child` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `title` | yes | `len+utf8` | `ConversationTitle` | `string` | `workbench.max-title-bytes (256)` |
| `checkpoint` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `sourceRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `contextGeneration` | yes | `u64-be` | `u64` | `UInt64` | — |
| `briefRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `goalRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `mode` | yes | `u16-be` | `WorkbenchForkMode` | `"readOnlyCurrentWorkspace" | "isolatedWritableWorkspace"` | — |
| `allocation` | no | `option+value` | `Option<WorkbenchForkBudget>` | `WorkbenchForkBudget` | — |

### `WorkbenchForkIntent`

Rust type: `WorkbenchIntent`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `WorkbenchIntent` | `"forkConversation"` | — |
| `fork` | yes | `ordered-fields` | `WorkbenchForkRequest` | `WorkbenchForkRequest` | — |

### `ConversationInputSource`

Rust type: `ConversationMessageSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `ConversationMessageSource` | `"input"` | — |
| `conversation` | yes | `fixed[16]` | `ConversationId` | `ConversationId` | `nonzero` |
| `input` | yes | `fixed[16]` | `WorkbenchInputId` | `WorkbenchInputId` | `nonzero` |
| `revision` | yes | `u64-be` | `u64` | `UInt64` | — |

### `ConversationReplySource`

Rust type: `ConversationMessageSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `ConversationMessageSource` | `"reply"` | — |
| `conversation` | yes | `fixed[16]` | `ConversationId` | `ConversationId` | `nonzero` |
| `operation` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |

### `ConversationLegacySource`

Rust type: `ConversationMessageSource`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `kind` | yes | `u16-be` | `ConversationMessageSource` | `"legacy"` | — |
| `run` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `index` | yes | `u32-be` | `u32` | `number` | — |

### `ConversationSearchSnippet`

Rust type: `ConversationSearchSnippet`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `source` | yes | `ordered-fields` | `ConversationMessageSource` | `ConversationInputSource | ConversationReplySource | ConversationLegacySource` | — |
| `text` | yes | `len+utf8` | `String` | `string` | `workbench.max-conversation-snippet-bytes (512)` |

### `WorkbenchBranchLineage`

Rust type: `WorkbenchBranchLineage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `parent` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `checkpoint` | yes | `fixed[16]` | `ControlOperationId` | `ControlOperationId` | `nonzero` |
| `sourceRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `contextGeneration` | yes | `u64-be` | `u64` | `UInt64` | — |
| `briefRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `goalRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `mode` | yes | `u16-be` | `WorkbenchForkMode` | `"readOnlyCurrentWorkspace" | "isolatedWritableWorkspace"` | — |
| `allocation` | no | `option+value` | `Option<WorkbenchForkBudget>` | `WorkbenchForkBudget` | — |

### `ConversationLibraryItem`

Rust type: `ConversationLibraryItem`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `WorkbenchQuery` | `WorkbenchQuery` | — |
| `title` | yes | `len+utf8` | `ConversationTitle` | `string` | `workbench.max-title-bytes (256)` |
| `pinned` | yes | `bool/u8` | `bool` | `boolean` | — |
| `archived` | yes | `bool/u8` | `bool` | `boolean` | — |
| `activityRevision` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `legacyRun` | no | `option+value` | `Option<RunId>` | `RunId` | `nonzero` |
| `goalState` | no | `option+value` | `Option<WorkbenchGoalState>` | `"active" | "waitingForUser" | "pausing" | "paused" | "blocked" | "budgetReached" | "achieved" | "cancelled"` | — |
| `goalDraft` | yes | `bool/u8` | `bool` | `boolean` | — |
| `handoff` | yes | `len+utf8` | `String` | `string` | `workbench.max-conversation-handoff-bytes (1024)` |
| `snippet` | no | `option+value` | `Option<ConversationSearchSnippet>` | `ConversationSearchSnippet` | — |
| `branch` | no | `option+value` | `Option<WorkbenchBranchLineage>` | `WorkbenchBranchLineage` | — |

### `ConversationLibraryPage`

Rust type: `ConversationLibraryPage`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `query` | yes | `ordered-fields` | `ConversationLibraryQuery` | `ConversationLibraryQuery` | — |
| `total` | yes | `u32-be` | `u32` | `number` | — |
| `nextOffset` | no | `option+value` | `Option<u32>` | `number` | — |
| `items` | yes | `len+items` | `Vec<ConversationLibraryItem>` | `readonly ConversationLibraryItem[]` | `workbench.max-conversation-library-page (64)` |

### `ProductProviderSelection`

Rust type: `ProductProviderSelection`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `writer` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |
| `reviewer` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |
| `fixer` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |

### `ProductDeliverable`

Rust type: `ProductDeliverable`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `workspacePath` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `changedPaths` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `product.max-deliverable-paths`, `product.max-detail-bytes` |
| `successfulCommands` | yes | `len+items` | `Vec<String>` | `readonly string[]` | `product.max-deliverable-commands`, `product.max-detail-bytes` |
| `runInstructions` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `accepted` | yes | `bool/u8` | `bool` | `boolean` | — |
| `commitRevision` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `exportPath` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `discarded` | yes | `bool/u8` | `bool` | `boolean` | — |

### `ProductRunSnapshot`

Rust type: `ProductRunSnapshot`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `runId` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `workspaceId` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `providers` | yes | `ordered-fields` | `ProductProviderSelection` | `ProductProviderSelection` | — |
| `phase` | yes | `u16-be` | `ProductRunPhase` | `ProductRunPhase` | — |
| `cycle` | yes | `u32-be` | `u32` | `number` | — |
| `task` | yes | `len+utf8` | `String` | `string` | `product.max-task-bytes` |
| `status` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `diff` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `gates` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `review` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `summary` | yes | `len+utf8` | `String` | `string` | `product.max-detail-bytes` |
| `deliverable` | no | `option+value` | `Option<ProductDeliverable>` | `ProductDeliverable` | — |

### `CandidateIdentity`

Rust type: `CandidateIdentity`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `runId` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `workspaceId` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `candidateDigest` | yes | `fixed[32]` | `Sha256Digest` | `Sha256Digest` | — |
| `conversationRevision` | yes | `u64-be` | `u64` | `UInt64` | — |
| `checkpointSequence` | yes | `u64-be` | `u64` | `UInt64` | `nonzero`, `contiguous` |

### `QualificationEvidenceRecord`

Rust type: `EvidenceRecord<QualificationEvidence>`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `provenance` | yes | `ordered-fields` | `CandidateIdentity` | `CandidateIdentity` | — |
| `result` | yes | `u16-be` | `QualificationEvidence` | `QualificationEvidence` | — |

### `QualificationEvidenceStatus`

Rust type: `EvidenceStatus<QualificationEvidence>`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `status` | yes | `u16-be` | `EvidenceStatus` | `EvidenceStatus` | — |
| `record` | no | `ordered-fields` | `EvidenceRecord<QualificationEvidence>` | `QualificationEvidenceRecord` | — |

### `CandidateCheckpoint`

Rust type: `CandidateCheckpoint`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `identity` | yes | `ordered-fields` | `CandidateIdentity` | `CandidateIdentity` | — |
| `stage` | yes | `u16-be` | `CandidateStage` | `CandidateStage` | — |
| `gates` | yes | `ordered-fields` | `EvidenceStatus<QualificationEvidence>` | `QualificationEvidenceStatus` | — |
| `obligations` | yes | `ordered-fields` | `EvidenceStatus<QualificationEvidence>` | `QualificationEvidenceStatus` | — |
| `review` | yes | `ordered-fields` | `EvidenceStatus<QualificationEvidence>` | `QualificationEvidenceStatus` | — |

### `RunSettlement`

Rust type: `RunSettlement`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `disposition` | yes | `u16-be` | `RunDisposition` | `RunDisposition` | — |
| `cause` | yes | `u16-be` | `SettlementCause` | `SettlementCause` | — |
| `checkpoint` | no | `option+value` | `Option<CandidateCheckpoint>` | `CandidateCheckpoint` | — |

### `ProductRunSettlementSnapshot`

Rust type: `ProductRunSettlementSnapshot`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `snapshot` | yes | `ordered-fields` | `ProductRunSnapshot` | `ProductRunSnapshot` | — |
| `settlement` | yes | `ordered-fields` | `RunSettlement` | `RunSettlement` | — |

### `ProductModelUpdate`

Rust type: `ProductModelUpdate`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `runId` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `models` | yes | `ordered-fields` | `ProductRoleModels` | `ProductRoleModels` | — |

### `ProductRunRequest`

Rust type: `ProductRunRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `runId` | yes | `fixed[16]` | `RunId` | `RunId` | `nonzero` |
| `workspaceId` | yes | `fixed[16]` | `WorkspaceId` | `WorkspaceId` | `nonzero` |
| `providers` | yes | `ordered-fields` | `ProductProviderSelection` | `ProductProviderSelection` | — |
| `task` | yes | `len+utf8` | `String` | `string` | `product.max-task-bytes` |

### `ProductModelChoice`

Rust type: `ProductModelChoice`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |
| `manual` | yes | `bool/u8` | `bool` | `boolean` | — |
| `effort` | no | `u16-be` | `ProductModelEffort` | `"default" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"` | — |

### `ProductRoleModels`

Rust type: `ProductRoleModels`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `writer` | yes | `ordered-fields` | `ProductModelChoice` | `ProductModelChoice` | — |
| `reviewer` | yes | `ordered-fields` | `ProductModelChoice` | `ProductModelChoice` | — |
| `fixer` | yes | `ordered-fields` | `ProductModelChoice` | `ProductModelChoice` | — |

### `ProductInteractionRequest`

Rust type: `ProductInteractionRequest`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `request` | yes | `ordered-fields` | `ProductRunRequest` | `ProductRunRequest` | — |
| `mode` | yes | `u16-be` | `ProductInteractionMode` | `"chat" | "plan" | "review" | "build"` | — |
| `models` | yes | `ordered-fields` | `ProductRoleModels` | `ProductRoleModels` | — |

### `ProductActivity`

Rust type: `ProductActivity`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `sequence` | yes | `u64-be` | `u64` | `UInt64` | `nonzero` |
| `kind` | yes | `u16-be` | `ProductActivityKind` | `"user" | "assistant" | "tool" | "status" | "error"` | — |
| `text` | yes | `len+utf8` | `String` | `string` | `product.max-activity-bytes (8192)` |
| `detail` | yes | `len+utf8` | `String` | `string` | `product.max-activity-bytes (8192)` |

### `ProductInteractionSnapshot`

Rust type: `ProductInteractionSnapshot`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `settled` | yes | `bool/u8` | `bool` | `boolean` | — |
| `state` | yes | `ordered-fields` | `ProductRunSnapshot | ProductRunSettlementSnapshot` | `ProductRunSnapshot | ProductRunSettlementSnapshot` | — |
| `mode` | yes | `u16-be` | `ProductInteractionMode` | `"chat" | "plan" | "review" | "build"` | — |
| `models` | yes | `ordered-fields` | `ProductRoleModels` | `ProductRoleModels` | — |
| `received` | yes | `u64-be` | `u64` | `UInt64` | — |
| `incorporated` | yes | `u64-be` | `u64` | `UInt64` | — |
| `activities` | yes | `len+items` | `Vec<ProductActivity>` | `readonly ProductActivity[]` | `product.max-activities (256)`, `strictly-sorted-unique` |

### `ProductModelQuery`

Rust type: `ProductModelQuery`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `profile` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |
| `refresh` | yes | `bool/u8` | `bool` | `boolean` | — |

### `ProductModelInfo`

Rust type: `ProductModelInfo`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `id` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |
| `label` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |
| `tools` | no | `option+value` | `Option<bool>` | `boolean` | — |

### `ProductModelCatalog`

Rust type: `ProductModelCatalog`

| Field | Required | Canonical wire | Rust | TypeScript | Bounds |
|---|:---:|---|---|---|---|
| `profile` | yes | `fixed[16]` | `ProviderProfileId` | `ProviderProfileId` | `nonzero` |
| `configured` | yes | `len+utf8` | `String` | `string` | `product.max-model-bytes (512)` |
| `fetchedUnixSeconds` | yes | `u64-be` | `u64` | `UInt64` | — |
| `cached` | yes | `bool/u8` | `bool` | `boolean` | — |
| `error` | yes | `len+utf8` | `String` | `string` | `app.max-diagnostic-bytes` |
| `models` | yes | `len+items` | `Vec<ProductModelInfo>` | `readonly ProductModelInfo[]` | `product.max-models (4096)` |

## Stable errors

| Tag | Code |
|---:|---|
| 1 | `unsupported-format` |
| 2 | `unsupported-family` |
| 3 | `unsupported-schema` |
| 4 | `unknown-tag` |
| 5 | `malformed-frame` |
| 6 | `truncated-frame` |
| 7 | `trailing-bytes` |
| 8 | `limit-exceeded` |
| 9 | `invalid-identifier` |
| 10 | `invalid-version` |
| 11 | `incompatible-version` |
| 12 | `missing-required-feature` |
| 13 | `invalid-limits` |
| 20 | `session-mismatch` |
| 21 | `idempotency-conflict` |
| 22 | `idempotency-capacity` |
| 23 | `stale-revision` |
| 24 | `invalid-command-frame` |
| 25 | `command-binding-mismatch` |
| 26 | `invalid-event-range` |
| 30 | `subscription-state` |
| 31 | `subscription-gap` |
| 32 | `illegal-acknowledgement` |
| 33 | `backpressure` |
| 40 | `artifact-state` |
| 41 | `artifact-ordering` |
| 42 | `artifact-size` |
| 43 | `artifact-digest` |
| 50 | `prompt-mismatch` |
| 51 | `prompt-stale` |
| 60 | `terminal-state` |
| 61 | `terminal-ordering` |
| 70 | `read-only` |
| 71 | `not-ready` |
| 72 | `cancelled` |
| 255 | `internal` |
