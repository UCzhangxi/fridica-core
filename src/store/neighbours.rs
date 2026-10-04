//! What a thread's actor keeps beside its turns: the daemon's runtime row,
//! thread memory, worker-result snapshots, reply evidence, debriefs, progress
//! notes and the scheduling sweep.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The daemon's runtime row as a start writes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStart {
    pub pid: u32,
    pub started_at: f64,
    pub observe_only: bool,
    pub config_fingerprint: String,
}

/// The daemon's runtime row: whether it runs, its heartbeat and its control
/// endpoint.
pub trait Runtime {
    /// The Slack status the previous run left, if any.
    fn previous_slack_status(&mut self) -> Result<Option<String>>;
    /// Start a run: the row is `starting`, its heartbeat is the start and no
    /// control endpoint is advertised yet.
    fn start_runtime(&mut self, start: &RuntimeStart) -> Result<()>;
    /// Advertise the control endpoint (empty for none).
    fn advertise_control(&mut self, endpoint: &str) -> Result<()>;
    /// Record a heartbeat.
    fn heartbeat(&mut self, now: f64) -> Result<()>;
    /// Stop the run: the row is `stopped` with a last heartbeat at `now`.
    fn stop_runtime(&mut self, now: f64) -> Result<()>;
}

/// A thread's memory as the parent edits it: decisions and task notes.
pub trait ThreadMemory {
    /// The thread's latest notes: their revision and data (JSON text).
    fn latest_notes(&mut self, session: &str) -> Result<Option<(i64, String)>>;
    /// The thread's decisions (JSON text). An unknown thread is an error.
    fn thread_decisions(&mut self, session: &str) -> Result<String>;
    /// Replace the thread's decisions (JSON text).
    fn keep_decisions(&mut self, session: &str, decisions: &str) -> Result<()>;
    /// Whether the thread has an outbox post with this idempotency key.
    fn post_queued(&mut self, session: &str, idem_key: &str) -> Result<bool>;
    /// Save `data` (JSON text) as the parent's notes `revision`, written on
    /// inbox item `inbox`, and audit the write.
    fn write_parent_notes(
        &mut self,
        session: &str,
        revision: i64,
        data: &str,
        inbox: i64,
        now: f64,
    ) -> Result<()>;
}

/// An item of a thread's inbox.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InboxEntry {
    pub kind: String,
    /// What it refers to (a message's event ID, a job's ID, ...).
    pub reference: String,
    /// JSON text.
    pub payload: String,
}

/// Worker results as the parent sees them, and the files they deliver.
pub trait ResultSnapshots {
    /// The join group of a thread's job, if the job exists.
    fn job_group(&mut self, job: &str, session: &str) -> Result<Option<String>>;
    /// The jobs of `group` in the thread (or `job` alone when `group` is
    /// empty), each with its worker as one JSON object (text), in the order
    /// they were queued.
    fn group_results(&mut self, session: &str, group: &str, job: &str) -> Result<Vec<String>>;
    /// One of the thread's inbox items.
    fn inbox_entry(&mut self, inbox: i64, session: &str) -> Result<Option<InboxEntry>>;
    /// A message's event ID and whether it came from a peer agent.
    fn message_origin(&mut self, event: &str) -> Result<Option<(String, bool)>>;
    /// The inbox item that queued a thread's job, if any.
    fn job_inbox(&mut self, job: &str, session: &str) -> Result<Option<i64>>;
    /// The ready files (path, bytes) of the thread's file-deliverable `jobs`,
    /// job by job in the order given, each job's in the order kept.
    fn deliverable_files(
        &mut self,
        session: &str,
        jobs: &[Option<String>],
    ) -> Result<Vec<(String, Vec<u8>)>>;
    /// Record that outbox post `post` answers `obligations`.
    fn link_answers(&mut self, post: i64, obligations: &[String]) -> Result<()>;
}

/// The thread's latest reply, as repeat suppression sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LastReply {
    /// Its outbox post.
    pub id: i64,
    /// The post's state.
    pub state: String,
    /// Who the reply answered, or empty.
    pub requester: String,
}

/// What reply rendering and repeat suppression read.
pub trait ReplyEvidence {
    /// The thread's latest reply post, if any.
    fn last_reply(&mut self, session: &str) -> Result<Option<LastReply>>;
    /// The thread's last 100 messages (sender, text), newest first.
    fn latest_messages(&mut self, session: &str) -> Result<Vec<(String, String)>>;
    /// The sender of a message in the thread.
    fn message_sender(&mut self, event: &str, session: &str) -> Result<Option<String>>;
}

/// Where a debrief starts: the thread as its finished discussion left it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DebriefOrigin {
    pub version: i64,
    pub turn: i64,
    /// Who the reply answered (`owner`, `peer`, `human`).
    pub class: String,
}

/// A debrief queued as an outbox post.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DebriefPost {
    pub session: String,
    pub inbox: i64,
    pub post: i64,
    pub class: String,
    pub turn: i64,
    pub now: f64,
}

/// A debrief call of the parent, as recorded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DebriefTurn {
    pub session: String,
    pub inbox: i64,
    /// JSON text.
    pub action: String,
    /// JSON text.
    pub response: String,
    /// JSON text: the request.
    pub context: String,
    pub created: f64,
}

/// Debriefs: a separate post once a discussion is finished.
pub trait Debriefs {
    /// The complete thread's version, turns and reply class, if its reply
    /// `after` is queued.
    fn debrief_origin(&mut self, session: &str, after: &str) -> Result<Option<DebriefOrigin>>;
    /// Queue a debrief of inbox item `inbox` (`payload` is JSON text).
    fn queue_debrief(&mut self, session: &str, inbox: i64, payload: &str, now: f64) -> Result<()>;
    /// Whether the debrief on inbox item `inbox` is still due: the thread is
    /// active at `version`, not debriefed at `turn`, and the item is being
    /// processed. An unknown thread is an error.
    fn debrief_due(
        &mut self,
        session: &str,
        version: Option<i64>,
        turn: i64,
        inbox: i64,
    ) -> Result<bool>;
    /// A stale debrief: the item is pending again and its unposted
    /// reservation released.
    fn debrief_stale(&mut self, inbox: i64) -> Result<()>;
    /// A debrief posted: the post takes the reply class and the reservation,
    /// and the thread is debriefed at `turn`.
    fn debrief_posted(&mut self, posted: &DebriefPost) -> Result<()>;
    /// No debrief: release the unposted reservation and audit it.
    fn debrief_unavailable(&mut self, session: &str, inbox: i64, now: f64) -> Result<()>;
    /// Record the parent's debrief call.
    fn keep_debrief_turn(&mut self, turn: &DebriefTurn) -> Result<()>;
}

/// Whether a progress note is still news.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgressState {
    /// Whether the inbox item is being processed.
    pub processing: bool,
    /// Whether the thread is active.
    pub active: bool,
}

/// A running job's progress note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgressNote {
    pub text: String,
    pub worker: String,
}

/// Scheduling threads' turns and their simplest inbox items.
pub trait ThreadTurns {
    /// Up to `limit` threads with a pending inbox item due by `now`, oldest
    /// item first.
    fn ready_threads(&mut self, now: f64, limit: usize) -> Result<Vec<String>>;
    /// Mark an inbox item done.
    fn inbox_done(&mut self, inbox: i64) -> Result<()>;
    /// The progress item's state in the thread. An unknown thread is an
    /// error.
    fn progress_state(&mut self, inbox: i64, session: &str) -> Result<ProgressState>;
    /// Note `seq` of the job's `attempt`, if the job still runs it.
    fn running_progress_note(
        &mut self,
        job: &str,
        attempt: i64,
        seq: i64,
        session: &str,
    ) -> Result<Option<ProgressNote>>;
}
