//! The read models behind the control API and the dashboard, and the edits the
//! control API makes to a thread.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The daemon's row of the status view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub started_at: f64,
    pub slack_status: String,
}

/// A thread as the control API and the dashboard show it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ThreadView {
    pub id: String,
    pub workspace: String,
    pub channel: String,
    pub root_ts: String,
    pub status: String,
    pub control: String,
    pub pause_reason: String,
    pub turns: i64,
    pub wait_streak: i64,
    pub no_progress: i64,
    pub last_reply_hash: String,
    pub reset_at: f64,
    pub summary: String,
    /// JSON text: the parent's decisions.
    pub decisions_json: String,
    /// JSON text: the thread's working context.
    pub context_json: String,
    pub debriefed_turn: i64,
    pub last_unsolicited: f64,
    pub created: f64,
    pub updated: f64,
    pub version: i64,
    /// JSON text: the owner's control detail.
    pub control_detail_json: String,
    pub throttled_until: f64,
    /// Who drives the thread's work: `parent` (its parent turns) or
    /// `external` (a driver on the control API).
    #[serde(default)]
    pub driver: String,
}

/// A message of a thread, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    pub event_id: String,
    pub ts: String,
    pub thread_ts: Option<String>,
    pub sender: String,
    pub text: String,
    pub source: String,
    /// JSON text: the metadata of a message Fridica posted, or `None`.
    pub meta_json: Option<String>,
}

/// A thread's latest notes revision.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NotesView {
    pub revision: i64,
    /// JSON text: the notes.
    pub data_json: String,
}

/// An instruction the owner sent a thread.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstructionView {
    pub id: i64,
    /// The client's own reference.
    #[serde(rename = "ref")]
    pub reference: String,
    pub created: f64,
    pub state: String,
    /// JSON text: the instruction.
    pub payload_json: String,
}

/// A worker as the control API and the dashboard show it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkerView {
    pub id: String,
    pub session_id: String,
    pub machine: String,
    pub workspace: String,
    pub backend: String,
    pub role: String,
    pub ephemeral: bool,
    pub backend_session_id: String,
    pub status: String,
    pub summary: String,
    /// JSON text: the worker's last result, or `None`.
    pub last_result_json: Option<String>,
    pub slot: i64,
    pub created: f64,
    pub updated: f64,
}

/// A job as the control API and the dashboard show it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JobView {
    pub id: String,
    pub worker_id: String,
    pub session_id: String,
    pub brief: String,
    pub join_group: String,
    pub inbox_id: Option<i64>,
    pub deliverable: String,
    pub fetch_repo: String,
    pub fetch_ref: String,
    pub status: String,
    pub attempt: i64,
    pub reported: bool,
    /// JSON text: the job's result, or `None`.
    pub result_json: Option<String>,
    pub error: String,
    pub queued_at: f64,
    pub started_at: f64,
    pub finished_at: f64,
    pub work_item_id: String,
    pub target_sha: String,
    pub target_tree: String,
    /// The role of the job's worker.
    #[serde(default)]
    pub role: String,
    /// The job's correlation labels ([`Job::tags`](crate::worker::Job::tags)).
    #[serde(default)]
    pub tags: Vec<String>,
}

/// An outbox post as the control API and the dashboard show it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PostView {
    pub id: i64,
    pub idem_key: String,
    pub session_id: String,
    pub kind: String,
    pub channel: String,
    pub thread_ts: Option<String>,
    pub text: String,
    /// JSON text: the post's metadata, or `None`.
    pub meta_json: Option<String>,
    pub filename: String,
    pub after: String,
    pub state: String,
    pub attempts: i64,
    pub retry_at: f64,
    pub sent_ts: String,
    pub error: String,
    pub created: f64,
    /// Whether the post carries a file (the file itself is never a view).
    pub has_file: bool,
    pub delivered_at: Option<f64>,
    pub trigger_event: String,
    pub trigger_class: String,
    /// JSON text: the obligations the post answers.
    pub answers_json: String,
}

/// An approval as the control API and the dashboard show it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ApprovalView {
    pub id: String,
    pub worker_id: String,
    pub job_id: String,
    pub session_id: String,
    pub backend_request_id: String,
    pub kind: String,
    pub summary: String,
    /// JSON text: what is being approved.
    pub detail_json: String,
    pub status: String,
    pub scope: String,
    pub decided_by: String,
    pub created: f64,
    pub decided_at: f64,
    pub expires_at: f64,
}

/// One entry of the audit log.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActivityView {
    pub id: i64,
    pub time: f64,
    pub actor: String,
    pub action: String,
    pub target: String,
    /// JSON text: the entry's details.
    pub details_json: String,
}

/// An obligation as the control API shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ObligationView {
    pub id: String,
    pub session_id: String,
    pub kind: String,
    pub summary: String,
    pub created: f64,
    pub due: f64,
    pub state: String,
    /// JSON text: how the obligation was disposed of.
    pub disposition_json: String,
    pub updated: f64,
}

/// How many queued or running jobs a machine has.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MachineLoad {
    pub machine: String,
    pub busy: i64,
}

/// Which machine a worker runs on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkerMachine {
    pub id: String,
    pub machine: String,
}

/// What the control API's status view counts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The daemon's `started_at` and `slack_status`, once it has started.
    pub runtime: Option<RuntimeStatus>,
    pub pending_approvals: i64,
    pub running_jobs: i64,
    pub queued_jobs: i64,
    /// Posts that failed, are ambiguous or are blocked.
    pub problem_posts: i64,
}

/// One message's Slack attachments, as listed for a thread.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageFiles {
    pub ts: String,
    pub sender: String,
    /// JSON text: the message's attachments.
    pub attachments: String,
}

/// The read models: threads, workers, jobs, approvals, posts and the activity
/// log as the control API and the dashboard show them. A filter list that is
/// empty matches everything. A record's `_json` fields hold JSON text, kept
/// byte for byte; views never carry binary data or private replay data.
pub trait Views {
    /// The status view's runtime row and counts.
    fn status(&mut self) -> Result<Status>;
    /// Threads whose control is in `controls`, most recently updated first.
    fn threads(&mut self, controls: &[String], limit: usize) -> Result<Vec<ThreadView>>;
    /// Threads the owner should look at: paused, or active and blocked.
    fn threads_needing_attention(&mut self) -> Result<Vec<ThreadView>>;
    /// One thread.
    fn thread(&mut self, id: &str) -> Result<Option<ThreadView>>;
    /// A thread's last `limit` messages, oldest first.
    fn thread_messages(&mut self, id: &str, limit: i64) -> Result<Vec<MessageView>>;
    /// A thread's latest notes revision.
    fn thread_notes(&mut self, id: &str) -> Result<Option<NotesView>>;
    /// The owner's last 20 instructions to a thread, newest first.
    fn owner_instructions(&mut self, id: &str) -> Result<Vec<InstructionView>>;
    /// A thread's workers, oldest first.
    fn thread_workers(&mut self, id: &str) -> Result<Vec<WorkerView>>;
    /// A thread's jobs, by worker and then in the order they were queued.
    fn thread_jobs(&mut self, id: &str) -> Result<Vec<JobView>>;
    /// A thread's posts, in order.
    fn thread_posts(&mut self, id: &str) -> Result<Vec<PostView>>;
    /// Workers whose status is in `statuses`, most recently updated first.
    fn workers(&mut self, statuses: &[String], limit: usize) -> Result<Vec<WorkerView>>;
    /// Running jobs, then queued ones, each oldest first.
    fn active_jobs(&mut self, limit: usize) -> Result<Vec<JobView>>;
    /// Every job: running and queued ones first, then newest first.
    fn all_jobs(&mut self, limit: usize) -> Result<Vec<JobView>>;
    /// Approvals whose status is in `statuses`, newest first.
    fn approvals(&mut self, statuses: &[String], limit: usize) -> Result<Vec<ApprovalView>>;
    /// Posts whose state is in `states`, newest first.
    fn posts(&mut self, states: &[String], limit: usize) -> Result<Vec<PostView>>;
    /// The audit log, newest first.
    fn activity(&mut self, limit: usize) -> Result<Vec<ActivityView>>;
    /// Obligations, newest first.
    fn obligations(&mut self, limit: usize) -> Result<Vec<ObligationView>>;
    /// How many queued or running jobs each machine has.
    fn busy_machines(&mut self) -> Result<Vec<MachineLoad>>;
    /// Every worker's machine.
    fn worker_machines(&mut self) -> Result<Vec<WorkerMachine>>;
    /// A thread's messages' attachments, in order, or `None` for an unknown
    /// thread.
    fn thread_files(&mut self, id: &str) -> Result<Option<Vec<MessageFiles>>>;
    /// The attachments (JSON text) of every message whose attachments mention
    /// the Slack file `file`, newest first.
    fn attachments_mentioning(&mut self, file: &str) -> Result<Vec<String>>;
    /// The most recently updated thread in a channel.
    fn latest_thread_in(&mut self, workspace: &str, channel: &str) -> Result<Option<String>>;
    /// Whether a thread exists.
    fn thread_exists(&mut self, id: &str) -> Result<bool>;
    /// Whether an approval exists.
    fn approval_exists(&mut self, id: &str) -> Result<bool>;
}

/// Notes the owner writes to a thread through the control API.
pub trait OwnerNotes {
    /// The thread's latest notes revision, or 0.
    fn notes_revision(&mut self, session: &str) -> Result<i64>;
    /// Save `data` (JSON text) as the thread's notes `revision` by `actor`,
    /// and audit the write.
    fn write_owner_notes(
        &mut self,
        session: &str,
        revision: i64,
        actor: &str,
        data: &str,
        now: f64,
    ) -> Result<()>;
}
