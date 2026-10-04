//! The unit of work, the replay ledger and health events.
use super::Backend;
use crate::store::{transact, Store};

/// A unit of work that fails leaves nothing behind, and its error comes back
/// as it was raised.
pub async fn a_failed_unit_of_work_leaves_nothing_behind<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let error = store
        .transact(|u| -> anyhow::Result<()> {
            u.record("x", 1.0, "{}", true)?;
            u.note("y", "{}", 1.0)?;
            anyhow::bail!("refused")
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "refused");
    let dynamic: &dyn Store = &store;
    let (seq, health) = transact(dynamic, |u| Ok((u.last_seq()?, u.count_between(0.0, 9.0)?)))
        .await
        .unwrap();
    assert_eq!((seq, health), (0, 0));
}

/// A unit of work's result comes back, and what it wrote is there for the
/// next one, also through a `dyn Store`.
pub async fn a_unit_of_work_returns_its_result<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let (seq, text) = store
        .transact(|u| Ok((u.record("x", 1.0, "{}", true)?, String::from("kept"))))
        .await
        .unwrap();
    assert_eq!(text, "kept");
    let dynamic: &dyn Store = &store;
    assert_eq!(transact(dynamic, |u| u.last_seq()).await.unwrap(), seq);
}

/// Ledger events come back in order, complete when completed, with their
/// payloads byte for byte.
pub async fn the_ledger_appends_in_order_and_completes_calls<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let (call, result) = store
        .transact(|u| {
            assert_eq!(u.last_seq()?, 0);
            let call = u.record("x_call", 1.0, r#"{"b":1,"a":2}"#, false)?;
            let result = u.record("x_result", 2.0, "{}", true)?;
            u.complete(call, true)?;
            Ok((call, result))
        })
        .await
        .unwrap();
    assert!(result > call);
    let events = store
        .transact(move |u| u.events_after(call - 1, 10))
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    // Payloads are kept byte for byte.
    assert_eq!(
        (
            events[0].kind.as_str(),
            events[0].payload.as_str(),
            events[0].complete
        ),
        ("x_call", r#"{"b":1,"a":2}"#, true)
    );
    assert_eq!(store.transact(|u| u.last_seq()).await.unwrap(), result);
    assert_eq!(
        store
            .transact(move |u| u.events_after(result, 10))
            .await
            .unwrap(),
        vec![]
    );
    assert_eq!(
        store
            .transact(move |u| u.events_after(0, 1))
            .await
            .unwrap()
            .len(),
        1
    );
}

/// `note_unless_since` dedupes by time, `note_unless_noted` by the named
/// top-level fields; a missing field never matches, and a field must be a
/// plain name.
pub async fn health_events_are_deduplicated_by_time_or_by_detail<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let recorded = store
        .transact(|u| {
            Ok([
                u.note_unless_since("deny", "{}", 100.0, 0.0)?,
                u.note_unless_since("deny", "{}", 200.0, 50.0)?,
                u.note_unless_since("deny", "{}", 300.0, 150.0)?,
                u.note_unless_noted(
                    "skip",
                    r#"{"channel":"C","root":"1"}"#,
                    1.0,
                    &["channel", "root"],
                )?,
                u.note_unless_noted(
                    "skip",
                    r#"{"channel":"C","root":"1","code":"x"}"#,
                    2.0,
                    &["channel", "root"],
                )?,
                u.note_unless_noted(
                    "skip",
                    r#"{"channel":"C","root":"2"}"#,
                    3.0,
                    &["channel", "root"],
                )?,
                // A missing field never matches, as in SQL.
                u.note_unless_noted("drop", "{}", 4.0, &["event_id"])?,
                u.note_unless_noted("drop", "{}", 5.0, &["event_id"])?,
            ])
        })
        .await
        .unwrap();
    assert_eq!(recorded, [true, false, true, true, false, true, true, true]);
    assert_eq!(
        store
            .transact(|u| u.count_between(100.0, 300.0))
            .await
            .unwrap(),
        1
    );
    assert!(store
        .transact(|u| u.note_unless_noted("x", "{}", 1.0, &["a'b"]))
        .await
        .is_err());
}

/// Intake senders are read in order, skipping records without one; a
/// repeated delivery is an intake of the same event at the same time.
pub async fn the_ledger_finds_intake_senders_and_repeats<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let (first, senders, repeat, elsewhere, later) = store
        .transact(|u| {
            let intake = |event: &str, sender: &str| {
                format!(r#"{{"message":{{"event_id":"{event}","sender":"{sender}"}}}}"#)
            };
            let first = u.record("intake", 5.0, &intake("e1", "U1"), true)?;
            u.record("other", 5.0, r#"{"message":{"sender":"U9"}}"#, true)?;
            u.record("intake", 5.0, r#"{"message":{}}"#, true)?;
            u.record("intake", 6.0, &intake("e2", "U2"), true)?;
            let again = u.record("intake", 5.0, &intake("e1", "U1"), true)?;
            Ok((
                first,
                u.intake_senders_after(0)?,
                u.has_recent_intake("e1", again, 5.0)?,
                u.has_recent_intake("e1", again, 6.0)?,
                u.has_recent_intake("e1", first, 5.0)?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(senders, ["U1", "U2", "U1"]);
    assert!(repeat && !elsewhere && !later);
    let after = store
        .transact(move |u| u.intake_senders_after(first))
        .await
        .unwrap();
    assert_eq!(after, ["U2", "U1"]);
}

/// The latest complete attachment result for a key, byte for byte.
pub async fn the_ledger_finds_the_latest_attachment_context<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let found = store
        .transact(|u| {
            u.record(
                "parent_attachment_result",
                1.0,
                r#"{"key":"k","context":{"n":1}}"#,
                true,
            )?;
            u.record(
                "parent_attachment_result",
                2.0,
                r#"{"key":"k","context":{"n":2}}"#,
                true,
            )?;
            u.record(
                "parent_attachment_result",
                3.0,
                r#"{"key":"k","context":{"n":3}}"#,
                false,
            )?;
            u.record(
                "parent_attachment_result",
                4.0,
                r#"{"key":"j","context":{"n":4}}"#,
                true,
            )?;
            Ok((u.attachment_context("k")?, u.attachment_context("x")?))
        })
        .await
        .unwrap();
    assert_eq!(found, (Some(r#"{"n":2}"#.to_string()), None));
}

/// A GitHub pause keeps the later end.
pub async fn a_github_pause_keeps_the_later_end<B: Backend>() {
    let (_guard, store) = B::fresh().await;
    let until = store
        .transact(|u| {
            let before = u.github_paused_until()?;
            u.pause_github(20.5)?;
            u.pause_github(10.0)?;
            Ok((before, u.github_paused_until()?))
        })
        .await
        .unwrap();
    assert_eq!(until, (None, Some("20.5".to_string())));
}
