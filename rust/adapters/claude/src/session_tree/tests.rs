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
    });
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
