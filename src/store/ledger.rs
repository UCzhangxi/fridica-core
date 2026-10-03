//! The replay ledger and health events.
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// One entry of the replay ledger: every boundary Fridica crosses (a Slack
/// event, a backend call, a commit) in order. A call is recorded incomplete
/// and completed when its result is recorded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    pub kind: String,
    pub time: f64,
    /// JSON text.
    pub payload: String,
    pub complete: bool,
}

/// The replay ledger.
pub trait Ledger {
    /// Append an event and return its sequence number, which is greater than
    /// every earlier one.
    fn record(&mut self, kind: &str, time: f64, payload: &str, complete: bool) -> Result<i64>;
    /// Mark an event complete (or not).
    fn complete(&mut self, seq: i64, complete: bool) -> Result<()>;
    /// The last sequence number, or 0 for an empty ledger.
    fn last_seq(&mut self) -> Result<i64>;
    /// Up to `limit` events after `seq`, in order.
    fn events_after(&mut self, seq: i64, limit: usize) -> Result<Vec<Event>>;
}

/// Health events: what went wrong or was skipped, for `fridica doctor` and the
/// weekly report.
pub trait Health {
    /// Record a health event (`details` is JSON text) and return its id.
    fn note(&mut self, kind: &str, details: &str, created: f64) -> Result<i64>;
    /// Record a health event unless one of the same kind was recorded after
    /// `since`; returns whether it was recorded.
    fn note_unless_since(
        &mut self,
        kind: &str,
        details: &str,
        created: f64,
        since: f64,
    ) -> Result<bool>;
    /// Record a health event unless one of the same kind already has the
    /// same top-level `fields` in its details; returns whether it was recorded.
    fn note_unless_noted(
        &mut self,
        kind: &str,
        details: &str,
        created: f64,
        fields: &[&str],
    ) -> Result<bool>;
    /// How many health events were recorded in `[from, to)`.
    fn count_between(&mut self, from: f64, to: f64) -> Result<i64>;
}
