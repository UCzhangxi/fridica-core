//! The storage contract: what Fridica keeps, independent of how (fridica#117).
//!
//! A backend implements [`Store`] and every area trait for its [`Unit`]. Fridica
//! runs all its reads and writes for one step through a unit of work
//! ([`transact`](trait.Store.html#method.transact)): they commit together, or none
//! of them does. Area methods take and return whole records or batches, so a
//! unit of work makes no round trip per row.
//!
//! Recorded payloads and details are JSON text, kept byte for byte: the replay
//! tapes compare them exactly.
//!
//! The areas grow as Fridica's queries move behind them; until then the traits
//! are unstable between minor versions.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{any::Any, future::Future, pin::Pin};

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

/// Everything one unit of work can do. Each area adds its trait here.
pub trait Unit: Ledger + Health {}
impl<T: Ledger + Health + ?Sized> Unit for T {}

/// A unit of work, type-erased so [`Store`] stays object safe.
pub type Work = Box<dyn FnOnce(&mut dyn Unit) -> Result<Box<dyn Any + Send>> + Send>;
/// The result of [`Store::run`].
pub type Pending<'a> = Pin<Box<dyn Future<Output = Result<Box<dyn Any + Send>>> + Send + 'a>>;

/// A storage backend.
pub trait Store: Send + Sync {
    /// Run `work` as one unit: commit what it did if it returns `Ok`, discard
    /// all of it otherwise. Callers use [`transact`](Self::transact).
    fn run(&self, work: Work) -> Pending<'_>;

    /// Run `work` as one unit of work and return its result.
    fn transact<R, F>(&self, work: F) -> impl Future<Output = Result<R>> + Send + '_
    where
        Self: Sized,
        R: Send + 'static,
        F: FnOnce(&mut dyn Unit) -> Result<R> + Send + 'static,
    {
        transact(self, work)
    }
}

/// [`Store::transact`] for a `dyn Store`.
pub async fn transact<R, F>(store: &(impl Store + ?Sized), work: F) -> Result<R>
where
    R: Send + 'static,
    F: FnOnce(&mut dyn Unit) -> Result<R> + Send + 'static,
{
    let result = store
        .run(Box::new(move |unit| {
            work(unit).map(|r| Box::new(r) as Box<dyn Any + Send>)
        }))
        .await?;
    Ok(*result
        .downcast::<R>()
        .expect("a store returns what its unit of work returned"))
}
