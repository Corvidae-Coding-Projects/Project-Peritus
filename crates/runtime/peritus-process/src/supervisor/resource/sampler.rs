//! Dedicated bounded resource-observation worker.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::platform::{self, ProcessTreeIdentity};

use super::disk::{DiskIndex, DiskObservation};

const PROCESS_INTERVAL: Duration = Duration::from_millis(20);
const DISK_SLICE_INTERVAL: Duration = Duration::from_millis(50);
const DISK_VERIFICATION_INTERVAL: Duration = Duration::from_secs(1);
const DISK_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const DISK_ENTRIES_PER_SLICE: usize = 128;
const REAPER_POLL_INTERVAL: Duration = Duration::from_millis(100);

static OBSERVER_REAPER: OnceLock<Result<ObserverReaper, std::io::Error>> = OnceLock::new();

struct ObserverReaper {
    sender: mpsc::Sender<thread::JoinHandle<()>>,
    _task: Mutex<Option<thread::JoinHandle<()>>>,
    retained_after_failure: Mutex<Vec<thread::JoinHandle<()>>>,
}

impl ObserverReaper {
    fn start() -> Result<Self, std::io::Error> {
        let (sender, receiver) = mpsc::channel::<thread::JoinHandle<()>>();
        let task = thread::Builder::new()
            .name("peritus-resource-observer-reaper".to_owned())
            .spawn(move || reap_observers(&receiver))?;
        Ok(Self {
            sender,
            _task: Mutex::new(Some(task)),
            retained_after_failure: Mutex::new(Vec::new()),
        })
    }

    fn retain(&self, observer: thread::JoinHandle<()>) {
        if let Err(error) = self.sender.send(observer) {
            self.retained_after_failure
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(error.0);
        }
    }
}

fn reap_observers(receiver: &mpsc::Receiver<thread::JoinHandle<()>>) {
    let mut observers = Vec::new();
    let mut connected = true;
    while connected || !observers.is_empty() {
        match receiver.recv_timeout(REAPER_POLL_INTERVAL) {
            Ok(observer) => observers.push(observer),
            Err(mpsc::RecvTimeoutError::Disconnected) => connected = false,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        while let Ok(observer) = receiver.try_recv() {
            observers.push(observer);
        }
        let mut index = 0;
        while index < observers.len() {
            if observers[index].is_finished() {
                let observer = observers.swap_remove(index);
                let _ = observer.join();
            } else {
                index += 1;
            }
        }
    }
}

fn observer_reaper() -> Result<&'static ObserverReaper, std::io::Error> {
    match OBSERVER_REAPER.get_or_init(ObserverReaper::start) {
        Ok(reaper) => Ok(reaper),
        Err(error) => Err(std::io::Error::new(error.kind(), error.to_string())),
    }
}

#[derive(Default)]
pub(super) struct SamplerUpdate {
    pub(super) process: Option<SamplerProcessSample>,
    pub(super) disk_growth: Option<SampleValue>,
}

impl SamplerUpdate {
    pub(super) fn is_empty(&self) -> bool {
        self.process.is_none() && self.disk_growth.is_none()
    }

    fn merge(&mut self, newer: Self) {
        if let Some(newer) = newer.process {
            if let Some(current) = self.process.as_mut() {
                current.merge(newer);
            } else {
                self.process = Some(newer);
            }
        }
        if let Some(newer) = newer.disk_growth {
            if let Some(current) = self.disk_growth.as_mut() {
                current.merge(newer);
            } else {
                self.disk_growth = Some(newer);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SamplerProcessSample {
    cpu_millis: SampleValue,
    memory_bytes: SampleValue,
    process_count: SampleValue,
    open_handles: SampleValue,
}

impl SamplerProcessSample {
    const fn unavailable() -> Self {
        Self {
            cpu_millis: SampleValue::unavailable(),
            memory_bytes: SampleValue::unavailable(),
            process_count: SampleValue::unavailable(),
            open_handles: SampleValue::unavailable(),
        }
    }

    fn merge(&mut self, newer: Self) {
        self.cpu_millis.merge(newer.cpu_millis);
        self.memory_bytes.merge(newer.memory_bytes);
        self.process_count.merge(newer.process_count);
        self.open_handles.merge(newer.open_handles);
    }

    pub(super) const fn cpu_millis(self) -> SampleValue {
        self.cpu_millis
    }

    pub(super) const fn memory_bytes(self) -> SampleValue {
        self.memory_bytes
    }

    pub(super) const fn process_count(self) -> SampleValue {
        self.process_count
    }

    pub(super) const fn open_handles(self) -> SampleValue {
        self.open_handles
    }
}

#[derive(Clone, Copy)]
pub(super) struct SampleValue {
    greatest: Option<u64>,
    proven_greatest: Option<u64>,
    available: bool,
}

impl SampleValue {
    const fn new(value: Option<u64>) -> Self {
        Self { greatest: value, proven_greatest: value, available: value.is_some() }
    }

    const fn incomplete(value: Option<u64>) -> Self {
        Self { greatest: value, proven_greatest: None, available: false }
    }

    const fn unavailable() -> Self {
        Self::incomplete(None)
    }

    fn merge(&mut self, newer: Self) {
        if let Some(value) = newer.greatest {
            self.greatest = Some(self.greatest.map_or(value, |current| current.max(value)));
        }
        if let Some(value) = newer.proven_greatest {
            self.proven_greatest =
                Some(self.proven_greatest.map_or(value, |current| current.max(value)));
        }
        self.available &= newer.available;
    }

    pub(super) const fn greatest(self) -> Option<u64> {
        self.greatest
    }

    pub(super) const fn available(self) -> bool {
        self.available
    }

    pub(super) const fn proven_greatest(self) -> Option<u64> {
        self.proven_greatest
    }
}

/// One worker whose latest observations overwrite older undrained observations.
pub(super) struct ResourceSampler {
    tree: Arc<Mutex<Option<ProcessTreeIdentity>>>,
    latest: Arc<Mutex<SamplerUpdate>>,
    control: Option<mpsc::Sender<SamplerControl>>,
    task: Option<thread::JoinHandle<()>>,
    reaper: &'static ObserverReaper,
}

enum SamplerControl {
    Wake,
    Finish,
}

impl ResourceSampler {
    pub(super) fn start(workspace: PathBuf) -> std::io::Result<Self> {
        let reaper = observer_reaper()?;
        let tree = Arc::new(Mutex::new(None));
        let latest = Arc::new(Mutex::new(SamplerUpdate::default()));
        let (control_tx, control_rx) = mpsc::channel();
        let worker_tree = Arc::clone(&tree);
        let worker_latest = Arc::clone(&latest);
        let task = thread::Builder::new()
            .name("peritus-resource-sampler".to_owned())
            .spawn(move || run(workspace, &worker_tree, &worker_latest, &control_rx))?;
        Ok(Self { tree, latest, control: Some(control_tx), task: Some(task), reaper })
    }

    pub(super) fn attach(&self, tree: ProcessTreeIdentity) {
        let mut current = self.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if *current != Some(tree) {
            *current = Some(tree);
            if let Some(control) = &self.control {
                let _ = control.send(SamplerControl::Wake);
            }
        }
    }

    pub(super) fn poll(&self) -> SamplerUpdate {
        std::mem::take(
            &mut *self.latest.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    pub(super) fn request_finish(&mut self) -> SamplerUpdate {
        if let Some(control) = self.control.take() {
            let _ = control.send(SamplerControl::Finish);
        }
        self.poll()
    }

    pub(super) fn retire(&mut self) -> SamplerUpdate {
        let mut update = self.request_finish();
        let Some(task) = self.task.take() else {
            return update;
        };
        if task.is_finished() {
            let panicked = task.join().is_err();
            update.merge(self.poll());
            if panicked {
                update.merge(incomplete_update());
            }
        } else {
            update.merge(self.poll());
            update.merge(incomplete_update());
            self.reaper.retain(task);
        }
        update
    }
}

fn incomplete_update() -> SamplerUpdate {
    SamplerUpdate {
        process: Some(SamplerProcessSample::unavailable()),
        disk_growth: Some(SampleValue::unavailable()),
    }
}

impl Drop for ResourceSampler {
    fn drop(&mut self) {
        if self.task.is_some() {
            let _ = self.retire();
        }
    }
}

#[derive(Clone, Copy)]
struct DiskBaseline {
    bytes: u64,
    provenance: DiskBaselineProvenance,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DiskBaselineProvenance {
    BeforeTreeAttachment,
    AfterTreeAttachment,
}

fn run(
    workspace: PathBuf,
    tree: &Mutex<Option<ProcessTreeIdentity>>,
    latest: &Mutex<SamplerUpdate>,
    control: &mpsc::Receiver<SamplerControl>,
) {
    let mut disk = DiskIndex::new(workspace);
    let mut baseline_disk: Option<DiskBaseline> = None;
    let mut next_process = Instant::now();
    let mut next_disk = Instant::now();
    let mut finish_requested = false;
    loop {
        while let Ok(message) = control.try_recv() {
            match message {
                SamplerControl::Wake => {
                    next_process = Instant::now();
                }
                SamplerControl::Finish => finish_requested = true,
            }
        }
        if finish_requested {
            if let Some(process) = observe_process(tree) {
                latest
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .merge(SamplerUpdate { process: Some(process), disk_growth: None });
            }
            break;
        }
        let now = Instant::now();
        let mut update = SamplerUpdate::default();
        if now >= next_process {
            update.process = observe_process(tree);
            next_process = now + PROCESS_INTERVAL;
        }
        if now >= next_disk {
            let interval = match disk.step(DISK_ENTRIES_PER_SLICE) {
                DiskObservation::Pending => DISK_SLICE_INTERVAL,
                DiskObservation::Complete(total) => {
                    if let Some(baseline) = baseline_disk {
                        let growth = total.saturating_sub(baseline.bytes);
                        update.disk_growth = Some(if baseline.provenance
                            == DiskBaselineProvenance::BeforeTreeAttachment
                        {
                            SampleValue::new(Some(growth))
                        } else {
                            // A post-attach baseline proves only growth within the later window.
                            // Preserve that telemetry without treating it as proof that the
                            // launch-relative allowance was exceeded.
                            SampleValue::incomplete(Some(growth))
                        });
                    } else {
                        let provenance = if tree
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .is_none()
                        {
                            DiskBaselineProvenance::BeforeTreeAttachment
                        } else {
                            DiskBaselineProvenance::AfterTreeAttachment
                        };
                        baseline_disk = Some(DiskBaseline { bytes: total, provenance });
                        update.disk_growth = Some(if provenance
                            == DiskBaselineProvenance::BeforeTreeAttachment
                        {
                            SampleValue::new(Some(0))
                        } else {
                            SampleValue::incomplete(Some(0))
                        });
                    }
                    DISK_VERIFICATION_INTERVAL
                }
                DiskObservation::Unavailable => {
                    update.disk_growth = Some(SampleValue::unavailable());
                    DISK_RETRY_INTERVAL
                }
            };
            next_disk = now + interval;
        }
        if !update.is_empty() {
            latest
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .merge(update);
        }
        let until_process = next_process.saturating_duration_since(Instant::now());
        let wait = until_process.min(next_disk.saturating_duration_since(Instant::now()));
        match control.recv_timeout(wait) {
            Ok(SamplerControl::Wake) => next_process = Instant::now(),
            Ok(SamplerControl::Finish) => finish_requested = true,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn observe_process(
    tree: &Mutex<Option<ProcessTreeIdentity>>,
) -> Option<SamplerProcessSample> {
    let identity = *tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    identity.map(|identity| match platform::sample_resources(identity) {
        Ok(sample) => SamplerProcessSample {
            cpu_millis: SampleValue::new(sample.cpu_millis()),
            memory_bytes: SampleValue::new(sample.memory_bytes()),
            process_count: SampleValue::new(sample.process_count()),
            open_handles: SampleValue::new(sample.open_handles()),
        },
        Err(_) => SamplerProcessSample::unavailable(),
    })
}
