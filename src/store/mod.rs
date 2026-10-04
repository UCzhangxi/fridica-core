//! The storage contract: what Fridica keeps, independent of how (fridica#117).
//!
//! A backend implements [`Store::run`] and every area trait for its [`Unit`].
//! Fridica runs all its reads and writes for one step through a unit of work
//! ([`transact`](trait.Store.html#method.transact)):
//!
//! - **All or nothing.** What a unit wrote commits when it returns `Ok`; when it
//!   returns `Err`, none of it does, and the error comes back as raised.
//! - **One at a time.** Units are serialised, or isolated as if they were.
//!
//! Area methods take and return whole records or batches, so a unit of work
//! makes no round trip per row.
//!
//! Recorded payloads and details are JSON text, kept byte for byte: the replay
//! tapes compare them exactly. Order is part of the contract (the ledger's
//! sequence numbers increase; each method says how its results are ordered).
//! Errors are [`anyhow::Error`] with the backend's own error inside.
//!
//! Opening and locking a store, migrating it and the weekly archives are each
//! backend's own API, outside the unit of work.
//!
//! With the `conformance` feature, `conformance` is the suite a backend runs
//! from its tests (`fridica_core::conformance_tests!`) to check all of this
//! through the traits alone.
//!
//! The areas grow as Fridica's queries move behind them; until then the traits
//! are unstable between minor versions.
use anyhow::Result;
use std::{any::Any, future::Future, pin::Pin, sync::Arc};

mod actor;
pub use actor::{
    Arrival, Fence, NewAsk, ObligationChange, ParentTurn, QueuedHandoff, Sessions, Settlement,
    TriageSettlement, TurnClose, TurnFailure, TurnInput, TurnJobs, TurnObligations, TurnRetry,
    Turns,
};
mod attention;
#[cfg(feature = "conformance")]
pub mod conformance;
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
mod modules;
pub use modules::{
    ApprovalSettlement, ApprovalStart, Approvals, Archive, ArchiveHit, ClaimedJob, Completed,
    Completion, ConfigurationIntent, ConfigurationIntents, Diagnostics, Fetches, Jobs, Links,
    NewApproval, Outbox, PendingConfigurationEdit, PendingWorkerControl, PostOutcome,
    PreviousSnapshot, WorkSnapshot, WorkerControlIntent, WorkerControls,
};
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
    + Turns
    + Sessions
    + TurnJobs
    + Jobs
    + TurnObligations
    + Outbox
    + Approvals
    + Fetches
    + WorkerControls
    + Diagnostics
    + Links
    + Archive
    + ConfigurationIntents
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
        + Turns
        + Sessions
        + TurnJobs
        + Jobs
        + TurnObligations
        + Outbox
        + Approvals
        + Fetches
        + WorkerControls
        + Diagnostics
        + Links
        + Archive
        + ConfigurationIntents
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

/// A shared store is a store, so a component holding `Arc<dyn Store>` calls
/// [`Store::transact`] on it like a concrete backend would.
impl<S: Store + ?Sized> Store for Arc<S> {
    fn run(&self, work: Work) -> Pending<'_> {
        (**self).run(work)
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
