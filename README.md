# fridica-core

Fridica's domain, without I/O: the types and pure logic that do not depend
on Slack, SQLite or worker processes.

It is the domain crate of [Fridica](https://github.com/chengcli/fridica), a
Slack agent that delegates work to Claude Code and Codex workers on your
machines; the `fridica` crate re-exports it as `fridica::core`.

- **No I/O.** Only serde, regex, sha2, uuid and unicode-casefold; `unsafe` is
  forbidden. Storage, Slack, worker processes and config loading belong to the
  host, which calls into these types and implements the adapter traits below.
- **Decisions are validated before they act.** `delegation::prepare` checks a
  parent decision against its `Scope` (whether the channel may delegate, the
  `Limits`, the machine `Registry`) and places every delegation, or names what
  to repair.
- **Placement is sticky and load-aware.** `placement` keeps a thread on its
  machine and workspace, reads the fixed load probe and assesses it.
- **Delivery outcomes are facts.** A post is `Sent`, `RateLimited`, `Rejected`
  or `Ambiguous`; `DeliveryState::after_restart` turns an interrupted `Sending`
  into `Ambiguous` instead of guessing.
- **Publication is gated.** `egress::scan` names every rule a text breaks
  (`deny_list:<line>`, `ai_trailer`) and never the matched text.
- **Attention is explicit.** `policy::legacy_gate` is the frozen v0.3 gate;
  `policy::attention_gate` is the v0.4 gate.

## Modules

- `ids`, `time`: thread IDs (`workspace:channel:root_ts`), clocks and ID sources.
- `parent`: the parent's request and decision (including hand-offs to linked
  threads of the channel), its action schema and context trimming, and the
  `Parent` adapter trait.
- `delegation`: validating a decision and placing its delegations, within a
  `Scope` (whether the channel may delegate, `Limits`, the machine `Registry`).
- `fork`: the worker's starting context as a fork of the parent's (with the
  state of the thread's linked threads): snapshot, delta and rendering, bounded
  by `limits.worker_context_chars`.
- `placement`: sticky, load-aware machine and workspace selection, and the
  fixed load probe's parser and assessment.
- `worker`, `result`, `approvals`: worker records, jobs, results and their
  format, failures (including `RateLimited`, temporary, with a reset time),
  approval requests and automatic command rules.
- `delivery`: outbox posts and delivery outcomes, and the `Delivery` trait.
- `egress`: the deny list and the scan of text about to be published.
- `policy`, `render`, `failure`: attention gates, reply rendering, blocked
  turns, and the wait after a rate-limited parent call.
- `config`: the parent, limits, placement and attention settings, and the
  machine registry. Loading them from TOML belongs to the host.

## What you supply

| Trait | Purpose |
|---|---|
| `parent::Parent` | Decide a `ParentRequest` |
| `delivery::Delivery` | Send a `ClaimedPost` and report its `DeliveryOutcome` |
| `time::Clock` | The current time (`SystemClock` and `ReplayClock` are provided) |
| `time::Identifiers` | New IDs per namespace (`RandomIds` and `SequenceIds` are provided) |

## Example

```rust,no_run
use fridica_core::egress::{scan, DenyList};

let deny = DenyList::parse("# private names\nAlice Example\nproject-\\d+\n")?;
assert_eq!(scan("see PROJECT-42", &deny), ["deny_list:3"]);
assert!(scan("ordinary text", &deny).is_empty());
# Ok::<(), anyhow::Error>(())
```

## License

MIT
