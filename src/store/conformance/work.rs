//! The outbox, jobs, approvals, fetches, worker controls and links.
use super::{claim, message, post, thread_with_job, Backend};
use crate::{
    delivery::DeliveryOutcome,
    parent::{ParentRequest, WorkerControl, WorkerOperation},
    store::{
        ApprovalSettlement, ApprovalStart, Completed, Completion, NewApproval, PostOutcome, Store,
    },
    worker::ApprovalDecision,
    Authority,
};

/// A key is queued once, with the same content; a post waits for the one it
/// comes after; a late result of another attempt is recorded and reported as
/// stale; only the owner retries; a send interrupted by a stop is ambiguous.
pub async fn the_outbox_queues_once_and_fences_delivery_attempts<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let (first, again, conflict, ready) = store
        .transact(|u| {
            let first = u.queue_post(&post("k1", "hi", ""), 1.0)?;
            let again = u.queue_post(&post("k1", "hi", ""), 1.0)?;
            let conflict = u
                .queue_post(&post("k1", "changed", ""), 1.0)
                .unwrap_err()
                .to_string();
            u.queue_post(&post("k2", "next", "k1"), 1.0)?;
            Ok((first, again, conflict, u.ready_posts(2.0, 10)?))
        })
        .await
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(
        conflict,
        "outbox idempotency key reused with different content"
    );
    // The second post waits for the first.
    assert_eq!(ready, [first]);
    let claim = store
        .transact(|u| u.claim_post(2.0, None))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((claim.id, claim.attempt), (first, 1));
    // A late result of another attempt is recorded, and reported as stale.
    let mut late = claim.clone();
    late.attempt = 9;
    let sent = DeliveryOutcome::Sent {
        reference: "5.000001".into(),
    };
    let (stale, done) = store
        .transact(move |u| {
            let stale = u.finish_delivery(&late, &sent, "U1", 3.0)?;
            Ok((stale, u.finish_delivery(&claim, &sent, "U1", 3.0)?))
        })
        .await
        .unwrap();
    assert_eq!((stale, done), (PostOutcome::Stale, PostOutcome::Sent));
    let (second, refused) = store
        .transact(|u| {
            let second = u.claim_post(4.0, None)?.unwrap();
            let refused = u.finish_delivery(
                &second,
                &DeliveryOutcome::Rejected {
                    code: "nope".into(),
                },
                "U1",
                4.0,
            )?;
            Ok((second, refused))
        })
        .await
        .unwrap();
    assert_eq!(refused, PostOutcome::Unsent);
    let (by_other, retried, recovered) = store
        .transact(move |u| {
            let by_other = u.retry_post(second.id, Authority::System, 5.0).is_err();
            let retried = u.retry_post(second.id, Authority::Owner, 5.0)?;
            u.claim_post(6.0, Some(second.id))?.unwrap();
            Ok((by_other, retried, u.recover_posts(7.0)?))
        })
        .await
        .unwrap();
    assert!(by_other && retried);
    assert_eq!(recovered, 1);
    let (posts, events, messages) = store
        .transact(|u| {
            Ok((
                u.thread_posts("T:C:1")?,
                u.events_after(0, 100)?,
                u.thread_messages("T:C:1", 10)?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(
        posts
            .iter()
            .map(|post| (post.state.as_str(), post.error.as_str()))
            .collect::<Vec<_>>(),
        [("sent", ""), ("ambiguous", "daemon_stopped_during_send")]
    );
    assert_eq!(
        events.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        [
            "delivery_call",
            "delivery_late",
            "delivery",
            "delivery_call",
            "delivery",
            "delivery_call"
        ]
    );
    // The sent reply is in the thread's history as Fridica's own message.
    let echo = messages
        .iter()
        .find(|message| message.ts == "5.000001")
        .expect("the sent reply is kept as a message");
    assert_eq!(echo.source, "self");
}

/// A post being sent is confirmed once.
pub async fn a_post_being_sent_is_confirmed_once<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let id = store
        .transact(|u| {
            let id = u.queue_post(&post("k1", "hi", ""), 1.0)?;
            u.claim_post(2.0, Some(id))?;
            u.confirm_post(id, "5.1", 3.0)?;
            Ok(id)
        })
        .await
        .unwrap();
    let again = store
        .transact(move |u| u.confirm_post(id, "5.1", 4.0))
        .await
        .unwrap_err();
    assert_eq!(again.to_string(), "post was not being sent");
}

/// A job is claimed into a slot, its progress recorded for the running
/// attempt only, its completion fenced by attempt; a stopped worker stays
/// stopped.
pub async fn jobs_are_claimed_progressed_and_completed<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    thread_with_job(&store).await;
    let snapshot = store.transact(|u| u.work_snapshot()).await.unwrap();
    assert_eq!(
        (
            snapshot.queued.len(),
            snapshot.running.len(),
            snapshot.workers.len()
        ),
        (1, 0, 1)
    );
    let claimed = claim(&store, "j1", 1).await;
    assert_eq!(
        (
            claimed.job.status.as_str(),
            claimed.job.attempt,
            claimed.worker.slot
        ),
        ("running", 1, 1)
    );
    let (progressed, stale_progress, busy, context, missing) = store
        .transact(|u| {
            Ok((
                u.record_job_progress("j1", 1, "halfway", 4.0)?,
                u.record_job_progress("j1", 7, "old", 4.0)?,
                u.busy_by_machine()?,
                u.work_context("T:C:1")?,
                u.job_record("nope").is_err(),
            ))
        })
        .await
        .unwrap();
    assert!(progressed && !stale_progress && missing);
    assert_eq!(busy.get("m"), Some(&1));
    assert_eq!(context["progress"][0]["note"], "halfway");
    assert_eq!(context["jobs"][0]["id"], "j1");
    let completion: Completion = serde_json::from_value(serde_json::json!({
        "outcome":{"Err":{"kind":"refusal","code":"no"}},
        "artifacts":[],"interrupted":false,"stopped":false,"allow_retry":false
    }))
    .unwrap();
    let (stale, done, job, worker, files) = store
        .transact(move |u| {
            Ok((
                u.complete_job("j1", 9, &completion, 5.0)?,
                u.complete_job("j1", 1, &completion, 5.0)?,
                u.job_record("j1")?,
                u.worker_record("w1")?,
                u.delegable_files("T:C:1")?,
            ))
        })
        .await
        .unwrap();
    assert_eq!((stale, done), (Completed::Stale, Completed::Finished));
    assert_eq!(job.status, "failed");
    assert_eq!(worker.status, "idle");
    assert!(files.is_empty());
    let (snapshot, recovered) = store
        .transact(|u| {
            u.stop_worker("w1", "owner", 6.0)?;
            Ok((u.previous_snapshot("w1", "j2")?, u.recover_jobs(7.0)?))
        })
        .await
        .unwrap();
    assert_eq!(snapshot, None);
    assert_eq!(recovered, 0);
    assert_eq!(
        store
            .transact(|u| u.worker_record("w1"))
            .await
            .unwrap()
            .status,
        "stopped"
    );
}

/// An approval waits for the owner or is decided by policy; it is settled
/// once; an interrupt and a configuration change cancel pending ones.
pub async fn approvals_begin_settle_and_cancel<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    thread_with_job(&store).await;
    let claimed = claim(&store, "j1", 1).await;
    let request = |id: &str, automatic| NewApproval {
        id: id.into(),
        worker: claimed.worker.clone(),
        job: claimed.job.clone(),
        request: serde_json::from_value(serde_json::json!({
            "backend_request_id":"b","kind":"tool","summary":"run it","detail":{"b":1,"a":2}
        }))
        .unwrap(),
        automatic,
        now: 4.0,
        expires_at: 100.0,
    };
    let (first, second, automatic) = (
        request("a1", None),
        request("a2", None),
        request("a3", Some(ApprovalDecision::Deny)),
    );
    let (started, denied, pending) = store
        .transact(move |u| {
            let started = u.begin_approval(&first)?;
            u.begin_approval(&second)?;
            Ok((
                started,
                u.begin_approval(&automatic)?,
                u.pending_approvals(10)?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(started, ApprovalStart::Pending);
    assert_eq!(denied, ApprovalStart::Immediate(ApprovalDecision::Deny));
    assert_eq!(pending.len(), 2);
    let (accepted, repeated, cancelled, after) = store
        .transact(|u| {
            let accepted = u.settle_approval(
                "a1",
                ApprovalSettlement::Decide(ApprovalDecision::Once),
                "owner",
                5.0,
            )?;
            let repeated = u.settle_approval("a1", ApprovalSettlement::Cancel, "owner", 5.0)?;
            u.cancel_worker_approvals("w1", 6.0)?;
            let cancelled = u.approval("a2")?.unwrap();
            u.cancel_pending_approvals(7.0)?;
            Ok((accepted, repeated, cancelled, u.approval("a1")?.unwrap()))
        })
        .await
        .unwrap();
    assert!(accepted && !repeated);
    assert_eq!(
        (cancelled.status.as_str(), cancelled.decided_by.as_str()),
        ("cancelled", "interrupted")
    );
    assert_eq!(
        (after.status.as_str(), after.scope.as_str()),
        ("approved", "once")
    );
    assert_eq!(
        store
            .transact(|u| u.approval("nope"))
            .await
            .unwrap()
            .map(|a| a.id),
        None
    );
}

/// A fetch is recorded in the ledger, completed once, and only for the job's
/// current attempt.
pub async fn a_fetch_is_fenced_by_its_job_attempt<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    thread_with_job(&store).await;
    let claimed = claim(&store, "j1", 1).await;
    let job = claimed.job.clone();
    let (seq, accepted, again) = store
        .transact(move |u| {
            let request = serde_json::json!({"repo":"r"});
            let seq = u.begin_fetch(&job, &request, 4.0)?.unwrap();
            let result = serde_json::json!({"commit":"abc"});
            let accepted = u.finish_fetch(&job, seq, &result, 5.0)?;
            Ok((
                seq,
                accepted,
                u.finish_fetch(&job, seq, &result, 6.0).is_err(),
            ))
        })
        .await
        .unwrap();
    assert!(accepted && again);
    let events = store
        .transact(move |u| u.events_after(seq - 1, 1))
        .await
        .unwrap();
    assert_eq!(events[0].kind, "repo_fetch");
    assert!(events[0].complete);
    // A stale attempt fetches nothing.
    let mut stale = claimed.job;
    stale.attempt = 5;
    assert_eq!(
        store
            .transact(move |u| u.begin_fetch(&stale, &serde_json::json!({}), 7.0))
            .await
            .unwrap(),
        None
    );
}

/// A worker control is checked against the jobs the parent saw, queued as a
/// pending intent, and completed with its outcome.
pub async fn worker_controls_are_queued_current_and_completed<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    thread_with_job(&store).await;
    claim(&store, "j1", 1).await;
    let jobs = store
        .transact(|u| u.controlled_jobs("T:C:1"))
        .await
        .unwrap();
    let request: ParentRequest = serde_json::from_value(serde_json::json!({
        "inbox_id":4,"call":"c","session":{"id":"T:C:1","work":{"jobs":jobs}},"trigger":{},
        "history":[],"obligations":[],"previous":null,"errors":[]
    }))
    .unwrap();
    let controls = [WorkerControl {
        worker_id: "w1".into(),
        op: WorkerOperation::Interrupt,
    }];
    let (current, pending, interrupt, listed) = store
        .transact(move |u| {
            let current = u.worker_controls_current(&request, &controls)?;
            u.queue_worker_controls(&request, &controls, 5.0)?;
            Ok((
                current,
                u.worker_control_pending("T:C:1")?,
                u.interrupt_pending("j1", 1)?,
                u.pending_worker_controls()?,
            ))
        })
        .await
        .unwrap();
    assert!(current && pending && interrupt);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (
            listed[0].intent.worker.as_str(),
            listed[0].intent.job.as_deref(),
            listed[0].intent.attempt
        ),
        ("w1", Some("j1"), Some(1))
    );
    let seq = listed[0].seq;
    let (pending, recent) = store
        .transact(move |u| {
            u.complete_worker_control(seq, "interrupted", 6.0)?;
            Ok((
                u.worker_control_pending("T:C:1")?,
                u.recent_worker_controls("T:C:1")?,
            ))
        })
        .await
        .unwrap();
    assert!(!pending);
    assert_eq!(recent[0]["outcome"], "interrupted");
    assert_eq!(recent[0]["complete"], true);
}

/// Links are recorded for a message and backfilled once; threads sharing an
/// item are then linked.
pub async fn links_are_recorded_and_backfilled_once<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    store
        .transact(|u| {
            u.open_thread("T:C:1", "T", "C", "1", 1.0)?;
            u.open_thread("T:C:2", "T", "C", "2", 1.0)?;
            u.keep_message(&message(
                "e1",
                "C",
                "2",
                "2",
                "U",
                "see snapy/x#12",
                "socket",
                90.0,
                "[]",
            ))
        })
        .await
        .unwrap();
    let (first, second) = store
        .transact(|u| {
            u.record_links(
                "T",
                "C",
                "T:C:1",
                "about #12 and thread 1000000002.000000 and 2",
                5.0,
            )?;
            Ok((
                u.backfill_links(100.0, 50.0)?,
                u.backfill_links(100.0, 50.0)?,
            ))
        })
        .await
        .unwrap();
    assert_eq!((first, second), (1, 0));
    let linked = store
        .transact(|u| u.linked_threads("T:C:1", 10, 1000.0))
        .await
        .unwrap();
    assert_eq!(
        linked
            .iter()
            .map(|t| (t.id.as_str(), t.shared_items.as_str()))
            .collect::<Vec<_>>(),
        [("T:C:2", "x#12")]
    );
}
