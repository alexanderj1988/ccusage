mod render;
mod state;
mod style;

use std::io::IsTerminal;

use ccusage_core::tree::{TreeOrder, UsageTreeNode, sort_nodes};
use ratatui::{DefaultTerminal, crossterm::event};

pub use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
pub use state::{Action, TreeViewState};

pub struct TreeReport {
    pub title: String,
    pub subtitle: String,
    pub roots: Vec<UsageTreeNode>,
    pub no_cost: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialView {
    Overview,
    Single,
}

pub fn interactive_terminal_available() -> bool {
    ccusage_terminal::stdout_is_terminal() && std::io::stdin().is_terminal()
}

pub(crate) fn initial_state(roots: &mut [UsageTreeNode], initial: InitialView) -> TreeViewState {
    let (order, expand_all) = match initial {
        InitialView::Overview => (TreeOrder::CostDesc, false),
        InitialView::Single => (TreeOrder::Chronological, true),
    };
    sort_nodes(roots, order);
    TreeViewState::new(roots, expand_all, order)
}

pub fn run(mut report: TreeReport, initial: InitialView) -> std::io::Result<()> {
    let mut state = initial_state(&mut report.roots, initial);
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(error) => {
            ratatui::restore();
            return Err(error);
        }
    };
    let result = event_loop(&mut terminal, &mut report, &mut state);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    report: &mut TreeReport,
    state: &mut TreeViewState,
) -> std::io::Result<()> {
    loop {
        let mut page = 1;
        terminal.draw(|frame| page = render::render(frame, report, state))?;
        if let event::Event::Key(key) = event::read()?
            && state.handle_key(&mut report.roots, key, page) == Action::Quit
        {
            return Ok(());
        }
    }
}
