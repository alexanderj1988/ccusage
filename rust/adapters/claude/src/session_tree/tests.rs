use std::path::Path;

use ccusage_test_support::fs_fixture;

use super::source::{
    EntryOrigin, classify_origin, main_transcript_path, read_agent_meta, read_session_title,
    tool_use_ids,
};

#[test]
fn classifies_main_subagent_and_workflow_transcripts() {
    assert_eq!(
        classify_origin(Path::new("/p/proj/s1.jsonl")),
        EntryOrigin::Main
    );
    assert_eq!(
        classify_origin(Path::new("/p/proj/s1/subagents/agent-a1.jsonl")),
        EntryOrigin::Subagent {
            agent_id: "a1".to_string(),
            workflow: None
        }
    );
    assert_eq!(
        classify_origin(Path::new(
            "/p/proj/s1/subagents/workflows/wf_9/agent-a2.jsonl"
        )),
        EntryOrigin::Subagent {
            agent_id: "a2".to_string(),
            workflow: Some("wf_9".to_string())
        }
    );
    assert_eq!(
        classify_origin(Path::new("/p/proj/agent-legacy.jsonl")),
        EntryOrigin::Main
    );
}

#[test]
fn finds_main_transcript_for_subagent_files() {
    assert_eq!(
        main_transcript_path(Path::new(
            "/p/proj/s1/subagents/workflows/wf_9/agent-a2.jsonl"
        )),
        Some(Path::new("/p/proj/s1.jsonl").to_path_buf())
    );
    assert_eq!(
        main_transcript_path(Path::new("/p/proj/s1.jsonl")),
        Some(Path::new("/p/proj/s1.jsonl").to_path_buf())
    );
}

#[test]
fn reads_agent_meta_next_to_transcript() {
    let fixture = fs_fixture!({
        "s1/subagents/agent-a1.jsonl": "",
        "s1/subagents/agent-a1.meta.json": r#"{"agentType":"general-purpose","description":"Task 1 · implement","toolUseId":"toolu_1","spawnDepth":2,"model":"haiku"}"#,
        "s1/subagents/agent-bad.jsonl": "",
        "s1/subagents/agent-bad.meta.json": "{not json",
    });
    let meta = read_agent_meta(&fixture.path("s1/subagents/agent-a1.jsonl")).unwrap();
    assert_eq!(meta.agent_type.as_deref(), Some("general-purpose"));
    assert_eq!(meta.description.as_deref(), Some("Task 1 · implement"));
    assert_eq!(meta.tool_use_id.as_deref(), Some("toolu_1"));
    assert_eq!(meta.spawn_depth, Some(2));
    assert!(read_agent_meta(&fixture.path("s1/subagents/agent-bad.jsonl")).is_none());
    assert!(read_agent_meta(&fixture.path("s1/subagents/agent-missing.jsonl")).is_none());
}

#[test]
fn custom_title_wins_over_latest_ai_title() {
    let fixture = fs_fixture!({
        "with-custom.jsonl": [
            r#"{"type":"ai-title","aiTitle":"first","sessionId":"s"}"#,
            r#"{"type":"custom-title","customTitle":"renamed","sessionId":"s"}"#,
            r#"{"type":"ai-title","aiTitle":"later","sessionId":"s"}"#,
        ]
        .join("\n"),
        "ai-only.jsonl": [
            r#"{"type":"ai-title","aiTitle":"first","sessionId":"s"}"#,
            r#"{"type":"ai-title","aiTitle":"latest","sessionId":"s"}"#,
        ]
        .join("\n"),
        "none.jsonl": r#"{"type":"user","message":{"content":"mentions ai-title\" in text"}}"#,
        "blank-custom.jsonl": [
            r#"{"type":"ai-title","aiTitle":"  valid ai  ","sessionId":"s"}"#,
            r#"{"type":"custom-title","customTitle":"   ","sessionId":"s"}"#,
        ]
        .join("\n"),
        "blank-later.jsonl": [
            r#"{"type":"custom-title","customTitle":" kept ","sessionId":"s"}"#,
            r#"{"type":"custom-title","customTitle":"","sessionId":"s"}"#,
        ]
        .join("\n"),
    });
    assert_eq!(
        read_session_title(&fixture.path("blank-custom.jsonl")).as_deref(),
        Some("valid ai")
    );
    assert_eq!(
        read_session_title(&fixture.path("blank-later.jsonl")).as_deref(),
        Some("kept")
    );
    assert_eq!(
        read_session_title(&fixture.path("with-custom.jsonl")).as_deref(),
        Some("renamed")
    );
    assert_eq!(
        read_session_title(&fixture.path("ai-only.jsonl")).as_deref(),
        Some("latest")
    );
    assert_eq!(read_session_title(&fixture.path("none.jsonl")), None);
    assert_eq!(read_session_title(&fixture.path("missing.jsonl")), None);
}

#[test]
fn collects_tool_use_ids_but_not_tool_result_references() {
    let fixture = fs_fixture!({
        "agent.jsonl": [
            r#"{"message":{"content":[{"type":"tool_use","id":"toolu_A","name":"Agent","input":{}}]}}"#,
            r#"{"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_X"}]}}"#,
            r#"{"message":{"content":[{"type":"tool_use","id":"toolu_B","name":"Bash","input":{}}]}}"#,
        ]
        .join("\n"),
    });
    assert_eq!(
        tool_use_ids(&fixture.path("agent.jsonl")),
        ["toolu_A", "toolu_B"]
    );
}

use ccusage_core::{
    cli::{CostMode, ReportTimezone, SharedArgs, SortOrder},
    tree::{TreeNodeKind, UsageTreeNode},
};
use ccusage_test_support::{Environment, Fixture};

use super::build_session_trees;

pub(crate) fn tree_fixture() -> Fixture {
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

pub(crate) fn tree_shared() -> SharedArgs {
    SharedArgs {
        mode: CostMode::Display,
        timezone: Some(ReportTimezone::UTC),
        ..SharedArgs::default()
    }
}

fn shape(node: &UsageTreeNode, depth: usize, out: &mut Vec<String>) {
    out.push(format!(
        "{}{:?} {} [{}] {:.2} {:.3}",
        "  ".repeat(depth),
        node.kind,
        node.label,
        node.agent_type,
        node.summary.total_cost,
        node.share
    ));
    for child in &node.children {
        shape(child, depth + 1, out);
    }
}

#[test]
fn builds_session_tree_with_nested_subagents_workflows_and_fallbacks() {
    let fixture = tree_fixture();
    let entries = Environment::with_var("CLAUDE_CONFIG_DIR", fixture.root())
        .run(|| crate::load_entries_since(&tree_shared(), None).unwrap());
    let trees = build_session_trees(&entries, SortOrder::Desc).unwrap();
    let mut lines = Vec::new();
    for root in &trees.roots {
        shape(root, 0, &mut lines);
    }
    assert_eq!(
        lines,
        [
            "Session feat: cursor pagination [Claude · main] 6.75 0.931",
            "  Main main thread [main] 3.00 0.444",
            "  Subagent Task 1 · implement endpoint [general-purpose] 1.25 0.185",
            "    Own own turns [general-purpose] 1.00 0.800",
            "    Subagent Explore · locate handlers [Explore] 0.25 0.200",
            "  Subagent agent-a3 [unknown] 2.00 0.296",
            "  Workflow workflow wf_1 [workflow] 0.50 0.074",
            "    Subagent agent-w1 [unknown] 0.50 1.000",
            "Session bbbbbbbb [Claude · main] 0.50 0.069",
        ]
    );
    assert_eq!(trees.roots[0].kind, TreeNodeKind::Session);
    assert_eq!(trees.roots[0].id.as_deref(), Some("session-a"));
    assert_eq!(trees.warnings.len(), 2, "{:?}", trees.warnings);
}

#[test]
fn unresolvable_spawn_id_attaches_to_session() {
    let fixture = fs_fixture!({
        "projects/p/s.jsonl": r#"{"timestamp":"2026-11-24T10:00:00Z","sessionId":"s","requestId":"r0","costUSD":1.0,"message":{"id":"m0","model":"claude-opus-4-7","usage":{"input_tokens":1,"output_tokens":1}}}"#,
        "projects/p/s/subagents/agent-x.jsonl": r#"{"timestamp":"2026-11-24T10:01:00Z","sessionId":"s","isSidechain":true,"requestId":"r1","costUSD":1.0,"message":{"id":"m1","model":"claude-haiku-4-5","usage":{"input_tokens":1,"output_tokens":1}}}"#,
        "projects/p/s/subagents/agent-x.meta.json": r#"{"agentType":"Explore","description":"orphan","toolUseId":"toolu_gone","spawnDepth":3}"#,
    });
    let entries = Environment::with_var("CLAUDE_CONFIG_DIR", fixture.root())
        .run(|| crate::load_entries_since(&tree_shared(), None).unwrap());
    let trees = build_session_trees(&entries, SortOrder::Desc).unwrap();
    let labels: Vec<&str> = trees.roots[0]
        .children
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(labels, ["main thread", "orphan"]);
}
