use ccusage_core::{
    ModelBreakdown, TimestampMs, UsageSummary,
    tree::{TreeNodeKind, TreeOrder, UsageTreeNode, assign_shares},
};
use ratatui::{Terminal, backend::TestBackend};

use super::render;
use crate::{
    TreeReport, TreeViewState,
    style::{Family, dominant_family},
};

fn summary(models: &[(&str, f64)]) -> UsageSummary {
    UsageSummary {
        input_tokens: 1_000,
        output_tokens: 200,
        cache_creation_tokens: 3_000,
        cache_read_tokens: 40_000,
        total_cost: models.iter().map(|(_, cost)| cost).sum(),
        models_used: models.iter().map(|(name, _)| (*name).to_string()).collect(),
        model_breakdowns: models
            .iter()
            .map(|(name, cost)| ModelBreakdown {
                model_name: (*name).to_string(),
                cost: *cost,
                ..ModelBreakdown::default()
            })
            .collect(),
        ..UsageSummary::default()
    }
}

fn node(
    kind: TreeNodeKind,
    key: &str,
    label: &str,
    agent: &str,
    models: &[(&str, f64)],
    children: Vec<UsageTreeNode>,
) -> UsageTreeNode {
    UsageTreeNode {
        kind,
        key: key.to_string(),
        id: None,
        label: label.to_string(),
        agent_type: agent.to_string(),
        summary: summary(models),
        first_timestamp: TimestampMs::from_millis(0),
        share: 0.0,
        children,
    }
}

fn report() -> TreeReport {
    let mut roots = vec![node(
        TreeNodeKind::Session,
        "s1",
        "feat: cursor pagination",
        "Claude · main",
        &[("claude-opus-4-7", 3.09), ("claude-sonnet-4-6", 1.94)],
        vec![
            node(
                TreeNodeKind::Main,
                "main",
                "main thread",
                "main",
                &[("claude-opus-4-7", 3.09)],
                Vec::new(),
            ),
            node(
                TreeNodeKind::Subagent,
                "agent-a",
                "Task 2 · write tests",
                "general-purpose",
                &[("claude-sonnet-4-6", 1.94)],
                Vec::new(),
            ),
        ],
    )];
    assign_shares(&mut roots);
    TreeReport {
        title: "Claude Code Usage Report - Sessions".to_string(),
        subtitle: "Detected: Claude · 1 sessions · subagents resolved".to_string(),
        roots,
    }
}

fn draw(width: u16, height: u16, select_down: usize) -> Terminal<TestBackend> {
    let mut report = report();
    let mut state = TreeViewState::new(&report.roots, true, TreeOrder::Chronological);
    for _ in 0..select_down {
        state.handle_key(
            &mut report.roots,
            crate::KeyEvent::new(crate::KeyCode::Down, crate::KeyModifiers::NONE),
            10,
        );
    }
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            render(frame, &report, &mut state);
        })
        .unwrap();
    terminal
}

#[test]
fn wide_layout_shows_all_columns_bar_and_detail() {
    insta::assert_snapshot!("tree_wide", draw(170, 20, 2).backend());
}

#[test]
fn narrow_layout_drops_bar_cache_columns_and_detail() {
    insta::assert_snapshot!("tree_narrow", draw(58, 14, 0).backend());
}

#[test]
fn dominant_family_is_other_when_models_mix() {
    assert_eq!(
        dominant_family(&summary(&[("claude-opus-4-7", 1.0)])),
        Family::Opus
    );
    assert_eq!(
        dominant_family(&summary(&[("claude-haiku-4-5", 1.0)])),
        Family::Haiku
    );
    assert_eq!(
        dominant_family(&summary(&[
            ("claude-opus-4-7", 1.0),
            ("claude-sonnet-4-6", 1.0)
        ])),
        Family::Other
    );
}
