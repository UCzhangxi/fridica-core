//! The read models, recorded names and runtime values, catch-up, file
//! lookups and the worker supervisor's writes.
use super::{claim, column, message, session_post, text, Backend};
use crate::store::{
    Cell, NewApproval, RecordedNames, Row, RuntimeStart, SlackIdentity, Store, Watermark,
};

/// Views list threads, messages and files as stored: columns keep their
/// names, order and stored types, and JSON stays text.
pub async fn views_read_threads_messages_and_files<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    store
        .transact(|u| {
            u.open_thread("T:C:1", "T", "C", "1", 5.0)?;
            u.open_thread("T:C:2", "T", "C", "2", 1.0)?;
            u.set_control("T:C:2", "paused", r#"{"b":1,"a":2}"#, "", 7.5, false)?;
            u.keep_message(&message(
                "e1",
                "C",
                "1",
                "1",
                "U",
                "one",
                "slack",
                1.0,
                r#"[{"id":"F1"}]"#,
            ))?;
            u.keep_message(&message(
                "e2", "C", "2.5", "1", "U", "two", "slack", 2.0, "[]",
            ))?;
            u.keep_message(&message(
                "e3",
                "C",
                "10",
                "1",
                "U",
                "three",
                "slack",
                3.0,
                r#"[{"id":"F12"}]"#,
            ))?;
            Ok(())
        })
        .await
        .unwrap();
    let (all, paused, attention, one, missing, messages, status) = store
        .transact(|u| {
            Ok((
                u.threads(&[], 10)?,
                u.threads(&["paused".into()], 10)?,
                u.threads_needing_attention()?,
                u.thread("T:C:1")?,
                u.thread("T:C:9")?,
                u.thread_messages("T:C:1", 2)?,
                u.status()?,
            ))
        })
        .await
        .unwrap();
    let id = |row: &Row| row.0[0].1.clone();
    assert_eq!(
        all.iter().map(id).collect::<Vec<_>>(),
        [text("T:C:2"), text("T:C:1")]
    );
    assert_eq!(paused.len(), 1);
    assert_eq!(attention, paused);
    assert_eq!(
        column(&paused[0], "control_detail_json"),
        text(r#"{"b":1,"a":2}"#)
    );
    // Columns keep their names, order and stored types; JSON stays text.
    let one = one.unwrap();
    assert_eq!(one.0[0].0, "id");
    assert_eq!(column(&one, "turns"), Cell::Integer(0));
    assert_eq!(column(&one, "updated"), Cell::Real(5.0));
    assert_eq!(column(&one, "decisions_json"), text("[]"));
    assert_eq!(missing, None);
    // The last messages, oldest first.
    assert_eq!(
        messages
            .iter()
            .map(|m| column(m, "text"))
            .collect::<Vec<_>>(),
        [text("two"), text("three")]
    );
    assert_eq!(column(&messages[0], "meta_json"), Cell::Null);
    assert_eq!(status.runtime, None);
    assert_eq!(status.pending_approvals, 0);
    let (files, unknown, mentioning, latest, exists, approval) = store
        .transact(|u| {
            Ok((
                u.thread_files("T:C:1")?,
                u.thread_files("T:C:9")?,
                u.attachments_mentioning("F1")?,
                u.latest_thread_in("T", "C")?,
                (u.thread_exists("T:C:1")?, u.thread_exists("T:C:9")?),
                u.approval_exists("A1")?,
            ))
        })
        .await
        .unwrap();
    let files = files.unwrap();
    assert_eq!(
        files.iter().map(|f| f.ts.as_str()).collect::<Vec<_>>(),
        ["1", "2.5", "10"]
    );
    assert_eq!(files[0].attachments, r#"[{"id":"F1"}]"#);
    assert_eq!(unknown, None);
    // Only the exact file ID, not one it prefixes.
    assert_eq!(mentioning, [r#"[{"id":"F1"}]"#]);
    assert_eq!(latest.as_deref(), Some("T:C:2"));
    assert_eq!(exists, (true, false));
    assert!(!approval);
}

/// Owner notes are revised once per revision, kept byte for byte, and
/// audited.
pub async fn owner_notes_are_revised_and_audited<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let revisions = store
        .transact(|u| {
            let before = u.notes_revision("T:C:1")?;
            u.write_owner_notes("T:C:1", before + 1, "U1", r#"{"b":1,"a":2}"#, 3.0)?;
            Ok((before, u.notes_revision("T:C:1")?))
        })
        .await
        .unwrap();
    assert_eq!(revisions, (0, 1));
    let (notes, activity) = store
        .transact(|u| Ok((u.thread_notes("T:C:1")?, u.activity(10)?)))
        .await
        .unwrap();
    let notes = serde_json::to_string(&notes.unwrap()).unwrap();
    assert!(notes.contains(r#"{\"b\":1,\"a\":2}"#), "{notes}");
    let activity = serde_json::to_string(&activity).unwrap();
    assert!(
        activity.contains("notes.write") && activity.contains(r#"{\"revision\":1}"#),
        "{activity}"
    );
    // A revision is written once.
    assert!(store
        .transact(|u| u.write_owner_notes("T:C:1", 1, "U1", "{}", 4.0))
        .await
        .is_err());
}

/// Recorded names are `None` until recorded, then kept as recorded; member
/// names are replaced whole.
pub async fn slack_names_are_kept_as_recorded<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let names = store.transact(|u| u.slack_names()).await.unwrap();
    assert_eq!(names, RecordedNames::default());
    let (names, users) = store
        .transact(|u| {
            u.keep_identity(&SlackIdentity {
                scopes: "chat:write".into(),
                channels: r#"{"C1":"room"}"#.into(),
                workspace: "scix".into(),
            })?;
            u.keep_user_names(r#"{"U2":"Bo","U1":"Ada"}"#)?;
            u.keep_user_names(r#"{"U1":"Ada"}"#)?;
            Ok((u.slack_names()?, u.user_names()?))
        })
        .await
        .unwrap();
    assert_eq!(names.workspace.as_deref(), Some("scix"));
    assert_eq!(names.channels.as_deref(), Some(r#"{"C1":"room"}"#));
    assert_eq!(names.users.as_deref(), Some(r#"{"U1":"Ada"}"#));
    assert_eq!(users, names.users);
}

/// The latest identity replaces the previous one, scopes included.
pub async fn slack_identity_is_kept_as_reported<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let (names, scopes) = store
        .transact(|u| {
            u.keep_identity(&SlackIdentity {
                scopes: "old".into(),
                channels: "{}".into(),
                workspace: "old".into(),
            })?;
            u.keep_identity(&SlackIdentity {
                scopes: "chat:write,files:read".into(),
                channels: r#"{"C1":"room"}"#.into(),
                workspace: "scix".into(),
            })?;
            Ok((u.slack_names()?, u.slack_scopes()?))
        })
        .await
        .unwrap();
    assert_eq!(names.workspace.as_deref(), Some("scix"));
    assert_eq!(names.channels.as_deref(), Some(r#"{"C1":"room"}"#));
    assert_eq!(scopes.as_deref(), Some("chat:write,files:read"));
}

/// The scopes are `None` until an identity records them.
pub async fn slack_scopes_are_read_when_recorded<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    assert_eq!(store.transact(|u| u.slack_scopes()).await.unwrap(), None);
    store
        .transact(|u| {
            u.keep_identity(&SlackIdentity {
                scopes: "files:read".into(),
                channels: "{}".into(),
                workspace: "scix".into(),
            })
        })
        .await
        .unwrap();
    assert_eq!(
        store.transact(|u| u.slack_scopes()).await.unwrap(),
        Some("files:read".into())
    );
}

/// The event feed's point reads: a post, a job's thread, a message's arrival.
pub async fn the_feed_reads_posts_jobs_and_messages<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let id = store
        .transact(|u| {
            u.keep_message(&message(
                "e1", "C", "1.0", "1.0", "U", "hi", "socket", 2.5, "[]",
            ))?;
            u.queue_post(&session_post("k", "T:C:1.0", "reply", "hi", ""), 1.0)
        })
        .await
        .unwrap();
    let found = store
        .transact(move |u| {
            Ok((
                u.outbox_post(id)?,
                u.outbox_post(id + 1)?,
                u.job_session("job-1")?,
                u.message_received_at("e1")?,
                u.message_received_at("e2")?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(
        found,
        (
            Some(crate::store::OutboxPost {
                kind: "reply".into(),
                session: "T:C:1.0".into()
            }),
            None,
            None,
            Some(2.5),
            None
        )
    );
}

/// The socket status is kept, and shows in the status view's runtime row.
pub async fn the_socket_status_is_kept_for_the_runtime_row<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let status = store
        .transact(|u| {
            u.start_runtime(&RuntimeStart {
                pid: 1,
                started_at: 1.0,
                observe_only: false,
                config_fingerprint: String::new(),
            })?;
            let before = u.socket_status()?;
            u.keep_socket_status("connected")?;
            Ok((before, u.socket_status()?))
        })
        .await
        .unwrap();
    assert_eq!(status, (None, Some("connected".into())));
    let runtime = store
        .transact(|u| u.status())
        .await
        .unwrap()
        .runtime
        .unwrap();
    assert_eq!(column(&runtime, "slack_status"), text("connected"));
}

/// A start makes the runtime row `starting`; a restart replaces it.
pub async fn the_runtime_row_starts_and_restarts<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    assert_eq!(
        store.transact(|u| u.previous_slack_status()).await.unwrap(),
        None
    );
    store
        .transact(|u| {
            u.start_runtime(&RuntimeStart {
                pid: 42,
                started_at: 10.0,
                observe_only: true,
                config_fingerprint: "f1".into(),
            })?;
            u.advertise_control("/run/sock")?;
            u.heartbeat(11.0)
        })
        .await
        .unwrap();
    let runtime = || async {
        store
            .transact(|u| u.status())
            .await
            .unwrap()
            .runtime
            .unwrap()
    };
    let row = runtime().await;
    assert_eq!(
        (column(&row, "started_at"), column(&row, "slack_status")),
        (Cell::Real(10.0), text("starting"))
    );
    assert_eq!(
        store.transact(|u| u.previous_slack_status()).await.unwrap(),
        Some("starting".into())
    );
    store.transact(|u| u.stop_runtime(12.0)).await.unwrap();
    store
        .transact(|u| {
            u.start_runtime(&RuntimeStart {
                pid: 43,
                started_at: 20.0,
                observe_only: false,
                config_fingerprint: "f2".into(),
            })
        })
        .await
        .unwrap();
    let row = runtime().await;
    assert_eq!(
        (column(&row, "started_at"), column(&row, "slack_status")),
        (Cell::Real(20.0), text("starting"))
    );
}

/// Catch-up reads the channel's newest message and recent threads, and keeps
/// its watermark and count of truncated passes.
pub async fn catch_up_keeps_watermarks_and_truncated_passes<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    store
        .transact(|u| {
            u.keep_message(&message(
                "e1", "C", "10.5", "10.5", "U", "a", "socket", 11.0, "[]",
            ))?;
            u.keep_message(&message(
                "e2", "C", "20.5", "10.5", "U", "b", "socket", 21.0, "[]",
            ))?;
            u.keep_message(&message(
                "e3", "D", "30.5", "30.5", "U", "c", "socket", 31.0, "[]",
            ))?;
            u.open_thread("T:C:1", "T", "C", "1", 5.0)?;
            u.open_thread("T:C:2", "T", "C", "2", 50.0)?;
            u.open_thread("T:D:3", "T", "D", "3", 60.0)
        })
        .await
        .unwrap();
    let (before, latest, earlier, none, roots, runs) = store
        .transact(|u| {
            Ok((
                u.catchup_mark("T", "C")?,
                u.latest_message_ts("T", "C", None)?,
                u.latest_message_ts("T", "C", Some(21.0))?,
                u.latest_message_ts("T", "E", None)?,
                u.recent_thread_roots("T", "C", 10.0)?,
                u.truncated_passes("T", "C")?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(
        (before, latest, earlier, none),
        (None, Some(20.5), Some(10.5), None)
    );
    assert_eq!(roots, ["2"]);
    assert_eq!(runs, None);
    let (mark, runs) = store
        .transact(|u| {
            u.keep_watermark(&Watermark {
                workspace: "T".into(),
                channel: "C".into(),
                mark: 7.25,
                pinned: true,
                truncated_passes: 1,
            })?;
            Ok((u.catchup_mark("T", "C")?, u.truncated_passes("T", "C")?))
        })
        .await
        .unwrap();
    assert_eq!((mark, runs.as_deref()), (Some(7.25), Some("1")));
}

/// Own uploads are those sent with the file's ID, or still unsent to the same
/// thread under the same name.
pub async fn file_lookups_find_own_uploads_and_thread_attachments<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    store
        .transact(|u| {
            let upload = |key: &str, kind: &str, name: &str| {
                let mut post = session_post(key, "T:C:1.0", kind, "", "");
                post.filename = name.into();
                post.blob = Some(vec![1]);
                post
            };
            let sent = u.queue_post(&upload("a", "upload", "plot.png"), 1.0)?;
            u.queue_post(&upload("b", "upload", "table.csv"), 1.0)?;
            u.queue_post(&upload("c", "reply", "other.txt"), 1.0)?;
            u.claim_post(1.0, Some(sent))?.expect("the upload is ready");
            u.confirm_post(sent, "F1", 1.0)?;
            u.keep_message(&message(
                "e1",
                "C",
                "1.0",
                "1.0",
                "U",
                "a",
                "socket",
                1.0,
                r#"[{"id":"F9"}]"#,
            ))?;
            u.keep_message(&message(
                "e2", "C", "2.0", "2.0", "U", "b", "socket", 2.0, "[]",
            ))
        })
        .await
        .unwrap();
    let (own, files, none) = store
        .transact(|u| {
            let files = [
                ("F1", "anything"),
                ("F2", "table.csv"),
                ("F3", "other.txt"),
                ("F4", "plot.png"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string()));
            Ok((
                u.own_uploads("C", "1.0", &files)?,
                u.session_attachments("T:C:1.0")?,
                u.session_attachments("T:C:9.0")?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(own, ["F1", "F2"]);
    assert_eq!(files, [r#"[{"id":"F9"}]"#]);
    assert!(none.is_empty());
}

/// An interrupt cancels only that worker's pending approvals and is audited;
/// instructions on a fresh session drop the backend session.
pub async fn the_supervisor_interrupts_and_fingerprints_workers<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    store
        .transact(|u| {
            u.open_thread("T:C:1", "T", "C", "1", 1.0)?;
            u.add_workers(
                &serde_json::from_value::<Vec<_>>(serde_json::json!([
                    {"id":"w1","session_id":"T:C:1","machine":"m","workspace":"/w",
                     "backend":"claude","backend_session_id":"s1"},
                    {"id":"w2","session_id":"T:C:1","machine":"m","workspace":"/w",
                     "backend":"claude","backend_session_id":"s2"}
                ]))?,
                1.0,
            )?;
            u.queue_jobs(
                &serde_json::from_value::<Vec<_>>(serde_json::json!([
                    {"id":"j1","worker_id":"w1","session_id":"T:C:1","brief":"one"},
                    {"id":"j2","worker_id":"w2","session_id":"T:C:1","brief":"two"}
                ]))?,
                2.0,
            )
        })
        .await
        .unwrap();
    let mut approvals = vec![];
    for (id, job, slot) in [("a1", "j1", 1), ("a2", "j2", 2)] {
        let claimed = claim(&store, job, slot).await;
        approvals.push(NewApproval {
            id: id.into(),
            worker: claimed.worker,
            job: claimed.job,
            request: serde_json::from_value(serde_json::json!({
                "backend_request_id":"b","kind":"tool","summary":"x","detail":{}
            }))
            .unwrap(),
            automatic: None,
            now: 4.0,
            expires_at: 100.0,
        });
    }
    store
        .transact(move |u| {
            for request in &approvals {
                u.begin_approval(request)?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let fingerprints = store
        .transact(|u| {
            u.interrupt_worker("w1", 9.0)?;
            let before = u.instructions_fingerprint("w1")?;
            u.begin_instructions("w1", "f1", false)?;
            let kept = u.instructions_fingerprint("w1")?;
            u.begin_instructions("w1", "f2", true)?;
            Ok((before, kept, u.instructions_fingerprint("w1")?))
        })
        .await
        .unwrap();
    assert_eq!(fingerprints, (None, Some("f1".into()), Some("f2".into())));
    let (a1, a2, w1, w2, activity) = store
        .transact(|u| {
            Ok((
                u.approval("a1")?.unwrap(),
                u.approval("a2")?.unwrap(),
                u.worker_record("w1")?,
                u.worker_record("w2")?,
                u.activity(10)?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(
        (a1.status.as_str(), a1.decided_by.as_str(), a1.decided_at),
        ("cancelled", "system", 9.0)
    );
    assert_eq!(a2.status, "pending");
    assert_eq!(w1.backend_session_id, "");
    assert_eq!(w2.backend_session_id, "s2");
    let interrupts = activity
        .iter()
        .filter(|row| column(row, "action") == text("worker.interrupt"))
        .map(|row| (column(row, "actor"), column(row, "target")))
        .collect::<Vec<_>>();
    assert_eq!(interrupts, [(text("owner"), text("w1"))]);
}
