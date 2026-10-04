//! Tolerant, bounded WorkerResult parsing compatible with the frozen contract.
use crate::worker::*;
use serde_json::Value;
pub const SUMMARY_LIMIT: usize = 1500;
pub const REPORT_LIMIT: usize = 4000;
pub const FORMAT_NOTE: &str = include_str!("result-format.txt");
/// The JSON schema of a `WorkerResult`.
pub const SCHEMA_JSON: &str = include_str!("result-schema.json");
pub const SUMMARIZE_PROMPT: &str="Return only the WorkerResult JSON object for the work you just did, in one fenced ```json block, with no other text.";
pub fn schema() -> Value {
    serde_json::from_str(SCHEMA_JSON).expect("embedded worker schema")
}
/// The bare `annotations` entry of the schema and format note.
const ANNOTATIONS: &str = r#""annotations":{"type":"object"}"#;
/// `annotations` caps: at most 32 top-level keys and 8 KiB of compact JSON, so
/// a host's fields stay small beside the 4000-character report.
pub const ANNOTATIONS_KEYS: usize = 32;
pub const ANNOTATIONS_BYTES: usize = 8192;
fn annotations(sub: &Value) -> Value {
    let mut sub = sub.clone();
    if let Some(o) = sub.as_object_mut() {
        o.entry("type").or_insert("object".into());
    }
    sub
}
/// The worker schema with the host's `annotations` sub-schema, if any, in
/// place of the bare object; core never reads the fields it asks for.
pub fn schema_with(annotations_schema: Option<&Value>) -> Value {
    let mut s = schema();
    if let Some(sub) = annotations_schema {
        s["properties"]["annotations"] = annotations(sub);
    }
    s
}
/// `FORMAT_NOTE` with the same sub-schema, for one delegate's prompt.
pub fn format_note(annotations_schema: Option<&Value>) -> String {
    match annotations_schema {
        Some(sub) => FORMAT_NOTE.replace(
            ANNOTATIONS,
            &format!(r#""annotations":{}"#, annotations(sub)),
        ),
        None => FORMAT_NOTE.into(),
    }
}
fn text(v: &Value, limit: usize) -> String {
    v.as_str().unwrap_or("").chars().take(limit).collect()
}
fn items<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v.is_object())
        .take(30)
}
pub fn coerce(v: &Value) -> Option<WorkerResult> {
    let status = v.get("status")?.as_str()?;
    let summary = v.get("summary")?.as_str()?;
    if !["done", "partial", "failed", "needs_input"].contains(&status) || strip(summary).is_empty()
    {
        return None;
    }
    Some(WorkerResult {
        status: status.into(),
        summary: summary.chars().take(SUMMARY_LIMIT).collect(),
        changes: items(v, "changes")
            .filter(|i| i["path"].as_str().is_some_and(|s| !s.is_empty()))
            .map(|i| Change {
                path: text(&i["path"], 500),
                change: i["change"]
                    .as_str()
                    .filter(|s| ["added", "deleted", "modified"].contains(s))
                    .unwrap_or("modified")
                    .into(),
                note: text(&i["note"], 500),
            })
            .collect(),
        validation: items(v, "validation")
            .filter(|i| i["command"].as_str().is_some_and(|s| !s.is_empty()))
            .map(|i| Validation {
                command: text(&i["command"], 500),
                outcome: i["outcome"]
                    .as_str()
                    .filter(|s| ["passed", "failed", "skipped"].contains(s))
                    .unwrap_or("skipped")
                    .into(),
                detail: text(&i["detail"], 300),
            })
            .collect(),
        artifacts: items(v, "artifacts")
            .filter(|i| {
                i["path"].as_str().is_some_and(|s| !s.is_empty())
                    && i["kind"]
                        .as_str()
                        .is_some_and(|s| ["png", "pdf", "md"].contains(&s))
            })
            .take(3)
            .map(|i| ArtifactRef {
                path: text(&i["path"], 1000),
                kind: i["kind"].as_str().unwrap().into(),
                caption: text(&i["caption"], 300),
            })
            .collect(),
        machine_state: MachineState {
            branch: text(&v["machine_state"]["branch"], 200),
            commit: text(&v["machine_state"]["commit"], 64),
            dirty: v["machine_state"]["dirty"] == true,
            notes: text(&v["machine_state"]["notes"], 1000),
        },
        unresolved: v["unresolved"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v.is_string())
            .take(30)
            .map(|v| text(v, 500))
            .collect(),
        question: text(&v["question"], 2000),
        report: text(&v["report"], REPORT_LIMIT),
        // Kept or dropped whole: a non-object or one over the caps is dropped.
        annotations: v["annotations"]
            .as_object()
            .filter(|m| {
                m.len() <= ANNOTATIONS_KEYS
                    && serde_json::to_string(m).is_ok_and(|s| s.len() <= ANNOTATIONS_BYTES)
            })
            .cloned(),
    })
}
struct Fence {
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
}
fn fences(text: &str) -> Vec<Fence> {
    let lines: Vec<_> = text
        .split_inclusive('\n')
        .scan(0, |at, line| {
            let start = *at;
            *at += line.len();
            Some((start, line))
        })
        .collect();
    let mut matches = vec![];
    let mut i = 0;
    while i < lines.len() {
        let (start, line) = lines[i];
        let content = line.strip_suffix('\n').unwrap_or(line);
        let opener = content.strip_prefix("```").is_some_and(|info| {
            let label = info.trim_end_matches([' ', '\t']);
            label
                .chars()
                .all(|c| c.is_alphanumeric() || ['_', '+', '-'].contains(&c))
        });
        if opener && line.ends_with('\n') {
            if let Some(j) = (i + 1..lines.len()).find(|j| {
                lines[*j]
                    .1
                    .strip_suffix('\n')
                    .unwrap_or(lines[*j].1)
                    .trim_end_matches([' ', '\t'])
                    == "```"
            }) {
                // Python's fence requires a newline before the closing fence.
                if j > i + 1 {
                    matches.push(Fence {
                        start,
                        end: lines[j].0 + lines[j].1.trim_end_matches('\n').len(),
                        body_start: start + line.len(),
                        body_end: lines[j].0 - 1,
                    });
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    matches
}
pub fn parse(text: &str) -> Option<WorkerResult> {
    let blocks = fences(text);
    let mut candidates = vec![strip(text)];
    candidates.extend(
        blocks
            .iter()
            .rev()
            .map(|f| strip(&text[f.body_start..f.body_end])),
    );
    if let Some(start) = text.rfind("\n{") {
        candidates.push(strip(&text[start..]));
    }
    candidates
        .into_iter()
        .find_map(|s| serde_json::from_str(s).ok().and_then(|v| coerce(&v)))
}
pub fn prose(text: &str) -> String {
    for f in fences(text).iter().rev() {
        if serde_json::from_str(&text[f.body_start..f.body_end])
            .ok()
            .and_then(|v| coerce(&v))
            .is_some()
        {
            return strip(&format!("{}{}", &text[..f.start], &text[f.end..])).into();
        }
    }
    strip(text).into()
}
pub fn fallback(text: &str) -> WorkerResult {
    let body = prose(text);
    let body = if body.is_empty() {
        "The worker finished without a report."
    } else {
        &body
    };
    let tail = |n| {
        body.chars()
            .skip(body.chars().count().saturating_sub(n))
            .collect::<String>()
    };
    coerce(&serde_json::json!({"status":"partial","summary":tail(SUMMARY_LIMIT),"report":tail(REPORT_LIMIT)})).unwrap()
}

fn strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
