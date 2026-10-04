//! Attention: messages arriving in threads, reply capacity, and the
//! obligations Fridica owes the owner.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A Slack message as intake keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrivedMessage {
    pub event_id: String,
    pub workspace: String,
    pub channel: String,
    pub ts: String,
    /// The thread's root: `thread_ts`, or `ts` for a root message.
    pub root_ts: String,
    pub thread_ts: Option<String>,
    pub sender: String,
    pub text: String,
    /// JSON text.
    pub files: String,
    /// Where it came from (`socket`, `self`, ...).
    pub source: String,
    /// JSON text.
    pub meta: Option<String>,
    pub received_at: f64,
    /// JSON text.
    pub attachments: String,
    pub mentions_owner: bool,
}

/// An inbox item a thread's actor claimed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: i64,
    /// What it is (`message`, `obligation_due`, ...).
    pub kind: String,
}

/// Where a thread's replies go.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub channel: String,
    pub root_ts: String,
}

/// A reply that counts against a thread's hourly limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecentReply {
    /// Who the reply answered (`peer`, `human`).
    pub trigger: String,
    /// When it was delivered, or `now` (or its later reservation) while
    /// still reserved.
    pub at: f64,
}

/// A reply reservation on an inbox item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReservedReply {
    /// Who the reply answers (`owner`, `peer`, `human`).
    pub trigger: String,
    /// The outbox post already queued for it, if any.
    pub post: Option<i64>,
}

/// A reply queued as an outbox post, with the obligations it answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueuedAnswer {
    pub post: i64,
    pub session: String,
    pub inbox: i64,
    pub trigger: String,
    /// The obligation IDs it answers.
    pub obligations: Vec<String>,
    /// The same IDs as JSON text, as the post records them.
    pub answers: String,
    pub time: f64,
}

/// An obligation opened because the owner was mentioned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mention {
    pub id: String,
    pub session: String,
    pub dedup_key: String,
    /// JSON text.
    pub source: String,
    pub created: f64,
    pub due: f64,
}

/// A change to an obligation's state by an actor, audited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Disposal {
    pub id: String,
    /// The new state (`declined`, `deferred`, ...).
    pub state: String,
    /// JSON text: the disposition.
    pub details: String,
    /// The new due time, or `None` to keep it.
    pub due: Option<f64>,
    /// JSON text: who did it.
    pub actor: String,
    pub time: f64,
}

/// What a historical mention review searches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MentionQuery {
    pub workspace: String,
    /// JSON text: the channel IDs.
    pub channels: String,
    pub since: f64,
    pub until: f64,
    /// The owner's member ID; the owner's own messages are skipped.
    pub owner: String,
    /// The text a mention contains.
    pub mention: String,
}

/// A historical message that mentions the owner and has no obligation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoricalMention {
    pub event_id: String,
    pub session: String,
    pub workspace: String,
    pub channel: String,
    pub ts: String,
    pub received_at: f64,
}

/// An obligation a historical review opens, deferred for owner review.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoricalObligation {
    pub id: String,
    pub session: String,
    pub dedup_key: String,
    /// The message that mentioned the owner.
    pub event_id: String,
    /// JSON text.
    pub source: String,
    pub created: f64,
    pub due: f64,
    /// JSON text.
    pub state: String,
    pub updated: f64,
}

/// An applied historical review, audited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Backfill {
    pub obligations: Vec<HistoricalObligation>,
    pub time: f64,
    pub actor: String,
    pub client_id: String,
    /// JSON text: what the review reports.
    pub result: String,
}

/// Messages arriving in threads and the thread inbox.
pub trait Inbox {
    /// Whether the thread is waiting for an answer.
    fn thread_waiting(&mut self, session: &str) -> Result<bool>;
    /// Keep a message unless one with its event ID is kept; returns whether
    /// it was new.
    fn keep_message(&mut self, message: &ArrivedMessage) -> Result<bool>;
    /// Open an active thread unless it exists.
    fn open_thread(
        &mut self,
        session: &str,
        workspace: &str,
        channel: &str,
        root_ts: &str,
        created: f64,
    ) -> Result<()>;
    /// Queue a message for its thread and return the inbox item's ID.
    fn queue_message(&mut self, session: &str, event_id: &str, created: f64) -> Result<i64>;
    /// Claim the thread's oldest pending item due by `now`, unless one is
    /// already being processed.
    fn claim_next(&mut self, session: &str, now: f64) -> Result<Option<InboxItem>>;
}

/// Reply reservations: the capacity a thread spends on replies.
pub trait Replies {
    /// Whether the thread is active; errors if there is no such thread.
    fn thread_active(&mut self, session: &str) -> Result<bool>;
    /// Whether the inbox item belongs to the thread and is pending or being
    /// processed.
    fn inbox_open(&mut self, inbox: i64, session: &str) -> Result<bool>;
    /// The state of the inbox item's reservation, if any.
    fn reservation_state(&mut self, inbox: i64) -> Result<Option<String>>;
    /// The thread's non-owner replies reserved, or delivered in the hour
    /// before `now`, by time.
    fn recent_replies(&mut self, session: &str, now: f64) -> Result<Vec<RecentReply>>;
    /// Hold the inbox item and throttle its thread until `until`.
    fn defer_reply(&mut self, session: &str, inbox: i64, until: f64) -> Result<()>;
    /// Reserve a reply to the inbox item, or reserve a released one again.
    fn reserve_reply(
        &mut self,
        id: &str,
        session: &str,
        inbox: i64,
        trigger: &str,
        now: f64,
    ) -> Result<()>;
    /// The inbox item's live reservation in the thread; errors if there is
    /// none.
    fn reserved_reply(&mut self, inbox: i64, session: &str) -> Result<ReservedReply>;
    /// Where the thread's replies go; errors if there is no such thread.
    fn thread_route(&mut self, session: &str) -> Result<Route>;
    /// Record a queued answer: the post's obligations await delivery, the
    /// reservation names the post and the inbox item is done. Stops at the
    /// first obligation not open in the thread and returns its ID.
    fn answer_queued(&mut self, answer: &QueuedAnswer) -> Result<Option<String>>;
}

/// Obligations: what the owner is owed an answer to.
pub trait Obligations {
    /// Open a mention obligation unless one with its ID exists.
    fn open_mention(&mut self, mention: &Mention) -> Result<()>;
    /// Open a "needs attention" signal for the thread unless `id` exists;
    /// returns whether it was opened.
    fn open_signal(&mut self, id: &str, session: &str, now: f64) -> Result<bool>;
    /// The obligation's state; errors if there is no such obligation.
    fn obligation_state(&mut self, id: &str) -> Result<String>;
    /// Change a live obligation's state and audit it; returns whether it
    /// was live.
    fn dispose(&mut self, disposal: &Disposal) -> Result<bool>;
    /// Queue every open or deferred obligation due by `now` in its thread's
    /// inbox, once per due time; returns how many were queued.
    fn queue_due(&mut self, now: f64) -> Result<usize>;
    /// Up to 1001 messages matching `query` in live threads without an
    /// obligation, oldest first.
    fn historical_mentions(&mut self, query: &MentionQuery) -> Result<Vec<HistoricalMention>>;
    /// Open a historical review's obligations, mark their messages as
    /// mentioning the owner, and audit the review.
    fn apply_backfill(&mut self, backfill: &Backfill) -> Result<()>;
}
