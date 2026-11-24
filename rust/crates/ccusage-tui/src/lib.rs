mod render;
mod state;
mod style;

use ccusage_core::tree::UsageTreeNode;

pub use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
pub use state::{Action, TreeViewState};

pub struct TreeReport {
    pub title: String,
    pub subtitle: String,
    pub roots: Vec<UsageTreeNode>,
}
