//! What Slack intake keeps beside the ledger: catch-up watermarks, the Socket
//! Mode status, attachment lookups, and the worker supervisor's few writes.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// One channel's catch-up watermark, as a catch-up pass commits it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watermark {
    pub workspace: String,
    pub channel: String,
    /// When the last complete (or given-up) pass ran.
    pub mark: f64,
    /// Whether the mark stays put because the pass was truncated.
    pub pinned: bool,
    /// Truncated passes in a row; 0 once the mark moves.
    pub truncated_passes: u32,
}

/// Durable catch-up state for each configured channel.
pub trait Catchup {
    /// The channel's watermark, if a pass has committed one.
    fn catchup_mark(&mut self, workspace: &str, channel: &str) -> Result<Option<f64>>;
    /// The newest message timestamp in the channel, among messages received
    /// before `before` (all of them when `None`).
    fn latest_message_ts(
        &mut self,
        workspace: &str,
        channel: &str,
        before: Option<f64>,
    ) -> Result<Option<f64>>;
    /// The roots of the channel's 200 most recently updated threads, keeping
    /// those updated at or after `since`.
    fn recent_thread_roots(
        &mut self,
        workspace: &str,
        channel: &str,
        since: f64,
    ) -> Result<Vec<String>>;
    /// The channel's count of truncated passes in a row, as stored (text).
    fn truncated_passes(&mut self, workspace: &str, channel: &str) -> Result<Option<String>>;
    /// Commit a pass's watermark and truncation count.
    fn keep_watermark(&mut self, mark: &Watermark) -> Result<()>;
}

/// The Socket Mode connection status.
pub trait SocketStatus {
    /// The last recorded status, if any.
    fn socket_status(&mut self) -> Result<Option<String>>;
    /// Record the status, for `meta` and the runtime row.
    fn keep_socket_status(&mut self, status: &str) -> Result<()>;
}

/// Lookups over messages' and posts' Slack files.
pub trait FileLookups {
    /// The IDs among `files` (ID, name) that Fridica uploaded itself to the
    /// thread `thread` in `channel`, in the order given.
    fn own_uploads(
        &mut self,
        channel: &str,
        thread: &str,
        files: &[(String, String)],
    ) -> Result<Vec<String>>;
    /// The attachments (JSON text) of every message in a thread.
    fn session_attachments(&mut self, session: &str) -> Result<Vec<String>>;
}

/// The worker supervisor's own writes.
pub trait Supervision {
    /// Audit the owner's interrupt of a worker and cancel its pending
    /// approvals.
    fn interrupt_worker(&mut self, worker: &str, now: f64) -> Result<()>;
    /// The fingerprint of the instructions the worker's backend session
    /// began with, if recorded.
    fn instructions_fingerprint(&mut self, worker: &str) -> Result<Option<String>>;
    /// Record the instructions a worker's next job runs with; with
    /// `fresh_session`, drop its backend session first.
    fn begin_instructions(
        &mut self,
        worker: &str,
        fingerprint: &str,
        fresh_session: bool,
    ) -> Result<()>;
}
