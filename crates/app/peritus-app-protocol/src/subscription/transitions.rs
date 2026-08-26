//! Subscription delivery, acknowledgement, flow-control, and terminal transitions.

use crate::DeliveryAttemptId;
use peritus_types::EventId;

use super::{
    Acknowledgement, CancellationDisposition, Delivery, DeliveryAdmission, EventCursor,
    PauseReason, RegisteredEventFrame, SubscriptionCancellation, SubscriptionError,
    SubscriptionErrorKind, SubscriptionGap, SubscriptionPhase, SubscriptionState,
    acknowledgement_is_legal, cursor_is_successor, delivery_window_is_safe, error::reject,
};

impl SubscriptionState {
    /// Admits one new distinct event at the exact next cursor.
    ///
    /// # Errors
    ///
    /// Rejects delivery outside the active phase or cursor arithmetic overflow.
    pub fn deliver(
        &mut self,
        event_id: EventId,
        attempt_id: DeliveryAttemptId,
        frame: RegisteredEventFrame,
    ) -> Result<DeliveryAdmission, SubscriptionError> {
        if self.phase != SubscriptionPhase::Active {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "ordinary delivery requires an active subscription",
            ));
        }
        if self.in_flight.len() == self.maximum_in_flight {
            return Ok(DeliveryAdmission::Backpressured);
        }
        let cursor = self.last_delivered.checked_next()?;
        if !cursor_is_successor(self.last_delivered.get(), cursor.get()) {
            return Err(reject(
                SubscriptionErrorKind::NonContiguousDelivery,
                "new event cursor is not the exact successor",
            ));
        }
        let delivery = Delivery::new(self.id, event_id, cursor, attempt_id, 1, frame)?;
        self.last_delivered = cursor;
        self.in_flight.push(delivery.clone());
        Ok(DeliveryAdmission::Delivered(delivery))
    }

    /// Redelivers one in-flight event while preserving event, cursor, frame, and digest.
    ///
    /// # Errors
    ///
    /// Rejects an unknown cursor, a non-active phase, or attempt counter overflow.
    pub fn redeliver(
        &mut self,
        cursor: EventCursor,
        attempt_id: DeliveryAttemptId,
    ) -> Result<Delivery, SubscriptionError> {
        if self.phase != SubscriptionPhase::Active {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "redelivery requires an active subscription",
            ));
        }
        let delivery =
            self.in_flight.iter_mut().find(|delivery| delivery.cursor == cursor).ok_or_else(
                || {
                    reject(
                        SubscriptionErrorKind::UnknownDelivery,
                        "redelivery cursor is not currently in flight",
                    )
                },
            )?;
        delivery.attempt = delivery.attempt.checked_add(1).ok_or_else(|| {
            reject(SubscriptionErrorKind::ArithmeticOverflow, "delivery attempt counter overflow")
        })?;
        delivery.attempt_id = attempt_id;
        Ok(delivery.clone())
    }

    /// Applies a cumulative acknowledgement and returns the exact released prefix length.
    ///
    /// # Errors
    ///
    /// Rejects wrong-subscription, regression, future, or gap-crossing acknowledgements.
    pub fn acknowledge(
        &mut self,
        acknowledgement: Acknowledgement,
    ) -> Result<usize, SubscriptionError> {
        if acknowledgement.subscription_id != self.id {
            return Err(reject(
                SubscriptionErrorKind::BindingMismatch,
                "acknowledgement names another subscription",
            ));
        }
        if matches!(self.phase, SubscriptionPhase::SnapshotRequired(_)) {
            return Err(reject(
                SubscriptionErrorKind::AcknowledgementAcrossGap,
                "acknowledgement cannot cross a declared gap",
            ));
        }
        if matches!(self.phase, SubscriptionPhase::Cancelled(_)) {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "cancelled subscription cannot accept acknowledgements",
            ));
        }
        if acknowledgement.cursor < self.last_acknowledged {
            return Err(reject(
                SubscriptionErrorKind::AcknowledgementRegression,
                "cumulative acknowledgement regresses",
            ));
        }
        if acknowledgement.cursor > self.last_delivered {
            return Err(reject(
                SubscriptionErrorKind::AcknowledgementFuture,
                "cumulative acknowledgement exceeds delivered data",
            ));
        }
        if !acknowledgement_is_legal(
            self.last_acknowledged.get(),
            self.last_delivered.get(),
            acknowledgement.cursor.get(),
            false,
        ) {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "cumulative acknowledgement is not legal",
            ));
        }
        let release = self
            .in_flight
            .iter()
            .take_while(|delivery| delivery.cursor <= acknowledgement.cursor)
            .count();
        let retained = self.in_flight.len() - release;
        if !delivery_window_is_safe(
            acknowledgement.cursor.get(),
            self.last_delivered.get(),
            retained,
            self.maximum_in_flight,
        ) {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "acknowledgement would violate delivery-window accounting",
            ));
        }
        self.in_flight.drain(..release);
        self.last_acknowledged = acknowledgement.cursor;
        Ok(release)
    }

    /// Pauses new delivery for an explicit reason.
    ///
    /// # Errors
    ///
    /// Rejects pausing a gap-recovery or terminal subscription.
    pub const fn pause(&mut self, reason: PauseReason) -> Result<(), SubscriptionError> {
        match self.phase {
            SubscriptionPhase::Active | SubscriptionPhase::Paused(_) => {
                self.phase = SubscriptionPhase::Paused(reason);
                Ok(())
            }
            SubscriptionPhase::SnapshotRequired(_) | SubscriptionPhase::Cancelled(_) => Err(
                reject(SubscriptionErrorKind::IllegalTransition, "subscription cannot be paused"),
            ),
        }
    }

    /// Resumes an explicitly paused subscription.
    ///
    /// # Errors
    ///
    /// Rejects resume unless the subscription is paused.
    pub const fn resume(&mut self) -> Result<(), SubscriptionError> {
        if !matches!(self.phase, SubscriptionPhase::Paused(_)) {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "only a paused subscription can resume",
            ));
        }
        self.phase = SubscriptionPhase::Active;
        Ok(())
    }

    /// Declares an unsatisfied retention gap and enters snapshot-required state.
    ///
    /// # Errors
    ///
    /// Rejects a gap for another requested cursor, after delivery, or after terminal cancellation.
    pub fn declare_gap(&mut self, gap: SubscriptionGap) -> Result<(), SubscriptionError> {
        if gap.requested != self.requested {
            return Err(reject(
                SubscriptionErrorKind::BindingMismatch,
                "gap does not name this subscription's requested cursor",
            ));
        }
        if self.last_delivered != self.requested || !self.in_flight.is_empty() {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "retention gap must be declared before ordinary delivery",
            ));
        }
        if matches!(self.phase, SubscriptionPhase::Cancelled(_)) {
            return Err(reject(
                SubscriptionErrorKind::IllegalTransition,
                "cancelled subscription cannot declare a gap",
            ));
        }
        self.phase = SubscriptionPhase::SnapshotRequired(gap);
        Ok(())
    }

    /// Applies a correlated terminal cancellation idempotently.
    ///
    /// # Errors
    ///
    /// Rejects another subscription or a conflicting cancellation fact.
    pub fn cancel(
        &mut self,
        cancellation: SubscriptionCancellation,
    ) -> Result<CancellationDisposition, SubscriptionError> {
        if cancellation.subscription_id != self.id {
            return Err(reject(
                SubscriptionErrorKind::BindingMismatch,
                "cancellation names another subscription",
            ));
        }
        match self.phase {
            SubscriptionPhase::Cancelled(retained) if retained == cancellation => {
                Ok(CancellationDisposition::Repeated)
            }
            SubscriptionPhase::Cancelled(_) => Err(reject(
                SubscriptionErrorKind::TerminalConflict,
                "cancellation conflicts with the retained terminal fact",
            )),
            _ => {
                self.phase = SubscriptionPhase::Cancelled(cancellation);
                Ok(CancellationDisposition::Applied)
            }
        }
    }
}
