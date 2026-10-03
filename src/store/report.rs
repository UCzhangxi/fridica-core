//! The daily channel report: its inputs, the kept reports, their exports and
//! their posts.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// What happened in one channel during a time window, as the daily report
/// counts it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelActivity {
    /// Messages received, other than Fridica's own.
    pub messages: i64,
    /// Messages that mention the owner.
    pub mentions: i64,
    /// Replies delivered.
    pub replies_delivered: i64,
    /// Replies queued in the window whose delivery is ambiguous.
    pub replies_ambiguous: i64,
    /// Obligations created in the window and still open.
    pub obligations_open: i64,
    /// Obligations answered in the window.
    pub obligations_answered: i64,
    /// Jobs that finished in the window, however they ended.
    pub jobs_finished: i64,
}

/// A generated report, as kept.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub channel: String,
    /// The local date, `YYYY-MM-DD`.
    pub day: String,
    pub timezone: String,
    /// JSON text.
    pub data: String,
    pub markdown: String,
    pub created: f64,
}

/// A report waiting to be written out, at the export generation it was
/// queued with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingExport {
    pub channel: String,
    pub day: String,
    pub markdown: String,
    pub generation: i64,
}

/// Daily reports: what they count, and their export and post bookkeeping.
pub trait Reports {
    /// What happened in `channel` during `[from, to)`.
    fn channel_activity(&mut self, channel: &str, from: f64, to: f64) -> Result<ChannelActivity>;
    /// How many campaign work items were updated during `[from, to)`.
    fn campaign_items_updated(&mut self, from: f64, to: f64) -> Result<i64>;
    /// Keep a report, replacing the channel's report for that day, and queue
    /// its export at a new generation.
    fn keep_report(&mut self, report: &Report) -> Result<()>;
    /// Every report waiting for export, by day then channel.
    fn pending_exports(&mut self) -> Result<Vec<PendingExport>>;
    /// The generation of a channel's report for `day` if its export is
    /// still pending.
    fn pending_export(&mut self, channel: &str, day: &str) -> Result<Option<i64>>;
    /// Mark the export done if it is still at `generation`; returns how many
    /// exports that marked (0 or 1).
    fn mark_exported(&mut self, channel: &str, day: &str, generation: i64) -> Result<usize>;
    /// Queue the kept report for `day` as a post to its channel under
    /// `idem_key`, unless it was queued before; returns whether it was
    /// queued. Fails if no report is kept for that day.
    fn queue_report_post(
        &mut self,
        channel: &str,
        day: &str,
        idem_key: &str,
        session: &str,
        created: f64,
    ) -> Result<bool>;
}
