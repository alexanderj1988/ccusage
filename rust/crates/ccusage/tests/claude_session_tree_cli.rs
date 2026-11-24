#![cfg(test)]

use ccusage_test_support::{Fixture, fs_fixture};
use serde_json::Value;

mod support;

fn fixture() -> Fixture {
    fs_fixture!({
        "projects/project-a/session-a.jsonl": [
            r#"{"type":"ai-title","aiTitle":"feat: cursor pagination","sessionId":"session-a"}"#,
            r#"{"timestamp":"2026-11-24T10:00:00Z","sessionId":"session-a","requestId":"req-main","costUSD":3.0,"message":{"id":"msg-main","model":"claude-opus-4-7","usage":{"input_tokens":100,"output_tokens":10}}}"#,
        ]
        .join("\n"),
        "projects/project-a/session-a/subagents/agent-a1.jsonl": r#"{"timestamp":"2026-11-24T10:05:00Z","sessionId":"session-a","isSidechain":true,"requestId":"req-a1","costUSD":1.0,"message":{"id":"msg-a1","model":"claude-haiku-4-5","content":[{"type":"tool_use","id":"toolu_nested","name":"Agent","input":{}}],"usage":{"input_tokens":50,"output_tokens":5}}}"#,
        "projects/project-a/session-a/subagents/agent-a1.meta.json": r#"{"agentType":"general-purpose","description":"Task 1 · implement endpoint","toolUseId":"toolu_main1","spawnDepth":1}"#,
        "projects/project-a/session-a/subagents/agent-a2.jsonl": r#"{"timestamp":"2026-11-24T10:06:00Z","sessionId":"session-a","isSidechain":true,"requestId":"req-a2","costUSD":0.25,"message":{"id":"msg-a2","model":"claude-haiku-4-5","usage":{"input_tokens":20,"output_tokens":2}}}"#,
        "projects/project-a/session-a/subagents/agent-a2.meta.json": r#"{"agentType":"Explore","description":"Explore · locate handlers","toolUseId":"toolu_nested","spawnDepth":2}"#,
        "projects/project-a/session-a/subagents/agent-a3.jsonl": r#"{"timestamp":"2026-11-24T10:10:00Z","sessionId":"session-a","isSidechain":true,"requestId":"req-a3","costUSD":2.0,"message":{"id":"msg-a3","model":"claude-sonnet-4-6","usage":{"input_tokens":30,"output_tokens":3}}}"#,
        "projects/project-a/session-a/subagents/workflows/wf_1/agent-w1.jsonl": r#"{"timestamp":"2026-11-24T10:20:00Z","sessionId":"session-a","isSidechain":true,"requestId":"req-w1","costUSD":0.5,"message":{"id":"msg-w1","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"output_tokens":1}}}"#,
        "projects/project-a/bbbbbbbb-1111.jsonl": r#"{"timestamp":"2026-11-24T11:00:00Z","sessionId":"bbbbbbbb-1111","requestId":"req-b","costUSD":0.5,"message":{"id":"msg-b","model":"claude-sonnet-4-6","usage":{"input_tokens":5,"output_tokens":1}}}"#,
    })
}

fn run_cli(fixture: &Fixture, args: &[&str]) -> String {
    let output = support::hermetic_command(fixture.root())
        .env("CLAUDE_CONFIG_DIR", fixture.root())
        .env("COLUMNS", "240")
        .args(args)
        .output()
        .expect("failed to run ccusage");
    assert!(
        output.status.success(),
        "ccusage {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("CLI stdout was not UTF-8")
}

const ARGS: [&str; 9] = [
    "claude",
    "session",
    "--tree",
    "--mode",
    "display",
    "--offline",
    "--no-color",
    "--timezone",
    "UTC",
];

#[test]
fn tree_json_nests_subagents_under_sessions() {
    let fixture = fixture();
    let args = ARGS.iter().copied().chain(["--json"]).collect::<Vec<_>>();
    let report: Value = serde_json::from_str(&run_cli(&fixture, &args)).unwrap();
    let session = &report["sessions"][0];
    assert_eq!(session["label"], "feat: cursor pagination");
    assert_eq!(session["sessionId"], "session-a");
    assert_eq!(session["children"][0]["kind"], "main");
    assert_eq!(
        session["children"][1]["label"],
        "Task 1 · implement endpoint"
    );
    assert_eq!(
        session["children"][1]["children"][1]["agentType"],
        "Explore"
    );
    assert_eq!(session["children"][3]["kind"], "workflow");
    assert_eq!(report["sessions"].as_array().map(Vec::len), Some(2));
    assert_eq!(report["totals"]["totalCost"], 7.25);
}

#[test]
fn tree_json_with_id_shows_only_that_session() {
    let fixture = fixture();
    let args = ARGS
        .iter()
        .copied()
        .chain(["--json", "--id", "bbbbbbbb-1111"])
        .collect::<Vec<_>>();
    let report: Value = serde_json::from_str(&run_cli(&fixture, &args)).unwrap();
    assert_eq!(report["sessions"].as_array().map(Vec::len), Some(1));
    assert_eq!(report["sessions"][0]["sessionId"], "bbbbbbbb-1111");
}

#[test]
fn tree_table_without_tty_is_static_and_fully_expanded() {
    let fixture = fixture();
    insta::assert_snapshot!("claude_session_tree_table", run_cli(&fixture, &ARGS));
}

#[test]
fn flat_session_json_is_unchanged_without_tree() {
    let fixture = fixture();
    let report: Value = serde_json::from_str(&run_cli(
        &fixture,
        &[
            "claude",
            "session",
            "--mode",
            "display",
            "--offline",
            "--json",
            "--timezone",
            "UTC",
        ],
    ))
    .unwrap();
    assert!(report["sessions"][0].get("children").is_none());
    assert_eq!(report["totals"]["totalCost"], 7.25);
}
