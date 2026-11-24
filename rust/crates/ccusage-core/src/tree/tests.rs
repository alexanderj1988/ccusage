use super::*;
use crate::{TimestampMs, UsageSummary};

fn node(
    kind: TreeNodeKind,
    key: &str,
    cost: f64,
    millis: i64,
    children: Vec<UsageTreeNode>,
) -> UsageTreeNode {
    UsageTreeNode {
        kind,
        key: key.to_string(),
        id: None,
        label: key.to_string(),
        agent_type: String::new(),
        summary: UsageSummary {
            total_cost: cost,
            ..UsageSummary::default()
        },
        first_timestamp: TimestampMs::from_millis(millis),
        share: 0.0,
        children,
    }
}

fn sample() -> Vec<UsageTreeNode> {
    vec![node(
        TreeNodeKind::Session,
        "s1",
        10.0,
        0,
        vec![
            node(TreeNodeKind::Subagent, "agent-b", 2.0, 30, Vec::new()),
            node(
                TreeNodeKind::Subagent,
                "agent-a",
                5.0,
                20,
                vec![
                    node(TreeNodeKind::Own, "own", 4.0, 20, Vec::new()),
                    node(TreeNodeKind::Subagent, "agent-c", 1.0, 25, Vec::new()),
                ],
            ),
            node(TreeNodeKind::Main, "main", 3.0, 10, Vec::new()),
        ],
    )]
}

fn keys<'a>(rows: &'a [FlatRow<'_>]) -> Vec<&'a str> {
    rows.iter().map(|row| row.key_path.as_str()).collect()
}

#[test]
fn chronological_order_puts_main_first_then_first_activity() {
    let mut roots = sample();
    sort_nodes(&mut roots, TreeOrder::Chronological);
    let rows = flatten_tree(&roots, &|_| true);
    assert_eq!(
        keys(&rows),
        [
            "s1",
            "s1/main",
            "s1/agent-a",
            "s1/agent-a/own",
            "s1/agent-a/agent-c",
            "s1/agent-b"
        ]
    );
}

#[test]
fn cost_order_sorts_every_level_by_cost_descending() {
    let mut roots = sample();
    sort_nodes(&mut roots, TreeOrder::CostDesc);
    let rows = flatten_tree(&roots, &|_| true);
    assert_eq!(
        keys(&rows),
        [
            "s1",
            "s1/agent-a",
            "s1/agent-a/own",
            "s1/agent-a/agent-c",
            "s1/main",
            "s1/agent-b"
        ]
    );
}

#[test]
fn flatten_draws_tree_prefixes_and_skips_collapsed_children() {
    let mut roots = sample();
    sort_nodes(&mut roots, TreeOrder::Chronological);
    let rows = flatten_tree(&roots, &|key| key == "s1" || key == "s1/agent-a");
    let prefixes: Vec<&str> = rows.iter().map(|row| row.prefix.as_str()).collect();
    assert_eq!(prefixes, ["", "├─ ", "├─ ", "│  ├─ ", "│  └─ ", "└─ "]);
    assert_eq!(rows[2].parent_key_path.as_deref(), Some("s1"));
    assert!(rows[2].expanded);

    let collapsed = flatten_tree(&roots, &|key| key == "s1");
    assert_eq!(
        keys(&collapsed),
        ["s1", "s1/main", "s1/agent-a", "s1/agent-b"]
    );
    assert!(!collapsed[2].expanded);
    assert!(collapsed[2].has_children());
}

#[test]
fn shares_are_relative_to_parent_and_roots_to_grand_total() {
    let mut roots = sample();
    roots.push(node(TreeNodeKind::Session, "s2", 30.0, 5, Vec::new()));
    assign_shares(&mut roots);
    assert!((roots[0].share - 0.25).abs() < 1e-9);
    assert!((roots[1].share - 0.75).abs() < 1e-9);
    let agent_a = &roots[0].children[1];
    assert!((agent_a.share - 0.5).abs() < 1e-9);
    assert!((agent_a.children[0].share - 0.8).abs() < 1e-9);
}

#[test]
fn zero_cost_parent_gives_zero_share() {
    let mut roots = vec![node(
        TreeNodeKind::Session,
        "s",
        0.0,
        0,
        vec![node(TreeNodeKind::Main, "main", 0.0, 0, Vec::new())],
    )];
    assign_shares(&mut roots);
    assert_eq!(roots[0].children[0].share.to_bits(), 0.0_f64.to_bits());
}

fn with_models(mut node: UsageTreeNode, models: &[(&str, f64)]) -> UsageTreeNode {
    node.summary.models_used = models.iter().map(|(name, _)| (*name).to_string()).collect();
    node.summary.model_breakdowns = models
        .iter()
        .map(|(name, cost)| crate::ModelBreakdown {
            model_name: (*name).to_string(),
            input_tokens: 10,
            cost: *cost,
            ..crate::ModelBreakdown::default()
        })
        .collect();
    node
}

#[test]
fn model_rows_split_multi_model_leaves_by_cost() {
    let main = with_models(
        node(TreeNodeKind::Main, "main", 3.0, 10, Vec::new()),
        &[("claude-sonnet-4-6", 1.0), ("claude-opus-4-7", 2.0)],
    );
    let single = with_models(
        node(TreeNodeKind::Subagent, "agent-a", 1.0, 20, Vec::new()),
        &[("claude-haiku-4-5", 1.0)],
    );
    let session = with_models(
        node(TreeNodeKind::Session, "s1", 4.0, 0, vec![main, single]),
        &[
            ("claude-opus-4-7", 2.0),
            ("claude-sonnet-4-6", 1.0),
            ("claude-haiku-4-5", 1.0),
        ],
    );
    let mut roots = vec![session];
    add_model_rows(&mut roots);
    assign_shares(&mut roots);

    let rows = flatten_tree(&roots, &|_| true);
    assert_eq!(
        keys(&rows),
        [
            "s1",
            "s1/main",
            "s1/main/model:claude-opus-4-7",
            "s1/main/model:claude-sonnet-4-6",
            "s1/agent-a",
        ]
    );
    let opus = rows[2].node;
    assert_eq!(opus.kind, TreeNodeKind::Model);
    assert_eq!(opus.label, "opus-4-7");
    assert_eq!(opus.summary.input_tokens, 10);
    assert!((opus.share - 2.0 / 3.0).abs() < 1e-9);
    assert!(roots[0].children[1].children.is_empty());
}

#[test]
fn model_label_names_one_model_or_counts_several() {
    let single = with_models(
        node(TreeNodeKind::Subagent, "a", 1.0, 0, Vec::new()),
        &[("claude-haiku-4-5", 1.0)],
    );
    assert_eq!(single.model_label(), "haiku-4-5");
    let several = with_models(
        node(TreeNodeKind::Main, "m", 1.0, 0, Vec::new()),
        &[("claude-opus-4-7", 0.5), ("claude-sonnet-4-6", 0.5)],
    );
    assert_eq!(several.model_label(), "2 models");
    let parent = with_models(
        node(TreeNodeKind::Session, "s", 1.0, 0, vec![single.clone()]),
        &[("claude-haiku-4-5", 1.0)],
    );
    assert_eq!(parent.model_label(), "haiku-4-5");
}
