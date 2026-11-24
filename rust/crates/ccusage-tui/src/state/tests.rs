use ccusage_core::{
    TimestampMs, UsageSummary,
    tree::{TreeNodeKind, TreeOrder, UsageTreeNode, sort_nodes},
};

use super::*;
use crate::{KeyCode, KeyEvent, KeyModifiers};

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

fn roots() -> Vec<UsageTreeNode> {
    vec![
        node(
            TreeNodeKind::Session,
            "s1",
            7.0,
            0,
            vec![
                node(TreeNodeKind::Main, "main", 3.0, 0, Vec::new()),
                node(
                    TreeNodeKind::Subagent,
                    "agent-a",
                    1.0,
                    10,
                    vec![node(TreeNodeKind::Subagent, "agent-c", 0.5, 11, Vec::new())],
                ),
                node(TreeNodeKind::Subagent, "agent-b", 3.0, 20, Vec::new()),
            ],
        ),
        node(
            TreeNodeKind::Session,
            "s2",
            1.0,
            5,
            vec![node(TreeNodeKind::Main, "main", 1.0, 5, Vec::new())],
        ),
    ]
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn press(state: &mut TreeViewState, roots: &mut [UsageTreeNode], codes: &[KeyCode]) {
    for code in codes {
        assert_eq!(state.handle_key(roots, key(*code), 10), Action::Continue);
    }
}

fn visible(state: &TreeViewState, roots: &[UsageTreeNode]) -> Vec<String> {
    state
        .visible_rows(roots)
        .into_iter()
        .map(|row| row.key_path)
        .collect()
}

fn selected(state: &TreeViewState, roots: &[UsageTreeNode]) -> String {
    let rows = state.visible_rows(roots);
    rows[state.selected_index(&rows)].key_path.clone()
}

#[test]
fn starts_collapsed_on_first_session() {
    let roots = roots();
    let state = TreeViewState::new(&roots, false, TreeOrder::CostDesc);
    assert_eq!(visible(&state, &roots), ["s1", "s2"]);
    assert_eq!(selected(&state, &roots), "s1");
}

#[test]
fn right_expands_left_collapses_or_jumps_to_parent() {
    let mut roots = roots();
    let mut state = TreeViewState::new(&roots, false, TreeOrder::Chronological);
    press(
        &mut state,
        &mut roots,
        &[KeyCode::Right, KeyCode::Down, KeyCode::Down],
    );
    assert_eq!(selected(&state, &roots), "s1/agent-a");
    press(&mut state, &mut roots, &[KeyCode::Char('l')]);
    assert_eq!(
        visible(&state, &roots),
        [
            "s1",
            "s1/main",
            "s1/agent-a",
            "s1/agent-a/agent-c",
            "s1/agent-b",
            "s2"
        ]
    );
    press(&mut state, &mut roots, &[KeyCode::Down, KeyCode::Left]);
    assert_eq!(selected(&state, &roots), "s1/agent-a");
    press(&mut state, &mut roots, &[KeyCode::Left]);
    assert_eq!(
        visible(&state, &roots),
        ["s1", "s1/main", "s1/agent-a", "s1/agent-b", "s2"]
    );
}

#[test]
fn enter_toggles_and_expand_collapse_all() {
    let mut roots = roots();
    let mut state = TreeViewState::new(&roots, false, TreeOrder::Chronological);
    press(&mut state, &mut roots, &[KeyCode::Enter]);
    assert_eq!(visible(&state, &roots).len(), 5);
    press(&mut state, &mut roots, &[KeyCode::Char(' ')]);
    assert_eq!(visible(&state, &roots).len(), 2);
    press(&mut state, &mut roots, &[KeyCode::Char('e')]);
    assert_eq!(visible(&state, &roots).len(), 7);
    press(
        &mut state,
        &mut roots,
        &[KeyCode::Char('G'), KeyCode::Char('c')],
    );
    assert_eq!(visible(&state, &roots), ["s1", "s2"]);
    assert_eq!(selected(&state, &roots), "s2");
}

#[test]
fn moves_clamp_and_page() {
    let mut roots = roots();
    let mut state = TreeViewState::new(&roots, true, TreeOrder::Chronological);
    press(&mut state, &mut roots, &[KeyCode::Up]);
    assert_eq!(selected(&state, &roots), "s1");
    press(&mut state, &mut roots, &[KeyCode::PageDown]);
    assert_eq!(selected(&state, &roots), "s2/main");
    press(&mut state, &mut roots, &[KeyCode::Char('g')]);
    assert_eq!(selected(&state, &roots), "s1");
    press(
        &mut state,
        &mut roots,
        &[KeyCode::Char('j'), KeyCode::Char('k')],
    );
    assert_eq!(selected(&state, &roots), "s1");
}

#[test]
fn sort_toggle_keeps_selection_and_reorders() {
    let mut roots = roots();
    sort_nodes(&mut roots, TreeOrder::Chronological);
    let mut state = TreeViewState::new(&roots, true, TreeOrder::Chronological);
    press(&mut state, &mut roots, &[KeyCode::Down, KeyCode::Down]);
    assert_eq!(selected(&state, &roots), "s1/agent-a");
    press(&mut state, &mut roots, &[KeyCode::Char('s')]);
    assert_eq!(state.order(), TreeOrder::CostDesc);
    assert_eq!(selected(&state, &roots), "s1/agent-a");
    assert_eq!(
        visible(&state, &roots)[..5],
        [
            "s1",
            "s1/main",
            "s1/agent-b",
            "s1/agent-a",
            "s1/agent-a/agent-c"
        ]
    );
}

#[test]
fn quit_keys() {
    let mut roots = roots();
    for event in [
        key(KeyCode::Char('q')),
        key(KeyCode::Esc),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        let mut state = TreeViewState::new(&roots, false, TreeOrder::CostDesc);
        assert_eq!(state.handle_key(&mut roots, event, 10), Action::Quit);
    }
}

#[test]
fn initial_views_set_order_and_expansion() {
    let mut overview = roots();
    let state = crate::initial_state(&mut overview, crate::InitialView::Overview);
    assert_eq!(state.order(), TreeOrder::CostDesc);
    assert_eq!(visible(&state, &overview), ["s1", "s2"]);

    let mut single = roots();
    let state = crate::initial_state(&mut single, crate::InitialView::Single);
    assert_eq!(state.order(), TreeOrder::Chronological);
    assert_eq!(visible(&state, &single).len(), 7);
}
