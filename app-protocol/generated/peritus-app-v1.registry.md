# Peritus application protocol v1 registry

Generated from Rust metadata. Numeric and semantic allocations are append-only.

## Families

| Tag | Family | Schema | Payloads |
|---:|---|---:|---|
| 94 | `app-client-hello` | 1 | `1:client-hello` |
| 95 | `app-server-hello` | 1 | `1:compatible`, `2:downgraded`, `3:incompatible` |
| 96 | `app-request` | 1 | `1:submit-command`, `2:subscribe`, `3:open-artifact`, `4:cancel-artifact`, `5:answer-prompt`, `6:cancel-prompt`, `7:attach-terminal`, `8:terminal-input`, `9:terminal-resize`, `10:detach-terminal`, `11:cancel-terminal`, `12:daemon-status`, `13:shutdown`, `14:begin-artifact-upload`, `15:upload-artifact-chunk`, `16:complete-artifact-upload`, `17:start-product-run`, `18:control-product-run`, `19:query-product-runs`, `20:continue-product-run`, `21:query-product-run-conversation` |
| 97 | `app-response` | 1 | `1:command-result`, `2:subscription-started`, `3:artifact-opened`, `4:prompt-accepted`, `5:terminal-attached`, `6:acknowledged`, `7:daemon-status`, `8:shutdown-accepted`, `9:error`, `10:product-run-accepted`, `11:product-runs`, `12:product-run-conversation`, `13:product-run-settled`, `14:product-run-settlements` |
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
