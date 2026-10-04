//! The read models behind the control API and the dashboard, and the edits the
//! control API makes to a thread.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// One stored value of a view row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cell {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

/// One row of a read model: its columns, named and in the order the view
/// selects them. A column whose name ends in `_json` holds JSON text, kept
/// byte for byte. Views never carry binary data or private replay data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Row(pub Vec<(String, Cell)>);

/// What the control API's status view counts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The daemon's `started_at` and `slack_status`, once it has started.
    pub runtime: Option<Row>,
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
/// empty matches everything.
pub trait Views {
    /// The status view's runtime row and counts.
    fn status(&mut self) -> Result<Status>;
    /// Threads whose control is in `controls`, most recently updated first.
    fn threads(&mut self, controls: &[String], limit: usize) -> Result<Vec<Row>>;
    /// Threads the owner should look at: paused, or active and blocked.
    fn threads_needing_attention(&mut self) -> Result<Vec<Row>>;
    /// One thread.
    fn thread(&mut self, id: &str) -> Result<Option<Row>>;
    /// A thread's last `limit` messages, oldest first.
    fn thread_messages(&mut self, id: &str, limit: i64) -> Result<Vec<Row>>;
    /// A thread's latest notes revision.
    fn thread_notes(&mut self, id: &str) -> Result<Option<Row>>;
    /// The owner's last 20 instructions to a thread, newest first.
    fn owner_instructions(&mut self, id: &str) -> Result<Vec<Row>>;
    /// A thread's workers, oldest first.
    fn thread_workers(&mut self, id: &str) -> Result<Vec<Row>>;
    /// A thread's jobs, by worker and then in the order they were queued.
    fn thread_jobs(&mut self, id: &str) -> Result<Vec<Row>>;
    /// A thread's posts, in order.
    fn thread_posts(&mut self, id: &str) -> Result<Vec<Row>>;
    /// Workers whose status is in `statuses`, most recently updated first.
    fn workers(&mut self, statuses: &[String], limit: usize) -> Result<Vec<Row>>;
    /// Running jobs, then queued ones, each oldest first.
    fn active_jobs(&mut self, limit: usize) -> Result<Vec<Row>>;
    /// Every job: running and queued ones first, then newest first.
    fn all_jobs(&mut self, limit: usize) -> Result<Vec<Row>>;
    /// Approvals whose status is in `statuses`, newest first.
    fn approvals(&mut self, statuses: &[String], limit: usize) -> Result<Vec<Row>>;
    /// Posts whose state is in `states`, newest first.
    fn posts(&mut self, states: &[String], limit: usize) -> Result<Vec<Row>>;
    /// The audit log, newest first.
    fn activity(&mut self, limit: usize) -> Result<Vec<Row>>;
    /// Obligations, newest first.
    fn obligations(&mut self, limit: usize) -> Result<Vec<Row>>;
    /// How many queued or running jobs each machine has (`machine`, `busy`).
    fn busy_machines(&mut self) -> Result<Vec<Row>>;
    /// Every worker's machine (`id`, `machine`).
    fn worker_machines(&mut self) -> Result<Vec<Row>>;
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
