use serde_json::json;

use super::*;
use crate::{
    ModelBreakdown, TimestampMs, UsageSummary,
    tree::{TreeNodeKind, UsageTreeNode},
};

fn leaf(kind: TreeNodeKind, key: &str, id: Option<&str>, cost: f64) -> UsageTreeNode {
    UsageTreeNode {
        kind,
        key: key.to_string(),
        id: id.map(str::to_string),
        label: key.to_string(),
        agent_type: "main".to_string(),
        summary: UsageSummary {
            input_tokens: 10,
            output_tokens: 2,
            total_cost: cost,
            models_used: vec!["claude-opus-4-7".to_string()],
            model_breakdowns: vec![ModelBreakdown {
                model_name: "claude-opus-4-7".to_string(),
                input_tokens: 10,
                output_tokens: 2,
                cost,
                ..ModelBreakdown::default()
            }],
            ..UsageSummary::default()
        },
        first_timestamp: TimestampMs::from_millis(0),
        share: 1.0,
        children: Vec::new(),
    }
}

#[test]
fn json_nests_children_and_names_ids_by_kind() {
    let mut session = leaf(TreeNodeKind::Session, "s1", Some("s1"), 2.0);
    session.children = vec![
        leaf(TreeNodeKind::Main, "main", None, 1.5),
        leaf(TreeNodeKind::Subagent, "agent-a", Some("a"), 0.5),
    ];
    let value = tree_report_json(&[session]);
    assert_eq!(value["sessions"][0]["kind"], json!("session"));
    assert_eq!(value["sessions"][0]["sessionId"], json!("s1"));
    assert_eq!(value["sessions"][0]["children"][0]["kind"], json!("main"));
    assert!(value["sessions"][0]["children"][0].get("agentId").is_none());
    assert_eq!(value["sessions"][0]["children"][1]["agentId"], json!("a"));
    assert_eq!(value["sessions"][0]["children"][1]["children"], json!([]));
    assert_eq!(
        value["sessions"][0]["modelsUsed"],
        json!(["claude-opus-4-7"])
    );
    assert_eq!(value["totals"]["totalCost"], json!(2.0));
    assert_eq!(value["sessions"][0]["totalTokens"], json!(12));
}

#[test]
fn row_values_show_prefix_models_and_share() {
    let mut session = leaf(TreeNodeKind::Session, "s1", Some("s1"), 2.0);
    let mut child = leaf(TreeNodeKind::Subagent, "agent-a", Some("a"), 0.5);
    child.share = 0.25;
    session.children = vec![child];
    let rows = crate::tree::flatten_tree(std::slice::from_ref(&session), &|_| true);
    let values = tree_row_values(&rows[1], false, &crate::cli::SharedArgs::default());
    assert_eq!(values[0], "└─ agent-a");
    assert_eq!(values[2], "opus-4-7");
    assert_eq!(values.last().map(String::as_str), Some("25.0%"));
}
