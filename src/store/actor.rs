//! The thread actor: loading a turn, fencing it against what changed while the
//! parent decided, and settling or committing it with its effects.
//!
//! A turn works on one claimed (`processing`) inbox item of a thread. Its
//! effects commit only while the thread's version still matches the one it
//! loaded; otherwise the item goes back to `pending` and nothing else changes.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// What a turn reads about its thread and inbox item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnInput {
    /// JSON text: the thread's state.
    pub thread: String,
    /// The item's kind (`message`, `obligation_due`, ...).
    pub kind: String,
    /// The item's reference: an event ID, obligation ID, ...
    pub reference: String,
    /// JSON text: the item's payload.
    pub payload: String,
    /// JSON text: the message a `message` item refers to.
    pub message: Option<String>,
    /// For an `obligation_due` item: whether the obligation came from a
    /// generated (peer) message.
    pub from_peer: Option<bool>,
    /// JSON text each: the thread's last 60 messages, newest first.
    pub history: Vec<String>,
    /// JSON text each: the thread's open, deferred or undelivered
    /// obligations with their deliveries, oldest first.
    pub obligations: Vec<String>,
    /// Whether the thread's last decide or repair call failed in a way the
    /// owner must review.
    pub review_required: bool,
}

/// A turn that ends without a decision: it observes, or waits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settlement {
    pub id: i64,
    pub session: String,
    /// The thread version the turn loaded.
    pub version: i64,
    /// The message whose verdict is recorded, if any.
    pub event: Option<String>,
    pub verdict: String,
    /// Keep the item pending until then instead of finishing it.
    pub until: Option<f64>,
}

/// A turn that failed: what happens to its item, and the signal and audit
/// it leaves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnFailure {
    pub id: i64,
    pub session: String,
    /// The item's new state: `pending` or `dropped`.
    pub state: String,
    pub not_before: f64,
    /// The signal obligation's ID and dedup key.
    pub signal: String,
    /// JSON text: the signal's source.
    pub source: String,
    /// JSON text: the audit details.
    pub details: String,
    pub now: f64,
}

/// One parent call a turn made, as `parent_turns` keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParentTurn {
    /// The call (`decide`, `repair`, `triage`).
    pub call: Option<String>,
    /// JSON text.
    pub response: String,
    /// JSON text.
    pub context: String,
    /// Why the turn could not use the call, or empty.
    pub error: String,
    pub created: Option<f64>,
    /// JSON text: the audit details when the call blocked the thread.
    pub blocked: Option<String>,
}

/// A turn retried later because the parent hit its usage limit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnRetry {
    pub id: i64,
    pub session: String,
    /// The thread version the turn loaded.
    pub version: Option<i64>,
    /// The calls kept; their action is `{}`.
    pub calls: Vec<ParentTurn>,
    pub retry_at: f64,
    /// JSON text: the audit details.
    pub details: String,
    pub now: f64,
}

/// A message triage decided not to answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriageSettlement {
    pub id: i64,
    pub session: String,
    /// The thread version the turn loaded.
    pub version: Option<i64>,
    /// The triage calls; only their response, context and time are kept.
    pub calls: Vec<ParentTurn>,
    pub event: Option<String>,
    pub verdict: String,
    pub now: f64,
}

/// The thread as a turn's commit finds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fence {
    pub version: i64,
    /// Whether the thread is active and the item still being processed.
    pub active: bool,
}

/// Messages that arrived while the parent decided.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arrival {
    pub arrived: bool,
    /// Whether the item was already rerun once for an arrival.
    pub reread: bool,
}

/// The thread's state after a committed turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnClose {
    pub session: String,
    pub status: String,
    /// The reply's outbox key: the turn counts once its reply is queued.
    pub reply_key: String,
    pub turn: i64,
    pub waiting: i64,
    pub quiet: i64,
    pub hash: String,
    /// The new summary, or empty to keep it.
    pub summary: String,
    pub now: f64,
}

/// A hand-off queued in a linked thread's inbox.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueuedHandoff {
    /// The target thread.
    pub target: String,
    /// The thread handing off.
    pub from: String,
    /// JSON text.
    pub payload: String,
    pub dedup_key: String,
    /// JSON text: the audit details if it was queued.
    pub queued: String,
    /// JSON text: the audit details if it was not (the target is not active
    /// or already has it).
    pub skipped: String,
}

/// A decision's change to an open or deferred obligation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObligationChange {
    pub id: String,
    /// `declined` or `deferred`.
    pub state: String,
    /// JSON text: the disposition.
    pub details: String,
    /// The new due time, or `None` to keep it.
    pub due: Option<f64>,
}

/// An ask a decision extracted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewAsk {
    /// Its ID and dedup key.
    pub id: String,
    /// JSON text.
    pub source: String,
    pub summary: String,
    pub due: f64,
}

/// A turn's life: load, fence, settle or commit.
pub trait Turns {
    /// The thread's state and the processing item; errors unless both exist.
    fn turn_input(&mut self, session: &str, id: i64) -> Result<TurnInput>;
    /// JSON text each: up to 10 root messages in the channel before
    /// `root_ts`, newest first.
    fn earlier_roots(
        &mut self,
        workspace: Option<&str>,
        channel: Option<&str>,
        root_ts: Option<&str>,
    ) -> Result<Vec<String>>;
    /// The thread's decisions (JSON text) and last debriefed turn.
    fn turn_decisions(&mut self, session: &str) -> Result<(String, i64)>;
    /// Whether the thread is active, still at `version`, and the item is
    /// still being processed.
    fn turn_live(&mut self, session: &str, version: Option<i64>, id: i64) -> Result<bool>;
    /// Settle a turn: a stale one goes back to pending; a current one waits
    /// until `until` or finishes with its verdict. The item's unposted
    /// reservation is released either way. Returns whether it was current.
    fn settle_turn(&mut self, settlement: &Settlement) -> Result<bool>;
    /// The item's attempts and state.
    fn inbox_attempts(&mut self, id: i64) -> Result<(i64, String)>;
    /// Record a processing item's failure: count the attempt, release its
    /// reservation, open a signal (unless the item is itself a signal's) and
    /// audit it.
    fn fail_turn(&mut self, failure: &TurnFailure) -> Result<()>;
    /// Keep the calls and put the item back: until `retry_at` if the turn is
    /// current at `version`, at once otherwise. Returns whether it was current.
    fn retry_turn(&mut self, retry: &TurnRetry) -> Result<bool>;
    /// Settle a triaged message: keep the calls, record its verdict, finish
    /// the item and bump the thread's version. A stale turn only goes back to
    /// pending. Returns whether it was current.
    fn settle_triage(&mut self, triage: &TriageSettlement) -> Result<bool>;
    /// On startup: every processing item is pending again. Returns how many.
    fn recover_turns(&mut self) -> Result<usize>;
    /// The thread's version and whether the turn is still active.
    fn fence_turn(&mut self, session: &str, id: i64) -> Result<Fence>;
    /// Whether a message from someone other than the owner's own posts came
    /// after `seen` (unless `None`), and whether the item was reread; marks
    /// it reread the first time.
    fn arrival(
        &mut self,
        session: &str,
        id: i64,
        seen: Option<f64>,
        owner: &str,
    ) -> Result<Arrival>;
    /// A stale turn: its item goes back to pending, its unposted reservation
    /// is released.
    fn return_turn(&mut self, id: i64) -> Result<()>;
    /// Hold the item until `until` and release its unposted reservation.
    fn defer_turn(&mut self, id: i64, until: f64) -> Result<()>;
    /// Keep a committed turn's calls (`action` is JSON text) and audit those
    /// that blocked the thread.
    fn record_parent_calls(
        &mut self,
        session: &str,
        id: i64,
        action: &str,
        calls: &[ParentTurn],
        now: f64,
    ) -> Result<()>;
    /// Record the thread's state after a committed turn and bump its version.
    fn close_turn(&mut self, close: &TurnClose) -> Result<()>;
    /// Record the message's verdict, finish the item and release its
    /// unposted reservation.
    fn finish_turn(&mut self, id: i64, event: Option<&str>, verdict: &str) -> Result<()>;
}

/// Thread and channel state a committed turn changes.
pub trait Sessions {
    /// When the channel last got an unsolicited reply.
    fn last_unsolicited(
        &mut self,
        workspace: Option<&str>,
        channel: Option<&str>,
    ) -> Result<Option<f64>>;
    /// The channel got an unsolicited reply now.
    fn mark_unsolicited(
        &mut self,
        workspace: Option<&str>,
        channel: Option<&str>,
        now: f64,
    ) -> Result<()>;
    /// Merge `patch` (JSON text) into the thread's context.
    fn patch_context(&mut self, session: &str, patch: &str) -> Result<()>;
    /// Label a queued post with its meta (JSON text) and triggering event,
    /// and as a report when `report`.
    fn label_post(&mut self, post: i64, meta: &str, trigger: &str, report: bool) -> Result<()>;
    /// Who the inbox item's reply answers, or `peer`.
    fn handoff_class(&mut self, id: i64) -> Result<String>;
    /// Queue hand-offs in their active targets' inboxes, once each, and
    /// audit each.
    fn queue_handoffs(&mut self, handoffs: &[QueuedHandoff], now: f64) -> Result<()>;
}

/// Jobs a committed turn reports on.
pub trait Jobs {
    /// The jobs were reported.
    fn mark_reported(&mut self, session: &str, jobs: &[Option<String>]) -> Result<()>;
    /// Whether the thread has a queued or running job.
    fn jobs_running(&mut self, session: &str) -> Result<bool>;
}

/// The obligations a committed turn settles or opens.
pub trait TurnObligations {
    /// A hand-off's post answers these obligations of thread `from`: each
    /// still open or deferred awaits the post's delivery.
    fn answer_for_handoff(
        &mut self,
        from: &str,
        obligations: &[String],
        post: i64,
        now: f64,
    ) -> Result<()>;
    /// Apply the changes, in order; stops and returns false at the first
    /// obligation no longer open or deferred in the thread.
    fn change_obligations(
        &mut self,
        session: &str,
        changes: &[ObligationChange],
        now: f64,
    ) -> Result<bool>;
    /// Open the asks.
    fn open_asks(&mut self, session: &str, asks: &[NewAsk], now: f64) -> Result<()>;
    /// Open a "needs attention" signal for a streak unless `id` exists
    /// (`source` is JSON text).
    fn open_streak_signal(&mut self, id: &str, session: &str, source: &str, now: f64)
        -> Result<()>;
}
