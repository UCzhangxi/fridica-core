//! What the terminal log and the event feed read beside the ledger: recorded
//! Slack names, ledger lookups, the GitHub read pause, and point reads in
//! tables whose areas have not moved yet.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The Slack names recorded for owner-facing output. Each is `None` until
/// recorded.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordedNames {
    /// The workspace's name.
    pub workspace: Option<String>,
    /// JSON text: configured channel IDs to names.
    pub channels: Option<String>,
    /// JSON text: member IDs to names.
    pub users: Option<String>,
}

/// Recorded Slack names. They are for display only and never grant scope.
pub trait SlackNames {
    /// The recorded workspace, channel and member names.
    fn slack_names(&mut self) -> Result<RecordedNames>;
    /// The recorded member names (JSON text), if any.
    fn user_names(&mut self) -> Result<Option<String>>;
    /// Replace the recorded member names with `users` (JSON text).
    fn keep_user_names(&mut self, users: &str) -> Result<()>;
    /// Record the bot's identity as Slack reported it at connect.
    fn keep_identity(&mut self, identity: &SlackIdentity) -> Result<()>;
}

/// The bot's identity as Slack reports it when Socket Mode connects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlackIdentity {
    /// The granted scopes, comma-separated, or `unknown`.
    pub scopes: String,
    /// JSON text: configured channel IDs to names.
    pub channels: String,
    /// The workspace's name.
    pub workspace: String,
}

/// Lookups in the replay ledger beyond reading it in order.
pub trait LedgerLookups {
    /// The senders of the `intake` records among the 500 after `seq`, in
    /// order; records without a readable sender are skipped.
    fn intake_senders_after(&mut self, seq: i64) -> Result<Vec<String>>;
    /// Whether an `intake` of Slack event `event_id` was recorded at exactly
    /// `time` among the 1000 records before `seq`: a repeated delivery.
    fn has_recent_intake(&mut self, event_id: &str, seq: i64, time: f64) -> Result<bool>;
    /// The context (JSON text) of the latest complete
    /// `parent_attachment_result` recorded for `key`.
    fn attachment_context(&mut self, key: &str) -> Result<Option<String>>;
}

/// An outbox post as the event feed names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboxPost {
    /// What the post is (a reply, a notice, ...).
    pub kind: String,
    /// The thread it goes to.
    pub session: String,
}

/// Single reads the event feed makes in the outbox, jobs and messages. Each
/// moves to its own area's trait when that area moves behind the contract.
pub trait FeedLookups {
    /// The outbox post with this ID.
    fn outbox_post(&mut self, id: i64) -> Result<Option<OutboxPost>>;
    /// The thread a job belongs to.
    fn job_session(&mut self, id: &str) -> Result<Option<String>>;
    /// When the message with this Slack event ID first arrived.
    fn message_received_at(&mut self, event_id: &str) -> Result<Option<f64>>;
}

/// The pause GitHub reads take after a rate limit.
pub trait GithubPause {
    /// The recorded end of the pause, as stored (a number in text), if any.
    fn github_paused_until(&mut self) -> Result<Option<String>>;
    /// Pause GitHub reads until `until`, or keep a later recorded end.
    fn pause_github(&mut self, until: f64) -> Result<()>;
}
