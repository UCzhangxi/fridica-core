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
use std::{any::Any, future::Future, pin::Pin};

mod attention;
pub use attention::{
    ArrivedMessage, Backfill, Disposal, HistoricalMention, HistoricalObligation, Inbox, InboxItem,
    Mention, MentionQuery, Obligations, QueuedAnswer, RecentReply, Replies, ReservedReply, Route,
};
mod controls;
pub use controls::{
    ControlState, LinkedJob, LinkedMessage, LinkedThread, LinkedThreads, OpenAsk, ResumePoint,
    ThreadControls, WorkerStop, WorkerStops,
};
mod events;
pub use events::{
    FeedLookups, GithubPause, LedgerLookups, OutboxPost, RecordedNames, SlackIdentity, SlackNames,
};
mod ingest;
pub use ingest::{Catchup, FileLookups, SocketStatus, Supervision, Watermark};
mod ledger;
pub use ledger::{Event, Health, Ledger};
mod neighbours;
pub use neighbours::{
    DebriefOrigin, DebriefPost, DebriefTurn, Debriefs, InboxEntry, LastReply, ProgressNote,
    ProgressState, ReplyEvidence, ResultSnapshots, Runtime, RuntimeStart, ThreadMemory,
    ThreadTurns,
};
mod views;
pub use views::{Cell, MessageFiles, OwnerNotes, Row, Status, Views};

/// Everything one unit of work can do. Each area defines its traits in a
/// module of its own and adds them here.
pub trait Unit:
    Ledger
    + Health
    + LedgerLookups
    + SlackNames
    + FeedLookups
    + GithubPause
    + Views
    + OwnerNotes
    + Catchup
    + SocketStatus
    + FileLookups
    + Supervision
    + Inbox
    + Replies
    + Obligations
    + ThreadControls
    + WorkerStops
    + LinkedThreads
    + Runtime
    + ThreadMemory
    + ResultSnapshots
    + ReplyEvidence
    + Debriefs
    + ThreadTurns
{
}
impl<T> Unit for T where
    T: ?Sized
        + Ledger
        + Health
        + LedgerLookups
        + SlackNames
        + FeedLookups
        + GithubPause
        + Views
        + OwnerNotes
        + Catchup
        + SocketStatus
        + FileLookups
        + Supervision
        + Inbox
        + Replies
        + Obligations
        + ThreadControls
        + WorkerStops
        + LinkedThreads
        + Runtime
        + ThreadMemory
        + ResultSnapshots
        + ReplyEvidence
        + Debriefs
        + ThreadTurns
{
}

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
