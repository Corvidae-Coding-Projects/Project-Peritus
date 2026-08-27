//! Connection-owned bounded subscription registry and journal tail pump.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use peritus_app_protocol::{
    Acknowledgement, AppEventEnvelope, AppEventPayload, AppMessage, AppProtocolLimits,
    DeliveryAdmission, DeliveryAttemptId, EventCursor, ProtocolContext, RegisteredEventFrame,
    SubscriptionBackpressure, SubscriptionCancellation, SubscriptionControl, SubscriptionGap,
    SubscriptionId, SubscriptionPhase, SubscriptionRequest, SubscriptionStarted, SubscriptionState,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::Instant,
};

use super::registry::CompiledFilter;
use crate::{AppFrameStream, AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

static ATTEMPT_NONCE: AtomicU64 = AtomicU64::new(1);
const REDELIVERY_INTERVAL: Duration = Duration::from_secs(2);

struct LiveSubscription {
    state: SubscriptionState,
    filter: CompiledFilter,
    next_redelivery: Instant,
}

/// All bounded event subscriptions owned by one authenticated connection.
pub(crate) struct SubscriptionRegistry {
    entries: BTreeMap<SubscriptionId, LiveSubscription>,
    maximum: usize,
}

impl SubscriptionRegistry {
    pub(crate) fn new(limits: AppProtocolLimits) -> Self {
        Self { entries: BTreeMap::new(), maximum: limits.max_topics() }
    }

    pub(crate) fn open(
        &mut self,
        request: &SubscriptionRequest,
        limits: AppProtocolLimits,
    ) -> Result<SubscriptionStarted, DaemonError> {
        if self.entries.len() == self.maximum
            || self.entries.contains_key(&request.subscription_id())
        {
            return Err(resource("subscription registry is full or identity is already active"));
        }
        let maximum = usize::try_from(request.maximum_in_flight())
            .map_err(|_| resource("subscription delivery window is unrepresentable"))?;
        if maximum > limits.max_in_flight_events() {
            return Err(resource("subscription delivery window exceeds negotiated limits"));
        }
        let filter = CompiledFilter::compile(request.filter())?;
        let state = SubscriptionState::new(
            request.subscription_id(),
            request.filter().clone(),
            request.after(),
            maximum,
        )
        .map_err(subscription_error)?;
        self.entries.insert(
            request.subscription_id(),
            LiveSubscription {
                state,
                filter,
                next_redelivery: Instant::now() + REDELIVERY_INTERVAL,
            },
        );
        Ok(SubscriptionStarted::new(
            request.subscription_id(),
            request.after(),
            request.maximum_in_flight(),
        ))
    }

    pub(crate) fn acknowledge(&mut self, value: Acknowledgement) -> Result<(), DaemonError> {
        let subscription = self
            .entries
            .get_mut(&value.subscription_id())
            .ok_or_else(|| invalid("acknowledgement names no active subscription"))?;
        subscription.state.acknowledge(value).map(|_| ()).map_err(subscription_error)
    }

    pub(crate) fn cancel(&mut self, value: SubscriptionCancellation) -> Result<(), DaemonError> {
        let subscription = self
            .entries
            .get_mut(&value.subscription_id())
            .ok_or_else(|| invalid("cancellation names no active subscription"))?;
        subscription.state.cancel(value).map_err(subscription_error)?;
        self.entries.remove(&value.subscription_id());
        Ok(())
    }

    pub(crate) fn control(&mut self, value: SubscriptionControl) -> Result<(), DaemonError> {
        let (id, pause) = match value {
            SubscriptionControl::Pause { subscription_id, reason } => {
                (subscription_id, Some(reason))
            }
            SubscriptionControl::Resume { subscription_id } => (subscription_id, None),
        };
        let subscription = self
            .entries
            .get_mut(&id)
            .ok_or_else(|| invalid("flow control names no active subscription"))?;
        match pause {
            Some(reason) => subscription.state.pause(reason),
            None => subscription.state.resume(),
        }
        .map_err(subscription_error)
    }

    pub(crate) async fn pump<S>(
        &mut self,
        frames: &mut AppFrameStream<S>,
        authority: &AuthorityHandle,
        context: ProtocolContext,
        limits: AppProtocolLimits,
    ) -> Result<(), DaemonError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        for subscription in self.entries.values_mut() {
            if subscription.state.phase() != SubscriptionPhase::Active {
                continue;
            }
            if Instant::now() >= subscription.next_redelivery
                && !subscription.state.in_flight().is_empty()
            {
                let cursors = subscription
                    .state
                    .in_flight()
                    .iter()
                    .map(|delivery| delivery.cursor())
                    .collect::<Vec<_>>();
                for cursor in cursors {
                    let delivery = subscription
                        .state
                        .redeliver(cursor, next_attempt(subscription.state.id(), cursor))
                        .map_err(subscription_error)?;
                    frames
                        .write(&AppMessage::Event(AppEventEnvelope::new(
                            context,
                            AppEventPayload::DomainEvent(delivery),
                        )))
                        .await?;
                }
                subscription.next_redelivery = Instant::now() + REDELIVERY_INTERVAL;
                continue;
            }

            let scanned = subscription.state.scanned_cursor().get();
            let window =
                authority.global_events_after(scanned, limits.max_in_flight_events()).await?;
            if window.has_retention_gap_after(subscription.state.requested_cursor().get())
                && subscription.state.scanned_cursor() == subscription.state.requested_cursor()
            {
                let gap = SubscriptionGap::new(
                    subscription.state.requested_cursor(),
                    EventCursor::new(window.earliest()),
                    EventCursor::new(window.latest()),
                )
                .map_err(subscription_error)?;
                subscription.state.declare_gap(gap).map_err(subscription_error)?;
                frames
                    .write(&AppMessage::Event(AppEventEnvelope::new(
                        context,
                        AppEventPayload::SubscriptionGap {
                            subscription_id: subscription.state.id(),
                            gap,
                        },
                    )))
                    .await?;
                continue;
            }
            for record in window.records() {
                let cursor = EventCursor::new(record.global_position());
                if subscription.filter.matches(record) {
                    let event_frame =
                        RegisteredEventFrame::new(record.frame_bytes().to_vec(), limits.codec())
                            .map_err(subscription_error)?;
                    match subscription
                        .state
                        .deliver(
                            cursor,
                            record.event_id(),
                            next_attempt(subscription.state.id(), cursor),
                            event_frame,
                        )
                        .map_err(subscription_error)?
                    {
                        DeliveryAdmission::Delivered(delivery) => {
                            frames
                                .write(&AppMessage::Event(AppEventEnvelope::new(
                                    context,
                                    AppEventPayload::DomainEvent(delivery),
                                )))
                                .await?;
                        }
                        DeliveryAdmission::Backpressured => {
                            let maximum = u32::try_from(subscription.state.maximum_in_flight())
                                .map_err(|_| resource("subscription window is unrepresentable"))?;
                            frames
                                .write(&AppMessage::Event(AppEventEnvelope::new(
                                    context,
                                    AppEventPayload::Backpressure(SubscriptionBackpressure::new(
                                        subscription.state.id(),
                                        subscription.state.last_delivered(),
                                        subscription.state.last_acknowledged(),
                                        maximum,
                                    )),
                                )))
                                .await?;
                            break;
                        }
                    }
                } else {
                    subscription.state.scan_to(cursor).map_err(subscription_error)?;
                }
            }
        }
        Ok(())
    }
}

fn next_attempt(subscription: SubscriptionId, cursor: EventCursor) -> DeliveryAttemptId {
    use sha2::{Digest, Sha256};
    let nonce = ATTEMPT_NONCE.fetch_add(1, Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/delivery-attempt/v1\0");
    hasher.update(subscription.as_bytes());
    hasher.update(cursor.get().to_be_bytes());
    hasher.update(nonce.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    DeliveryAttemptId::new(bytes).expect("domain-separated SHA-256 prefix is nonzero")
}

fn subscription_error(error: peritus_app_protocol::SubscriptionError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "advance application subscription",
        error.to_string(),
        error,
    )
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "advance application subscription",
        detail,
    )
}

fn resource(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::ResourceLimit,
        DaemonRecovery::Retry,
        "advance application subscription",
        detail,
    )
}
