//!  Owns the bounded queue and the dedicated formula calculation thread.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::calc::engine::{EngineState, create_engine, unsupported_initial_results};

const QUEUE_CAPACITY: usize = 4;
use crate::calc::snapshot::{
    CalculationChange, CalculationResult, CalculationSnapshot, FormulaResultRoot,
    MarkdownCalculationSource,
};
use crate::document::{CalcStatus, CalcValue, CellAddress, TableId};
use crate::file_worker::JobToken;

pub(crate) enum Message {
    Initialize {
        token: JobToken,
        snapshot: Arc<CalculationSnapshot>,
    },
    Update {
        token: JobToken,
        changes: Vec<CalculationChange>,
        markdown_source: Option<MarkdownCalculationSource>,
    },
    Stop,
}

// A calculation worker that keeps the IronCalc model on its own thread.
pub struct CalculationWorker {
    pub(super) sender: SyncSender<Message>,
    pub(super) results: Receiver<CalculationResult>,
    pub(super) thread: Option<JoinHandle<()>>,
    pub(super) cancelled: Arc<Mutex<HashSet<JobToken>>>,
}

impl CalculationWorker {
    // Starts one calculation thread with a bounded queue.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (result_sender, results) = mpsc::sync_channel(QUEUE_CAPACITY * 2);
        let cancelled = Arc::new(Mutex::new(HashSet::new()));
        let worker_cancelled = Arc::clone(&cancelled);
        let thread = thread::Builder::new()
            .name("texcel-calculation-worker".to_owned())
            .spawn(move || worker_loop(receiver, result_sender, worker_cancelled))
            .ok();
        Self {
            sender,
            results,
            thread,
            cancelled,
        }
    }

    /// Retains a worker-built immutable snapshot without copying its sparse cell contents.
    pub fn initialize_shared_retaining(
        &self,
        token: JobToken,
        snapshot: Arc<CalculationSnapshot>,
    ) -> Result<(), (CalculationSubmitError, Arc<CalculationSnapshot>)> {
        match self
            .sender
            .try_send(Message::Initialize { token, snapshot })
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(Message::Initialize { snapshot, .. })) => {
                Err((CalculationSubmitError::QueueFull, snapshot))
            }
            Err(TrySendError::Disconnected(Message::Initialize { snapshot, .. })) => {
                Err((CalculationSubmitError::Stopped, snapshot))
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                unreachable!("initialize_retaining only submits initialization messages")
            }
        }
    }

    // Sends changed cells to the calculation model and reevaluates dependent formulas.
    pub fn update(
        &self,
        token: JobToken,
        changes: Vec<CalculationChange>,
    ) -> Result<(), CalculationSubmitError> {
        self.submit(Message::Update {
            token,
            changes,
            markdown_source: None,
        })
    }

    /// Projects a Markdown edit batch on the worker before applying it to the engine model.
    pub(crate) fn update_markdown(
        &self,
        token: JobToken,
        changes: Vec<CalculationChange>,
        markdown_source: MarkdownCalculationSource,
    ) -> Result<(), CalculationSubmitError> {
        self.submit(Message::Update {
            token,
            changes,
            markdown_source: Some(markdown_source),
        })
    }

    // Limits the number of result batches applied in one UI update.
    pub fn poll_limit(&self, limit: usize) -> Vec<CalculationResult> {
        let mut results = Vec::new();
        while results.len() < limit {
            match self.results.try_recv() {
                Ok(result) => results.push(result),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return results,
            }
        }
        results
    }

    /// Marks queued or running work so the worker can skip its result.
    pub fn cancel(&self, token: JobToken) {
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.insert(token);
        }
    }

    /// Clears a marker when a cancelled result was already waiting in the result queue.
    pub fn forget_cancellation(&self, token: JobToken) {
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.remove(&token);
        }
    }

    /// Queues a calculation job for the worker thread.
    fn submit(&self, message: Message) -> Result<(), CalculationSubmitError> {
        match self.sender.try_send(message) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(CalculationSubmitError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(CalculationSubmitError::Stopped),
        }
    }
}

impl Default for CalculationWorker {
    /// Starts a calculation worker with a bounded queue.
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CalculationWorker {
    /// Stops the worker thread when its owner is dropped.
    fn drop(&mut self) {
        let _ = self.sender.try_send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            drop(thread);
        }
    }
}

// Why a calculation request could not be queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalculationSubmitError {
    QueueFull,
    Stopped,
}

impl std::fmt::Display for CalculationSubmitError {
    /// Formats the calculation submission error.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = crate::i18n::calculation_submit_message(*self);
        formatter.write_str(&message.resolve(crate::i18n::Language::English))
    }
}

/// Processes calculation jobs on the dedicated thread.
pub(super) fn worker_loop(
    receiver: Receiver<Message>,
    results: SyncSender<CalculationResult>,
    cancelled: Arc<Mutex<HashSet<JobToken>>>,
) {
    let mut documents = HashMap::<u64, EngineState>::new();
    while let Ok(message) = receiver.recv() {
        match message {
            Message::Initialize { token, snapshot } => {
                if take_cancelled(&cancelled, token) {
                    continue;
                }
                let (state, error) = match create_engine(Arc::clone(&snapshot)) {
                    Ok(mut state) => {
                        let error = state.refresh_formula_masking().err();
                        state.evaluate();
                        (Some(state), error)
                    }
                    Err(error) => (None, Some(error)),
                };
                let entries = state.as_ref().map_or_else(
                    || unsupported_initial_results(&snapshot),
                    EngineState::results,
                );
                if take_cancelled(&cancelled, token) {
                    continue;
                }
                if let Some(state) = state {
                    documents.insert(token.document_id, state);
                }
                if !send_results(&results, token, entries, error, Vec::new()) {
                    break;
                }
            }
            Message::Update {
                token,
                changes,
                markdown_source,
            } => {
                if take_cancelled(&cancelled, token) {
                    continue;
                }
                let changes = match markdown_source
                    .as_ref()
                    .map(|source| crate::calc::project_markdown_changes(source, &changes))
                {
                    Some(Ok(changes)) => changes,
                    Some(Err(error)) => {
                        let failed_changes = changes
                            .iter()
                            .map(|change| (change.table_id, change.address))
                            .collect();
                        if !send_results(&results, token, Vec::new(), Some(error), failed_changes) {
                            break;
                        }
                        continue;
                    }
                    None => changes,
                };
                let Some(state) = documents.get_mut(&token.document_id) else {
                    let _ = results.send(CalculationResult {
                        token,
                        full_snapshot: None,
                        failed_changes: Vec::new(),
                        error: Some(crate::i18n::Message::new(
                            "status-calculation-model-unavailable",
                        )),
                        done: true,
                    });
                    continue;
                };
                let error = state
                    .apply_changes(&changes)
                    .and_then(|()| state.refresh_formula_masking())
                    .err();
                if error.is_none() {
                    state.evaluate();
                }
                // Never label a partially applied update as results for this revision.
                let entries = if error.is_none() {
                    state.results()
                } else {
                    Vec::new()
                };
                let failed_changes = if error.is_some() {
                    changes
                        .iter()
                        .map(|change| (change.table_id, change.address))
                        .collect()
                } else {
                    Vec::new()
                };
                if take_cancelled(&cancelled, token) {
                    continue;
                }
                if !send_results(&results, token, entries, error, failed_changes) {
                    break;
                }
            }
            Message::Stop => break,
        }
    }
}

/// Removes a cancellation marker when a queued job reaches a safe boundary.
fn take_cancelled(cancelled: &Mutex<HashSet<JobToken>>, token: JobToken) -> bool {
    cancelled
        .lock()
        .is_ok_and(|mut cancelled| cancelled.remove(&token))
}

/// Returns calculation results without blocking the UI thread.
fn send_results(
    sender: &SyncSender<CalculationResult>,
    token: JobToken,
    entries: Vec<(TableId, CellAddress, CalcStatus, Option<CalcValue>)>,
    error: Option<crate::i18n::Message>,
    failed_changes: Vec<(TableId, CellAddress)>,
) -> bool {
    let full_snapshot = error
        .is_none()
        .then(|| Arc::new(FormulaResultRoot::new(token.revision, entries)));
    sender
        .send(CalculationResult {
            token,
            full_snapshot,
            failed_changes,
            error,
            done: true,
        })
        .is_ok()
}

#[cfg(test)]
impl CalculationWorker {
    // Moves the loaded document state into the calculation model.
    pub fn initialize(
        &self,
        token: JobToken,
        snapshot: CalculationSnapshot,
    ) -> Result<(), CalculationSubmitError> {
        self.submit(Message::Initialize {
            token,
            snapshot: Arc::new(snapshot),
        })
    }
    /// Keeps ownership of a document-sized initial snapshot when the bounded queue is full.
    pub fn initialize_retaining(
        &self,
        token: JobToken,
        snapshot: CalculationSnapshot,
    ) -> Result<(), (CalculationSubmitError, CalculationSnapshot)> {
        self.initialize_shared_retaining(token, Arc::new(snapshot))
            .map_err(|(error, snapshot)| {
                let snapshot = Arc::try_unwrap(snapshot).unwrap_or_else(|shared| (*shared).clone());
                (error, snapshot)
            })
    }
    // Collects all calculation results without making the UI thread wait.
    pub fn poll(&self) -> Vec<CalculationResult> {
        self.poll_limit(usize::MAX)
    }
}
