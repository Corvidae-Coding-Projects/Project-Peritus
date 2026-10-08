//! Polling and daemon-observation updates for the product-run screen.

use std::collections::VecDeque;

use peritus_app_protocol::{
    AppRequestPayload, ProductArtifactPage, ProductArtifactQuery, ProductArtifactReference,
    ProductDeliverable, ProductDeliverableIndexKind, ProductDeliverableIndexPage,
    ProductDeliverableIndexQuery, ProductDeliverableIndexReference, ProductInteractionQuery,
    ProductInteractionSnapshot, ProductRunOperation, ProductRunPageCursor,
    ProductRunReferencePage, ProductRunReferenceQuery, ProductRunReferenceSnapshot,
    ProductRunSettlementSnapshot, ProductRunSnapshot,
    product_deliverable_index_reference,
};
use peritus_run_settlement::RunSettlement;
use peritus_types::RunId;

use super::ProductUi;
use crate::{
    action::Effect,
    model::{AppModel, PendingRequest},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProductHydrationTarget {
    List,
    Control,
}

#[derive(Debug)]
pub(super) struct ProductHydration {
    target: ProductHydrationTarget,
    remaining: VecDeque<ProductRunReferenceSnapshot>,
    current: Option<RunHydration>,
    completed: Vec<(ProductRunSnapshot, Option<RunSettlement>)>,
    next: Option<ProductRunPageCursor>,
    continuation: Option<ProductRunPageCursor>,
}

#[derive(Debug)]
struct RunHydration {
    reference: ProductRunReferenceSnapshot,
    task: String,
    status: String,
    diff: String,
    gates: String,
    review: String,
    summary: String,
    operation_identity: String,
    operation_known: String,
    operation_uncertainty: String,
    workspace: String,
    changed_paths: Vec<String>,
    successful_commands: Vec<String>,
    instructions: String,
    commit: String,
    export: String,
    step: HydrationStep,
}

#[derive(Debug)]
enum HydrationStep {
    Text(TextRead),
    Index(IndexRead),
    Finish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextField {
    Task,
    Status,
    Diff,
    Gates,
    Review,
    Summary,
    OperationIdentity,
    OperationKnown,
    OperationUncertainty,
    Workspace,
    Instructions,
    Commit,
    Export,
}

#[derive(Debug)]
struct TextRead {
    field: TextField,
    reference: ProductArtifactReference,
    offset: u64,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct IndexRead {
    kind: ProductDeliverableIndexKind,
    reference: ProductDeliverableIndexReference,
    after: Option<u64>,
    values: Vec<String>,
    entries: VecDeque<(u64, ProductArtifactReference)>,
    next: Option<u64>,
    page_received: bool,
    active: Option<(u64, TextRead)>,
}

enum HydrationRequest {
    References(ProductRunReferenceQuery),
    Artifact(ProductArtifactQuery),
    Index(ProductDeliverableIndexQuery),
}

impl ProductHydration {
    fn new(
        target: ProductHydrationTarget,
        page: &ProductRunReferencePage,
    ) -> Result<Self, &'static str> {
        if target == ProductHydrationTarget::Control
            && (page.entries().len() != 1 || page.next().is_some())
        {
            return Err("control response did not identify exactly one product run");
        }
        Ok(Self {
            target,
            remaining: page
                .entries()
                .iter()
                .map(|entry| entry.snapshot().clone())
                .collect(),
            current: None,
            completed: Vec::with_capacity(page.entries().len()),
            next: page.next(),
            continuation: None,
        })
    }

    fn next_request(&mut self) -> Result<Option<HydrationRequest>, &'static str> {
        loop {
            if self.current.is_none() {
                let Some(reference) = self.remaining.pop_front() else {
                    if let Some(cursor) = self.next.take() {
                        self.continuation = Some(cursor);
                        return Ok(Some(HydrationRequest::References(
                            ProductRunReferenceQuery::after(cursor),
                        )));
                    }
                    if self.continuation.is_some() {
                        return Err("product run continuation is already pending");
                    }
                    return Ok(None);
                };
                self.current = Some(RunHydration::new(reference));
            }
            if self
                .current
                .as_ref()
                .is_some_and(|current| matches!(&current.step, HydrationStep::Finish))
            {
                let finished = self
                    .current
                    .take()
                    .ok_or("product hydration lost its completed run")?
                    .finish()?;
                self.completed.push(finished);
                continue;
            }
            let current = self.current.as_mut().ok_or("product hydration lost its current run")?;
            match &mut current.step {
                HydrationStep::Text(read) => {
                    let query = ProductArtifactQuery::new(
                        current.reference.run_id(),
                        read.reference,
                        read.offset,
                    )
                    .map_err(|_| "product artifact continuation is invalid")?;
                    return Ok(Some(HydrationRequest::Artifact(query)));
                }
                HydrationStep::Index(index) => {
                    if let Some((_, read)) = &index.active {
                        let query = ProductArtifactQuery::new(
                            current.reference.run_id(),
                            read.reference,
                            read.offset,
                        )
                        .map_err(|_| "deliverable item continuation is invalid")?;
                        return Ok(Some(HydrationRequest::Artifact(query)));
                    }
                    if let Some((ordinal, reference)) = index.entries.pop_front() {
                        index.active = Some((
                            ordinal,
                            TextRead {
                                field: TextField::Task,
                                reference,
                                offset: 0,
                                bytes: Vec::new(),
                            },
                        ));
                        continue;
                    }
                    if index.page_received {
                        if let Some(next) = index.next.take() {
                            index.after = Some(next);
                            index.page_received = false;
                            continue;
                        }
                        if u64::try_from(index.values.len()).unwrap_or(u64::MAX)
                            != index.reference.count()
                        {
                            return Err("deliverable index ended at a different item count");
                        }
                        current.advance_after_index()?;
                        continue;
                    }
                    let query = ProductDeliverableIndexQuery::new(
                        current.reference.run_id(),
                        index.kind,
                        index.reference,
                        index.after,
                    )
                    .map_err(|_| "deliverable index continuation is invalid")?;
                    return Ok(Some(HydrationRequest::Index(query)));
                }
                HydrationStep::Finish => unreachable!("handled before borrowing the current run"),
            }
        }
    }

    fn accept_references(
        &mut self,
        expected: ProductRunReferenceQuery,
        page: &ProductRunReferencePage,
    ) -> Result<(), &'static str> {
        if self.target != ProductHydrationTarget::List {
            return Err("control hydration cannot continue through run history");
        }
        let cursor = self
            .continuation
            .take()
            .ok_or("product run continuation has no hydration owner")?;
        if expected != ProductRunReferenceQuery::after(cursor)
            || page.store() != cursor.store()
            || page.entries().is_empty()
            || page
                .entries()
                .first()
                .is_some_and(|entry| entry.sequence() >= cursor.after_sequence())
            || page.next().is_some_and(|next| {
                next.store() != cursor.store()
                    || next.highwater_sequence() != cursor.highwater_sequence()
                    || next.highwater_run() != cursor.highwater_run()
            })
        {
            return Err("product run continuation changed its durable catalog range");
        }
        let duplicate = page.entries().iter().any(|entry| {
            let run = entry.snapshot().run_id();
            self.current
                .as_ref()
                .is_some_and(|current| current.reference.run_id() == run)
                || self.remaining.iter().any(|value| value.run_id() == run)
                || self.completed.iter().any(|(value, _)| value.run_id() == run)
        });
        if duplicate {
            return Err("product run continuation repeated a prior run");
        }
        self.remaining.extend(
            page.entries()
                .iter()
                .map(|entry| entry.snapshot().clone()),
        );
        self.next = page.next();
        Ok(())
    }

    fn accept_artifact(
        &mut self,
        expected: ProductArtifactQuery,
        page: &ProductArtifactPage,
    ) -> Result<(), &'static str> {
        if page.query() != expected {
            return Err("product artifact response changed its requested range");
        }
        let current = self.current.as_mut().ok_or("product artifact has no hydration owner")?;
        let mut completed = None;
        match &mut current.step {
            HydrationStep::Text(read) => {
                if expected.source() != read.reference || expected.offset() != read.offset {
                    return Err("product artifact response is out of hydration order");
                }
                read.bytes.extend_from_slice(page.bytes());
                if let Some(next) = page.next() {
                    read.offset = next;
                } else {
                    if !read.reference.matches_bytes(&read.bytes) {
                        return Err("product artifact digest or length changed");
                    }
                    let text = String::from_utf8(std::mem::take(&mut read.bytes))
                        .map_err(|_| "product artifact is not UTF-8")?;
                    completed = Some((read.field, text));
                }
            }
            HydrationStep::Index(index) => {
                let (ordinal, read) = index
                    .active
                    .as_mut()
                    .ok_or("deliverable item response has no hydration owner")?;
                if expected.source() != read.reference || expected.offset() != read.offset {
                    return Err("deliverable item response is out of hydration order");
                }
                read.bytes.extend_from_slice(page.bytes());
                if let Some(next) = page.next() {
                    read.offset = next;
                } else {
                    if !read.reference.matches_bytes(&read.bytes) {
                        return Err("deliverable item digest or length changed");
                    }
                    let expected_ordinal = u64::try_from(index.values.len())
                        .map_err(|_| "deliverable item count is unrepresentable")?;
                    if *ordinal != expected_ordinal {
                        return Err("deliverable item response is not contiguous");
                    }
                    let text = String::from_utf8(std::mem::take(&mut read.bytes))
                        .map_err(|_| "deliverable item is not UTF-8")?;
                    index.values.push(text);
                    index.active = None;
                }
            }
            HydrationStep::Finish => return Err("product artifact arrived after hydration"),
        }
        if let Some((field, text)) = completed {
            current.advance_after_text(field, text)?;
        }
        Ok(())
    }

    fn accept_index(
        &mut self,
        expected: ProductDeliverableIndexQuery,
        page: &ProductDeliverableIndexPage,
    ) -> Result<(), &'static str> {
        if page.query() != expected {
            return Err("deliverable index response changed its requested range");
        }
        let current = self.current.as_mut().ok_or("deliverable index has no hydration owner")?;
        let HydrationStep::Index(index) = &mut current.step else {
            return Err("deliverable index response is out of hydration order");
        };
        if index.kind != expected.kind()
            || index.reference != expected.index()
            || index.after != expected.after()
            || index.page_received
            || index.active.is_some()
            || !index.entries.is_empty()
        {
            return Err("deliverable index response is out of hydration order");
        }
        let first = u64::try_from(index.values.len())
            .map_err(|_| "deliverable item count is unrepresentable")?;
        for (position, entry) in page.entries().iter().enumerate() {
            let position = u64::try_from(position)
                .map_err(|_| "deliverable page position is unrepresentable")?;
            if first.checked_add(position) != Some(entry.ordinal()) {
                return Err("deliverable index page is not contiguous with prior pages");
            }
            index.entries.push_back((entry.ordinal(), entry.value()));
        }
        index.next = page.next();
        index.page_received = true;
        Ok(())
    }
}

impl RunHydration {
    fn new(reference: ProductRunReferenceSnapshot) -> Self {
        let task = reference.task();
        Self {
            reference,
            task: String::new(),
            status: String::new(),
            diff: String::new(),
            gates: String::new(),
            review: String::new(),
            summary: String::new(),
            operation_identity: String::new(),
            operation_known: String::new(),
            operation_uncertainty: String::new(),
            workspace: String::new(),
            changed_paths: Vec::new(),
            successful_commands: Vec::new(),
            instructions: String::new(),
            commit: String::new(),
            export: String::new(),
            step: HydrationStep::Text(TextRead {
                field: TextField::Task,
                reference: task,
                offset: 0,
                bytes: Vec::new(),
            }),
        }
    }

    fn advance_after_text(
        &mut self,
        field: TextField,
        text: String,
    ) -> Result<(), &'static str> {
        match field {
            TextField::Task => self.task = text,
            TextField::Status => self.status = text,
            TextField::Diff => self.diff = text,
            TextField::Gates => self.gates = text,
            TextField::Review => self.review = text,
            TextField::Summary => self.summary = text,
            TextField::OperationIdentity => self.operation_identity = text,
            TextField::OperationKnown => self.operation_known = text,
            TextField::OperationUncertainty => self.operation_uncertainty = text,
            TextField::Workspace => self.workspace = text,
            TextField::Instructions => self.instructions = text,
            TextField::Commit => self.commit = text,
            TextField::Export => self.export = text,
        }
        let next = match field {
            TextField::Task => Some(TextField::Status),
            TextField::Status => Some(TextField::Diff),
            TextField::Diff => Some(TextField::Gates),
            TextField::Gates => Some(TextField::Review),
            TextField::Review => Some(TextField::Summary),
            TextField::Summary => Some(TextField::OperationIdentity),
            TextField::OperationIdentity => Some(TextField::OperationKnown),
            TextField::OperationKnown => Some(TextField::OperationUncertainty),
            TextField::OperationUncertainty => {
                self.reference.deliverable().map(|_| TextField::Workspace)
            }
            TextField::Workspace => {
                let deliverable = self
                    .reference
                    .deliverable()
                    .ok_or("product deliverable reference disappeared")?;
                self.step = HydrationStep::Index(IndexRead::new(
                    ProductDeliverableIndexKind::ChangedPaths,
                    deliverable.changed_paths(),
                ));
                return Ok(());
            }
            TextField::Instructions => Some(TextField::Commit),
            TextField::Commit => Some(TextField::Export),
            TextField::Export => None,
        };
        self.advance_to_optional_text(next)
    }

    fn advance_to_optional_text(
        &mut self,
        mut field: Option<TextField>,
    ) -> Result<(), &'static str> {
        while let Some(candidate) = field {
            if let Some(reference) = self.reference_for(candidate)? {
                self.step = HydrationStep::Text(TextRead {
                    field: candidate,
                    reference,
                    offset: 0,
                    bytes: Vec::new(),
                });
                return Ok(());
            }
            field = match candidate {
                TextField::Diff => Some(TextField::Gates),
                TextField::Gates => Some(TextField::Review),
                TextField::Review => Some(TextField::Summary),
                TextField::Summary => Some(TextField::OperationIdentity),
                TextField::OperationUncertainty => {
                    self.reference.deliverable().map(|_| TextField::Workspace)
                }
                TextField::Commit => Some(TextField::Export),
                TextField::Export => None,
                _ => return Err("required product artifact reference is absent"),
            };
        }
        self.step = HydrationStep::Finish;
        Ok(())
    }

    fn reference_for(
        &self,
        field: TextField,
    ) -> Result<Option<ProductArtifactReference>, &'static str> {
        let deliverable = self.reference.deliverable();
        Ok(match field {
            TextField::Task => Some(self.reference.task()),
            TextField::Status => Some(self.reference.status()),
            TextField::Diff => self.reference.diff(),
            TextField::Gates => self.reference.gates(),
            TextField::Review => self.reference.review(),
            TextField::Summary => self.reference.summary(),
            TextField::OperationIdentity => Some(self.reference.operation().identity()),
            TextField::OperationKnown => Some(self.reference.operation().known()),
            TextField::OperationUncertainty => self.reference.operation().uncertainty(),
            TextField::Workspace => Some(
                deliverable
                    .ok_or("product deliverable reference is absent")?
                    .workspace_path(),
            ),
            TextField::Instructions => Some(
                deliverable
                    .ok_or("product deliverable reference is absent")?
                    .run_instructions(),
            ),
            TextField::Commit => deliverable.and_then(|value| value.commit_revision()),
            TextField::Export => deliverable.and_then(|value| value.export_path()),
        })
    }

    fn advance_after_index(&mut self) -> Result<(), &'static str> {
        let HydrationStep::Index(index) =
            std::mem::replace(&mut self.step, HydrationStep::Finish)
        else {
            return Err("product hydration index state disappeared");
        };
        if product_deliverable_index_reference(index.kind, &index.values)
            .map_err(|_| "deliverable index root is invalid")?
            != index.reference
        {
            return Err("deliverable index root or count changed");
        }
        match index.kind {
            ProductDeliverableIndexKind::ChangedPaths => {
                self.changed_paths = index.values;
                let deliverable = self
                    .reference
                    .deliverable()
                    .ok_or("product deliverable reference disappeared")?;
                self.step = HydrationStep::Index(IndexRead::new(
                    ProductDeliverableIndexKind::SuccessfulCommands,
                    deliverable.successful_commands(),
                ));
            }
            ProductDeliverableIndexKind::SuccessfulCommands => {
                self.successful_commands = index.values;
                self.advance_to_optional_text(Some(TextField::Instructions))?;
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<(ProductRunSnapshot, Option<RunSettlement>), &'static str> {
        let operation = ProductRunOperation::new(
            self.reference.operation().kind(),
            self.reference.operation().state(),
            self.operation_identity,
            self.operation_known,
            self.operation_uncertainty,
            self.reference.operation().legal_controls(),
        )
        .map_err(|_| "hydrated product operation is invalid")?;
        let mut snapshot = ProductRunSnapshot::new(
            self.reference.run_id(),
            self.reference.workspace_id(),
            self.reference.providers(),
            self.reference.phase(),
            self.reference.cycle(),
            self.task,
            self.status,
            self.diff,
            self.gates,
            self.review,
            self.summary,
            operation,
        )
        .map_err(|_| "hydrated product snapshot is invalid")?;
        if let Some(reference) = self.reference.deliverable() {
            let mut deliverable = ProductDeliverable::candidate(
                self.workspace,
                self.changed_paths,
                self.successful_commands,
                self.instructions,
                reference.qualification(),
            )
            .map_err(|_| "hydrated product deliverable is invalid")?;
            if reference.accepted() {
                deliverable = deliverable.mark_accepted();
            }
            if reference.commit_revision().is_some() {
                deliverable = deliverable
                    .mark_committed(self.commit)
                    .map_err(|_| "hydrated commit revision is invalid")?;
            }
            if reference.export_path().is_some() {
                deliverable = deliverable
                    .mark_exported(self.export)
                    .map_err(|_| "hydrated export path is invalid")?;
            }
            if reference.discarded() {
                deliverable = deliverable.mark_discarded();
            }
            snapshot = snapshot.with_deliverable(deliverable);
        }
        Ok((snapshot, self.reference.settlement()))
    }
}

impl IndexRead {
    fn new(
        kind: ProductDeliverableIndexKind,
        reference: ProductDeliverableIndexReference,
    ) -> Self {
        Self {
            kind,
            reference,
            after: None,
            values: Vec::new(),
            entries: VecDeque::new(),
            next: None,
            page_received: false,
            active: None,
        }
    }
}

impl AppModel {
    pub(in crate::model) fn accept_observation_query(
        &mut self,
        observations: &[peritus_app_protocol::ProductRunObservation],
        exact: Option<RunId>,
    ) {
        let snapshots =
            observations.iter().map(|value| value.snapshot().clone()).collect::<Vec<_>>();
        self.accept_product_query(&snapshots, exact);
        if let Some(product) = &mut self.product {
            for value in observations
                .iter()
                .filter(|value| exact.is_none_or(|id| id == value.snapshot().run_id()))
            {
                let run = value.snapshot().run_id();
                if let Some(settlement) = value.settlement() {
                    product.settlements.insert(run, settlement);
                } else {
                    product.settlements.remove(&run);
                }
            }
        }
    }

    pub(in crate::model) fn accept_product_query(
        &mut self,
        snapshots: &[ProductRunSnapshot],
        exact: Option<RunId>,
    ) {
        if let Some(run_id) = exact {
            if let Some(snapshot) = snapshots.iter().find(|value| value.run_id() == run_id) {
                self.accept_product_run(snapshot.clone());
            }
        } else {
            self.accept_product_runs(snapshots.to_vec());
        }
    }

    pub(in crate::model) fn poll_product_runs(&mut self) -> Vec<Effect> {
        if self.product.is_none() || self.context.is_none() {
            return Vec::new();
        }
        let mut effects: Vec<Effect> = self.poll_chat();
        if self.view == crate::model::View::Preview
            && let Some(page) = self.product.as_ref().and_then(|product| product.preview.as_ref())
            && page
                .launches()
                .iter()
                .any(|launch| launch.state() == peritus_app_protocol::WorkbenchLaunchState::Running)
        {
            effects.extend(self.refresh_preview(page.query()));
        }
        if self.product_hydration.is_some()
            || self.pending.values().any(|pending| {
                matches!(
                    pending,
                    PendingRequest::ProductQuery
                        | PendingRequest::ProductExactQuery(_)
                        | PendingRequest::ProductReferenceContinuation(_)
                        | PendingRequest::ProductArtifactHydration(_)
                        | PendingRequest::ProductIndexHydration(_)
                        | PendingRequest::ProductControl
                )
            })
        {
            return effects;
        }
        effects.extend(self.request(
            AppRequestPayload::QueryProductRunReferences(ProductRunReferenceQuery::first()),
            PendingRequest::ProductQuery,
        ));
        if !self
            .pending
            .values()
            .any(|pending| {
                matches!(
                    pending,
                    PendingRequest::ProductInteractionQuery
                        | PendingRequest::ProductActivityPage(_)
                )
            })
            && let Some(run_id) = self
                .product
                .as_ref()
                .and_then(ProductUi::selected_run)
                .map(ProductRunSnapshot::run_id)
        {
            if self.supports_activity_pages() {
                effects.extend(self.request_activity_history(run_id));
            } else if let Some(effect) = self.request(
                AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run_id)),
                PendingRequest::ProductInteractionQuery,
            ) {
                effects.push(effect);
            }
        }
        effects
    }

    pub(in crate::model) fn begin_product_hydration(
        &mut self,
        page: &ProductRunReferencePage,
        target: ProductHydrationTarget,
    ) -> Vec<Effect> {
        self.pending.retain(|_, pending| {
            !matches!(
                pending,
                PendingRequest::ProductArtifactHydration(_)
                    | PendingRequest::ProductIndexHydration(_)
                    | PendingRequest::ProductReferenceContinuation(_)
            )
        });
        self.pending_started.retain(|request, _| self.pending.contains_key(request));
        match ProductHydration::new(target, page) {
            Ok(hydration) => self.product_hydration = Some(hydration),
            Err(error) => {
                self.product_hydration = None;
                self.notice(NoticeLevel::Error, error);
                return Vec::new();
            }
        }
        self.drive_product_hydration()
    }

    pub(in crate::model) fn accept_product_reference_page(
        &mut self,
        expected: ProductRunReferenceQuery,
        page: &ProductRunReferencePage,
    ) -> Vec<Effect> {
        let result = self
            .product_hydration
            .as_mut()
            .ok_or("product run continuation has no active hydration")
            .and_then(|hydration| hydration.accept_references(expected, page));
        if let Err(error) = result {
            self.product_hydration = None;
            self.notice(NoticeLevel::Error, error);
            return Vec::new();
        }
        self.drive_product_hydration()
    }

    pub(in crate::model) fn accept_product_artifact_page(
        &mut self,
        expected: ProductArtifactQuery,
        page: &ProductArtifactPage,
    ) -> Vec<Effect> {
        let result = self
            .product_hydration
            .as_mut()
            .ok_or("product artifact response has no active hydration")
            .and_then(|hydration| hydration.accept_artifact(expected, page));
        if let Err(error) = result {
            self.product_hydration = None;
            self.notice(NoticeLevel::Error, error);
            return Vec::new();
        }
        self.drive_product_hydration()
    }

    pub(in crate::model) fn accept_product_index_page(
        &mut self,
        expected: ProductDeliverableIndexQuery,
        page: &ProductDeliverableIndexPage,
    ) -> Vec<Effect> {
        let result = self
            .product_hydration
            .as_mut()
            .ok_or("deliverable index response has no active hydration")
            .and_then(|hydration| hydration.accept_index(expected, page));
        if let Err(error) = result {
            self.product_hydration = None;
            self.notice(NoticeLevel::Error, error);
            return Vec::new();
        }
        self.drive_product_hydration()
    }

    fn drive_product_hydration(&mut self) -> Vec<Effect> {
        let request = match self.product_hydration.as_mut() {
            Some(hydration) => match hydration.next_request() {
                Ok(request) => request,
                Err(error) => {
                    self.product_hydration = None;
                    self.notice(NoticeLevel::Error, error);
                    return Vec::new();
                }
            },
            None => return Vec::new(),
        };
        match request {
            Some(HydrationRequest::References(query)) => self
                .request(
                    AppRequestPayload::QueryProductRunReferences(query),
                    PendingRequest::ProductReferenceContinuation(query),
                )
                .into_iter()
                .collect(),
            Some(HydrationRequest::Artifact(query)) => self
                .request(
                    AppRequestPayload::QueryProductArtifact(query),
                    PendingRequest::ProductArtifactHydration(query),
                )
                .into_iter()
                .collect(),
            Some(HydrationRequest::Index(query)) => self
                .request(
                    AppRequestPayload::QueryProductDeliverableIndex(query),
                    PendingRequest::ProductIndexHydration(query),
                )
                .into_iter()
                .collect(),
            None => {
                let hydration = self
                    .product_hydration
                    .take()
                    .expect("completed product hydration is present");
                let snapshots = hydration
                    .completed
                    .iter()
                    .map(|(snapshot, _)| snapshot.clone())
                    .collect::<Vec<_>>();
                match hydration.target {
                    ProductHydrationTarget::List => {
                        self.accept_product_runs(snapshots);
                        if let Some(product) = &mut self.product {
                            product.settlements.clear();
                            for (snapshot, settlement) in hydration.completed {
                                if let Some(settlement) = settlement {
                                    product.settlements.insert(snapshot.run_id(), settlement);
                                }
                            }
                        }
                    }
                    ProductHydrationTarget::Control => {
                        let Some((snapshot, settlement)) = hydration.completed.into_iter().next()
                        else {
                            self.notice(
                                NoticeLevel::Error,
                                "controlled product run disappeared during retrieval",
                            );
                            return Vec::new();
                        };
                        let status = snapshot.status().to_owned();
                        self.accept_product_run(snapshot.clone());
                        if let Some(product) = &mut self.product {
                            if let Some(settlement) = settlement {
                                product.settlements.insert(snapshot.run_id(), settlement);
                            } else {
                                product.settlements.remove(&snapshot.run_id());
                            }
                            product.confirmation = None;
                        }
                        self.notice(NoticeLevel::Info, format!("coding run: {status}"));
                    }
                }
                Vec::new()
            }
        }
    }

    pub(in crate::model) fn accept_product_runs(&mut self, snapshots: Vec<ProductRunSnapshot>) {
        if let Some(product) = &mut self.product {
            let selected = product.selected_run().map(ProductRunSnapshot::run_id);
            product.runs = snapshots;
            product.selected = selected
                .and_then(|id| product.runs.iter().position(|run| run.run_id() == id))
                .unwrap_or_else(|| product.selected.min(product.runs.len().saturating_sub(1)));
            product
                .settlements
                .retain(|run_id, _| product.runs.iter().any(|run| run.run_id() == *run_id));
        }
    }

    pub(in crate::model) fn accept_product_run(&mut self, snapshot: ProductRunSnapshot) {
        let Some(product) = &mut self.product else { return };
        if let Some(existing) =
            product.runs.iter_mut().find(|run| run.run_id() == snapshot.run_id())
        {
            *existing = snapshot;
        } else {
            product.runs.insert(0, snapshot);
            product.selected = 0;
        }
    }

    pub(in crate::model) fn accept_product_settlement(
        &mut self,
        settled: &ProductRunSettlementSnapshot,
    ) {
        let run_id = settled.snapshot().run_id();
        self.accept_product_run(settled.snapshot().clone());
        if let Some(product) = &mut self.product {
            product.settlements.insert(run_id, *settled.settlement());
            product.confirmation = None;
        }
    }

    pub(in crate::model) fn accept_product_interaction(
        &mut self,
        conversation: ProductInteractionSnapshot,
    ) {
        let Some(product) = &mut self.product else { return };
        if product
            .selected_run()
            .is_some_and(|run| run.run_id() == conversation.snapshot().run_id())
        {
            product.conversation = Some(conversation);
        }
    }
}
