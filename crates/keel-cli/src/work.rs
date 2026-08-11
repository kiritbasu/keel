//! `keel ready`, `keel claim` and `keel close` — the three work verbs.
//!
//! # Why these go through the daemon
//!
//! Reads have to: DuckDB refuses a read-only connection while any process holds
//! the write lock, and the daemon is always running (TQ-15). Writes have to for a
//! different reason — hard constraint 1, the daemon owns the single write path.
//!
//! So all three call the daemon and fall back to the store only when nothing is
//! listening, which is the one moment opening it directly is safe. `ready` uses
//! the local API, and the two writes use the MCP endpoint, so the CLI and a model
//! are calling literally the same code rather than two implementations that agree
//! until they do not.

use anyhow::{Context, Result, bail};
use keel_core::{CloseReason, DuckStore};
use serde_json::{Value, json};
use std::path::Path;

/// Print `keel ready`.
#[allow(clippy::too_many_arguments)]
pub fn ready(
    home: &Path,
    daemon: &str,
    project: &str,
    unclaimed: bool,
    labels: &[String],
    no_labels: &[String],
    milestone: Option<&str>,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let mut args = json!({
        "project": project,
        "limit": limit,
        "surface": "cli",
    });
    if unclaimed {
        args["unclaimed"] = json!(true);
    }
    if !labels.is_empty() {
        args["labels"] = json!(labels);
    }
    if !no_labels.is_empty() {
        args["without_labels"] = json!(no_labels);
    }
    if let Some(m) = milestone {
        args["milestone"] = json!(m);
    }

    let structured = match call_daemon(daemon, "keel_ready", &args)? {
        Some(v) => v,
        None => directly(home, |store| {
            let mut s = store;
            keel_mcp::dispatch(
                &mut s,
                keel_mcp::ToolCall {
                    name: "keel_ready",
                    arguments: &args,
                },
            )
            .map_err(|e| anyhow::anyhow!("{}", e.message))
        })?,
    };

    if json_out {
        println!("{}", serde_json::to_string_pretty(&structured)?);
        return Ok(());
    }

    let items = structured
        .get("ready")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        println!("nothing ready");
        return Ok(());
    }
    for item in &items {
        let field = |k: &str| item.get(k).and_then(Value::as_str).unwrap_or("");
        println!("  {:<10} {}", field("reference"), field("title"));
        println!("             {}", field("why"));
    }

    // Hard constraint 4: a list that was cut says so, with the total. Ten of ten
    // reads exactly like ten of ninety otherwise.
    let total = structured.get("total").and_then(Value::as_u64).unwrap_or(0);
    if structured
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        println!(
            "\n{} of {total} shown — raise --limit for the rest",
            items.len()
        );
    } else {
        println!("\n{total} ready");
    }
    Ok(())
}

/// Print `keel claim`.
pub fn claim(
    home: &Path,
    daemon: &str,
    task: &str,
    force: bool,
    session: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let session = require_session(session)?;
    let mut args = json!({
        "id": task,
        "session_id": session,
        "surface": "cli",
    });
    if force {
        args["force"] = json!(true);
    }

    let structured = run_write(home, daemon, "keel_claim", &args)?;

    if json_out {
        println!("{}", serde_json::to_string_pretty(&structured)?);
        return Ok(());
    }
    let reference = structured
        .get("reference")
        .and_then(Value::as_str)
        .unwrap_or(task);
    let title = structured
        .pointer("/task/title")
        .and_then(Value::as_str)
        .unwrap_or("");
    println!("{reference} claimed — {title}");
    if let Some(previous) = structured.get("took_over_from").and_then(Value::as_str) {
        println!("  taken over from session {previous}, whose claim had gone stale");
    }
    Ok(())
}

/// Print `keel close`.
#[allow(clippy::too_many_arguments)]
pub fn close(
    home: &Path,
    daemon: &str,
    task: &str,
    reason: &str,
    message: &str,
    evidence: &[String],
    other: Option<&str>,
    session: Option<&str>,
    json_out: bool,
) -> Result<()> {
    // Parsed here rather than left to the daemon, so a typo costs no round trip
    // and the error names the five values.
    let reason = CloseReason::parse(reason).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut args = json!({
        "id": task,
        "reason": reason.as_str(),
        "message": message,
        "surface": "cli",
    });
    if !evidence.is_empty() {
        args["evidence"] = json!(evidence);
    }
    if let Some(other) = other {
        args["other"] = json!(other);
    }
    if let Some(s) = session {
        args["session_id"] = json!(s);
    }

    let structured = run_write(home, daemon, "keel_close", &args)?;

    if json_out {
        println!("{}", serde_json::to_string_pretty(&structured)?);
        return Ok(());
    }
    let reference = structured
        .get("reference")
        .and_then(Value::as_str)
        .unwrap_or(task);
    println!("{reference} closed as {reason}");
    if let Some(to) = structured.pointer("/linked/to").and_then(Value::as_str) {
        let rel = structured
            .pointer("/linked/rel")
            .and_then(Value::as_str)
            .unwrap_or("linked");
        println!("  {rel} {to}");
    }
    Ok(())
}

/// A claim has to name a session, and Keel never invents one.
fn require_session(session: Option<&str>) -> Result<String> {
    match session.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Ok(s.to_owned()),
        None => bail!(
            "a claim has to name the session doing the work, and none was given.\n\n\
             Pass `--session <id>` or set `KEEL_SESSION`. Keel never invents one: a claim \
             without it would say the task is taken and not by whom, which is worse than \
             leaving it unclaimed."
        ),
    }
}

/// Run a write through the daemon, falling back to the store when none is up.
///
/// Attribution note: on the daemon path the write is recorded as `claude`,
/// because the MCP endpoint falls back to the transport's identity and cannot
/// see who is at the other end of it. `surface: cli` is sent so the record still
/// says where it came from, which is the part that is knowable.
fn run_write(home: &Path, daemon: &str, tool: &str, args: &Value) -> Result<Value> {
    match call_daemon(daemon, tool, args)? {
        Some(v) => Ok(v),
        None => {
            tracing::debug!("no daemon listening, opening the store directly");
            directly(home, |mut store| {
                keel_mcp::dispatch(
                    &mut store,
                    keel_mcp::ToolCall {
                        name: tool,
                        arguments: args,
                    },
                )
                .map_err(|e| anyhow::anyhow!("{}", e.message))
            })
        }
    }
}

/// Open the store and run one dispatch against it.
///
/// Safe only because we got here by failing to reach a daemon, which is the one
/// condition under which nothing else holds DuckDB's write lock.
fn directly(home: &Path, f: impl FnOnce(DuckStore) -> Result<Value>) -> Result<Value> {
    let store =
        DuckStore::open(home).with_context(|| format!("open the store at {}", home.display()))?;
    f(store)
}

/// Call one MCP tool on the daemon.
///
/// `Ok(None)` means nothing is listening, which is the signal to fall back.
/// Anything else — a refusal, a validation error — is returned as an error
/// carrying what the daemon said, because that message is written to be acted
/// on rather than retried.
fn call_daemon(base: &str, tool: &str, args: &Value) -> Result<Option<Value>> {
    use keel_mcp::protocol::{
        HEADER_METHOD, HEADER_NAME, HEADER_PROTOCOL_VERSION, PROTOCOL_VERSION,
    };

    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args },
    });

    let response = match ureq::post(&format!("{base}/mcp"))
        .set(HEADER_METHOD, "tools/call")
        .set(HEADER_NAME, tool)
        .set(HEADER_PROTOCOL_VERSION, PROTOCOL_VERSION)
        .timeout(std::time::Duration::from_secs(30))
        .send_json(&body)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(text);
            bail!("the daemon at {base} refused {tool} ({code}): {message}");
        }
        // No listener. The only case worth falling back on.
        Err(_) => return Ok(None),
    };

    let envelope: Value = response
        .into_json()
        .with_context(|| format!("read the daemon's response to {tool}"))?;

    if let Some(message) = envelope.pointer("/error/message").and_then(Value::as_str) {
        bail!("{message}");
    }

    // A tool error arrives as a successful JSON-RPC response with `isError`, so
    // it has to be looked for rather than assumed away by the HTTP status.
    let result = envelope.get("result").cloned().unwrap_or(envelope);
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or("the daemon reported an error with no message");
        bail!("{text}");
    }

    Ok(Some(
        result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| result.clone()),
    ))
}
