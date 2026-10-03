//! Safe settlement for unavailable or invalid model answers. Infrastructure and
//! recording faults still propagate; no unrecorded effect is acknowledged.
use crate::parent::{Decision, ParentFailure};
use serde_json::json;
pub const UNAVAILABLE: &str =
    "I couldn't get to this just now; I'll need to look at it myself before anyone retries.";
pub const INVALID: &str = "I couldn't produce a valid action after one repair attempt. Owner review is needed before retrying.";
/// The turn's stand-in decision when the parent fails or its action stays
/// invalid after repair. Nothing is posted: the thread waits for owner review.
pub fn blocked(invalid: bool) -> Decision {
    let text = if invalid { INVALID } else { UNAVAILABLE };
    // Neither failure is posted (send: false): the thread is still marked
    // blocked, so due events do not call the model again, the requester's
    // mention stays open, and the host escalates the cause to the owner.
    let reply = json!({"send":false,"text":text,"status":"blocked"});
    serde_json::from_value(json!({"reply":reply,
        "note":{"kind":"status","blocker":text,"next_step":"Owner review before retrying"}}))
    .expect("fixed failure action")
}
/// The failure code of a parent call refused by the backend's usage limit.
/// Temporary: the turn is retried after `RATE_LIMIT_RETRY`, without blocking
/// the thread or asking for owner review.
pub const RATE_LIMITED: &str = "parent_rate_limited";
/// Seconds before a rate-limited parent turn is retried. A refused call costs
/// nothing, so a short fixed wait reaches the reset soon after it.
pub const RATE_LIMIT_RETRY: f64 = 600.;
pub fn rate_limited(failure: &ParentFailure) -> bool {
    failure.code == RATE_LIMITED
}
pub fn prevents_effects(failure: &ParentFailure) -> bool {
    matches!(
        failure.code.as_str(),
        "parent_recording_failed"
            | "parent_context_recording_failed"
            | "parent_context_snapshot_missing"
            | "parent_context_snapshot_invalid"
            | "parent_context_scope"
            | "parent_context_time"
    )
}
