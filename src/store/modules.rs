//! The outbox, jobs and workers, approvals, scoped fetches, parent worker
//! controls, diagnostics, the channel ledger's links, the weekly archives and
//! the configuration-edit journal.
use crate::{
    config::{registry::Registry, Limits},
    delivery::{ClaimedPost, DeliveryOutcome, Post},
    fork::ContextBundle,
    parent::{ParentRequest, WorkerControl, WorkerOperation},
    worker::{
        Approval, ApprovalDecision, ApprovalRequest, CollectedArtifact, Job, Outcome,
        WorkerFailure, WorkerRecord,
    },
    Authority,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

/// What recording a delivery attempt's outcome did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostOutcome {
    /// The post was sent.
    Sent,
    /// The post was not sent: it waits for a retry, or failed.
    Unsent,
    /// The attempt was no longer current, or its post changed. The late
    /// result is recorded; the caller reports the attempt as stale.
    Stale,
}

/// The ordered, durable outbox. Delivery attempts are fenced by their attempt
/// count, so a late result of an older attempt cannot settle a retry.
pub trait Outbox {
    /// Queue a post and return its ID. A post whose idempotency key is
    /// already queued returns that post's ID, and must have the same content.
    fn queue_post(&mut self, post: &Post, now: f64) -> Result<i64>;
    /// Up to `limit` (at most 100) posts ready to send now, in order.
    fn ready_posts(&mut self, now: f64, limit: usize) -> Result<Vec<i64>>;
    /// Claim the next ready post (or post `id`, when it is ready) for a
    /// delivery attempt and record the call.
    fn claim_post(&mut self, now: f64, id: Option<i64>) -> Result<Option<ClaimedPost>>;
    /// Record the outcome of a claimed delivery attempt as `owner`.
    fn finish_delivery(
        &mut self,
        claim: &ClaimedPost,
        outcome: &DeliveryOutcome,
        owner: &str,
        now: f64,
    ) -> Result<PostOutcome>;
    /// Mark a post being sent as sent with its Slack `reference`, with the
    /// reply reservations and obligations it settles.
    fn confirm_post(&mut self, id: i64, reference: &str, now: f64) -> Result<()>;
    /// At startup: posts left being sent become ambiguous. Returns how many.
    fn recover_posts(&mut self, now: f64) -> Result<usize>;
    /// The owner retries a failed or ambiguous post, and the posts waiting
    /// on it; returns whether it was retryable.
    fn retry_post(&mut self, id: i64, actor: Authority, now: f64) -> Result<bool>;
}

/// The jobs queued, running and with their worker, read in one snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkSnapshot {
    pub queued: Vec<Job>,
    pub running: Vec<Job>,
    pub workers: Vec<WorkerRecord>,
}

/// The snapshot a worker's previous job ran with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreviousSnapshot {
    /// The previous job's ID.
    pub job: String,
    pub bundle: ContextBundle,
}

/// A job admitted to run, and its worker.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimedJob {
    pub job: Job,
    pub worker: WorkerRecord,
}

/// How a job attempt ended.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Completion {
    pub outcome: std::result::Result<Outcome, WorkerFailure>,
    pub artifacts: Vec<CollectedArtifact>,
    pub interrupted: bool,
    pub stopped: bool,
    pub allow_retry: bool,
}

/// What recording a job attempt's completion did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Completed {
    Finished,
    /// The job is queued again, once, in the same backend session.
    Retried,
    /// The attempt was no longer current.
    Stale,
}

/// Durable job admission and completion, and the workers jobs run on.
pub trait Jobs {
    /// The job with this ID. An unknown job is an error.
    fn job_record(&mut self, id: &str) -> Result<Job>;
    /// The worker with this ID. An unknown worker is an error.
    fn worker_record(&mut self, id: &str) -> Result<WorkerRecord>;
    /// Add workers, in order.
    fn add_workers(&mut self, workers: &[WorkerRecord], now: f64) -> Result<()>;
    /// Queue jobs, in order, each on a worker of its thread that is not stopped.
    fn queue_jobs(&mut self, jobs: &[Job], now: f64) -> Result<()>;
    /// The thread's work as the parent sees it: its workers, machine load,
    /// active jobs, recent worker controls, results, other threads' jobs in
    /// the channel and the latest progress of running jobs.
    fn work_context(&mut self, session: &str) -> Result<Value>;
    /// The snapshot a worker's previous job (other than `job`) ran with.
    fn previous_snapshot(&mut self, worker: &str, job: &str) -> Result<Option<PreviousSnapshot>>;
    /// Up to 20 files attached in the thread by others, newest first, that a
    /// delegation may hand to a worker.
    fn delegable_files(&mut self, session: &str) -> Result<Vec<Value>>;
    /// The queued and running jobs and every worker.
    fn work_snapshot(&mut self) -> Result<WorkSnapshot>;
    /// Admit queued job `id` to run in `slot`, rechecking every durable limit
    /// against `machines` and `limits`. A job that can no longer run is
    /// cancelled; one that must wait returns `None` too.
    fn claim_job(
        &mut self,
        id: &str,
        slot: usize,
        machines: &Registry,
        limits: &Limits,
        now: f64,
    ) -> Result<Option<ClaimedJob>>;
    /// Record a progress note of a running job attempt and queue it for the
    /// job's thread; returns whether the attempt was running.
    fn record_job_progress(
        &mut self,
        job: &str,
        attempt: u32,
        text: &str,
        now: f64,
    ) -> Result<bool>;
    /// Record how a job attempt ended.
    fn complete_job(
        &mut self,
        id: &str,
        attempt: u32,
        completion: &Completion,
        now: f64,
    ) -> Result<Completed>;
    /// Stop a worker for `actor`: cancel its queued jobs and pending approvals.
    fn stop_worker(&mut self, worker: &str, actor: &str, now: f64) -> Result<()>;
    /// At startup: running jobs are interrupted and pending approvals
    /// cancelled. Returns how many jobs were interrupted.
    fn recover_jobs(&mut self, now: f64) -> Result<usize>;
    /// Queued and running jobs per machine.
    fn busy_by_machine(&mut self) -> Result<BTreeMap<String, i64>>;
}

/// A new approval request from a worker's running job.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewApproval {
    pub id: String,
    pub worker: WorkerRecord,
    pub job: Job,
    pub request: ApprovalRequest,
    /// The policy's decision, when it decides without the owner.
    pub automatic: Option<ApprovalDecision>,
    pub now: f64,
    pub expires_at: f64,
}

/// How an approval request starts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ApprovalStart {
    /// It waits for the owner.
    Pending,
    /// It is decided now (by policy, or denied for a stale job).
    Immediate(ApprovalDecision),
}

/// How an approval request is settled.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ApprovalSettlement {
    Decide(ApprovalDecision),
    Expire,
    Cancel,
}

/// Approval requests and their audit.
pub trait Approvals {
    /// The approval request with this ID.
    fn approval(&mut self, id: &str) -> Result<Option<Approval>>;
    /// Up to `limit` (at most 1000) pending requests, newest first.
    fn pending_approvals(&mut self, limit: usize) -> Result<Vec<Approval>>;
    /// Start an approval request.
    fn begin_approval(&mut self, request: &NewApproval) -> Result<ApprovalStart>;
    /// Settle a request as `actor`. A late decision expires or cancels it;
    /// returns false for a decision not accepted or a request not pending.
    fn settle_approval(
        &mut self,
        id: &str,
        settlement: ApprovalSettlement,
        actor: &str,
        now: f64,
    ) -> Result<bool>;
    /// Cancel a worker's pending requests (it was interrupted).
    fn cancel_worker_approvals(&mut self, worker: &str, now: f64) -> Result<()>;
    /// Cancel every pending request (the configuration changed).
    fn cancel_pending_approvals(&mut self, now: f64) -> Result<()>;
}

/// Scoped repository fetches, fenced by the job attempt they serve.
pub trait Fetches {
    /// Record the start of a fetch for `job`; returns its sequence number, or
    /// `None` when the job attempt is no longer active.
    fn begin_fetch(&mut self, job: &Job, request: &Value, now: f64) -> Result<Option<i64>>;
    /// Record a fetch's `result`; returns whether the job attempt is still
    /// active to use it. A stale or repeated completion is an error.
    fn finish_fetch(&mut self, job: &Job, seq: i64, result: &Value, now: f64) -> Result<bool>;
}

/// A parent's control of one worker: stop it, or interrupt its running
/// attempt. The intent is kept as recorded.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerControlIntent {
    pub session: String,
    pub inbox: i64,
    pub worker: String,
    pub op: WorkerOperation,
    pub job: Option<String>,
    pub attempt: Option<u32>,
}

/// A worker control not yet carried out.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingWorkerControl {
    pub seq: i64,
    pub intent: WorkerControlIntent,
}

/// Worker controls the parent decides: durable intents in the replay ledger,
/// committed with the parent's turn and carried out after.
pub trait WorkerControls {
    /// The thread's queued and running jobs, as the parent's snapshot names them.
    fn controlled_jobs(&mut self, session: &str) -> Result<Vec<Value>>;
    /// Whether a worker control of the thread is pending.
    fn worker_control_pending(&mut self, session: &str) -> Result<bool>;
    /// The thread's last 20 worker controls and their outcomes, newest first.
    fn recent_worker_controls(&mut self, session: &str) -> Result<Vec<Value>>;
    /// Whether an interrupt of this job attempt is pending.
    fn interrupt_pending(&mut self, job: &str, attempt: u32) -> Result<bool>;
    /// Whether the jobs `controls` target are still those `request` saw.
    fn worker_controls_current(
        &mut self,
        request: &ParentRequest,
        controls: &[WorkerControl],
    ) -> Result<bool>;
    /// Record `controls` and their immediate effects.
    fn queue_worker_controls(
        &mut self,
        request: &ParentRequest,
        controls: &[WorkerControl],
        now: f64,
    ) -> Result<()>;
    /// Up to 128 pending controls, oldest first.
    fn pending_worker_controls(&mut self) -> Result<Vec<PendingWorkerControl>>;
    /// Record that a control was carried out with `outcome`.
    fn complete_worker_control(&mut self, seq: i64, outcome: &str, now: f64) -> Result<()>;
}

/// Diagnostic metadata.
pub trait Diagnostics {
    /// The Slack scopes recorded at the last daemon start, if any.
    fn slack_scopes(&mut self) -> Result<Option<String>>;
}

/// The channel ledger's links: which threads of a channel refer to which
/// pull request or issue, and to which other threads.
pub trait Links {
    /// Record what a stored message's `text` refers to, for its thread.
    fn record_links(
        &mut self,
        workspace: &str,
        channel: &str,
        session: &str,
        text: &str,
        now: f64,
    ) -> Result<()>;
    /// Once: link the messages of the last `window` seconds. Returns how
    /// many messages were read.
    fn backfill_links(&mut self, now: f64, window: f64) -> Result<usize>;
}

/// A thread found in the weekly archives.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchiveHit {
    pub thread: String,
    pub week: String,
    pub last_activity: f64,
    pub summary: String,
    pub matches: Vec<String>,
}

/// The weekly archives that quiet threads move to.
pub trait Archive {
    /// Bring an archived thread back; returns whether it was archived.
    fn revive_thread(&mut self, session: &str, now: f64) -> Result<bool>;
    /// Bring an archived thread back for intake. A failed revival is undone
    /// and noted as a health event, never an error.
    fn revive_or_note(&mut self, session: &str, now: f64) -> Result<()>;
    /// Up to `limit` archived threads whose messages or summary contain
    /// `query`, newest week first.
    fn search_archives(&mut self, query: &str, limit: usize) -> Result<Vec<ArchiveHit>>;
}

/// An owner's configuration edit: the file and its fingerprints before and
/// after.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationIntent {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

/// The configuration edit awaiting reconciliation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingConfigurationEdit {
    pub seq: i64,
    pub intent: ConfigurationIntent,
}

/// The configuration-edit journal. Recording an edit is not a unit-of-work
/// step: it replaces the file after its transaction commits.
pub trait ConfigurationIntents {
    /// The pending edit, if any. More than one is an error.
    fn pending_configuration_edit(&mut self) -> Result<Option<PendingConfigurationEdit>>;
    /// Settle the pending edit `seq`: `applied`, or not.
    fn complete_configuration_edit(&mut self, seq: i64, applied: bool, now: f64) -> Result<()>;
}
