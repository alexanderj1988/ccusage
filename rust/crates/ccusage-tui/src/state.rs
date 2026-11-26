use std::collections::HashSet;

use ccusage_core::tree::{FlatRow, TreeOrder, UsageTreeNode, flatten_tree, sort_nodes};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::TableState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Continue,
    Quit,
}

#[derive(Debug)]
pub struct TreeViewState {
    expanded: HashSet<String>,
    selected_key: Option<String>,
    order: TreeOrder,
    pub(crate) table: TableState,
}

impl TreeViewState {
    pub fn new(roots: &[UsageTreeNode], expand_all: bool, order: TreeOrder) -> Self {
        let mut state = Self {
            expanded: HashSet::new(),
            selected_key: roots.first().map(|root| root.key.clone()),
            order,
            table: TableState::default(),
        };
        if expand_all {
            state.expand_all(roots);
        }
        state
    }

    pub fn order(&self) -> TreeOrder {
        self.order
    }

    pub fn visible_rows<'a>(&self, roots: &'a [UsageTreeNode]) -> Vec<FlatRow<'a>> {
        flatten_tree(roots, &|key| self.expanded.contains(key))
    }

    pub fn selected_index(&self, rows: &[FlatRow<'_>]) -> usize {
        self.selected_key
            .as_deref()
            .and_then(|key| rows.iter().position(|row| row.key_path == key))
            .unwrap_or(0)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "crossterm KeyCode is non-exhaustive; unmapped keys are ignored"
    )]
    pub fn handle_key(
        &mut self,
        roots: &mut [UsageTreeNode],
        key: KeyEvent,
        page: usize,
    ) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::Continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Action::Quit;
        }
        let page = page.max(1);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                self.select(roots, |current, _| current.saturating_sub(1));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select(roots, |current, last| (current + 1).min(last));
            }
            KeyCode::PageUp => self.select(roots, |current, _| current.saturating_sub(page)),
            KeyCode::PageDown => {
                self.select(roots, |current, last| {
                    current.saturating_add(page).min(last)
                });
            }
            KeyCode::Home | KeyCode::Char('g') => self.select(roots, |_, _| 0),
            KeyCode::End | KeyCode::Char('G') => self.select(roots, |_, last| last),
            KeyCode::Right | KeyCode::Char('l') => self.expand_selected(roots),
            KeyCode::Left | KeyCode::Char('h') => self.collapse_or_parent(roots),
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected(roots),
            KeyCode::Char('e') => self.expand_all(roots),
            KeyCode::Char('c') => self.collapse_all(),
            KeyCode::Char('s') => self.toggle_order(roots),
            _ => {}
        }
        Action::Continue
    }

    fn select(&mut self, roots: &[UsageTreeNode], next: impl FnOnce(usize, usize) -> usize) {
        let rows = self.visible_rows(roots);
        let Some(last) = rows.len().checked_sub(1) else {
            return;
        };
        let index = next(self.selected_index(&rows), last).min(last);
        self.selected_key = Some(rows[index].key_path.clone());
    }

    fn selected_row<'a>(&self, rows: &'a [FlatRow<'a>]) -> Option<&'a FlatRow<'a>> {
        rows.get(self.selected_index(rows))
    }

    fn expand_selected(&mut self, roots: &[UsageTreeNode]) {
        let rows = self.visible_rows(roots);
        if let Some(row) = self.selected_row(&rows).filter(|row| row.has_children()) {
            self.expanded.insert(row.key_path.clone());
        }
    }

    fn collapse_or_parent(&mut self, roots: &[UsageTreeNode]) {
        let rows = self.visible_rows(roots);
        let Some(row) = self.selected_row(&rows) else {
            return;
        };
        if row.expanded {
            self.expanded.remove(&row.key_path);
        } else if let Some(parent) = &row.parent_key_path {
            self.selected_key = Some(parent.clone());
        }
    }

    fn toggle_selected(&mut self, roots: &[UsageTreeNode]) {
        let rows = self.visible_rows(roots);
        if let Some(row) = self.selected_row(&rows).filter(|row| row.has_children())
            && !self.expanded.remove(&row.key_path)
        {
            self.expanded.insert(row.key_path.clone());
        }
    }

    fn expand_all(&mut self, roots: &[UsageTreeNode]) {
        self.expanded = flatten_tree(roots, &|_| true)
            .into_iter()
            .filter(|row| row.has_children())
            .map(|row| row.key_path)
            .collect();
    }

    fn collapse_all(&mut self) {
        self.expanded.clear();
        self.selected_key = self
            .selected_key
            .as_deref()
            .and_then(|key| key.split('/').next())
            .map(str::to_string);
    }

    fn toggle_order(&mut self, roots: &mut [UsageTreeNode]) {
        self.order = match self.order {
            TreeOrder::Chronological => TreeOrder::CostDesc,
            TreeOrder::CostDesc => TreeOrder::Chronological,
        };
        sort_nodes(roots, self.order);
    }
}

#[cfg(test)]
mod tests;
