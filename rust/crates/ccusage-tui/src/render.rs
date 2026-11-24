use ccusage_core::{
    format_currency, format_number, sanitize_terminal_text, short_model_name, tree::FlatRow,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::{
    TreeReport, TreeViewState,
    style::{Family, dominant_family},
};

const BAR_WIDTH: usize = 10;
const DETAIL_MIN_WIDTH: u16 = 60;
const BAR_MIN_WIDTH: u16 = 150;
const CACHE_MIN_WIDTH: u16 = 120;

#[derive(Clone, Copy)]
struct Columns {
    cache: bool,
    bar: bool,
}

pub(crate) fn render(
    frame: &mut Frame<'_>,
    report: &TreeReport,
    state: &mut TreeViewState,
) -> usize {
    let area = frame.area();
    let show_detail = area.width >= DETAIL_MIN_WIDTH;
    let [header, body, detail, footer] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(4),
        Constraint::Length(if show_detail { 6 } else { 0 }),
        Constraint::Length(1),
    ])
    .areas(area);
    let columns = Columns {
        cache: area.width >= CACHE_MIN_WIDTH,
        bar: area.width >= BAR_MIN_WIDTH,
    };
    let rows = state.visible_rows(&report.roots);
    let selected = state.selected_index(&rows);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(report.title.as_str()),
            Line::from(report.subtitle.as_str()),
        ])
        .block(Block::default().borders(Borders::ALL)),
        header,
    );
    render_table(frame, body, report, &rows, columns, state, selected);
    if show_detail && let Some(row) = rows.get(selected) {
        render_detail(frame, detail, &rows, row);
    }
    render_footer(frame, footer);
    usize::from(body.height.saturating_sub(4))
}

fn render_table(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &TreeReport,
    rows: &[FlatRow<'_>],
    columns: Columns,
    state: &mut TreeViewState,
    selected: usize,
) {
    let mut headers = vec!["Task", "Agent", "Model", "Input", "Output"];
    let mut widths = vec![
        Constraint::Fill(3),
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    if columns.cache {
        headers.extend(["Cache Create", "Cache Read", "Total Tokens"]);
        widths.extend([
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(13),
        ]);
    }
    headers.extend(["Cost (USD)", "Share"]);
    widths.extend([
        Constraint::Length(10),
        Constraint::Length(if columns.bar { 18 } else { 7 }),
    ]);

    let body = rows.iter().map(|row| table_row(row, columns));
    let total_cost: f64 = report
        .roots
        .iter()
        .map(|root| root.summary.total_cost)
        .sum();
    let mut total = vec![
        Cell::from("Total"),
        Cell::from(""),
        Cell::from(""),
        right(format_number(
            report.roots.iter().map(|r| r.summary.input_tokens).sum(),
        )),
        right(format_number(
            report.roots.iter().map(|r| r.summary.output_tokens).sum(),
        )),
    ];
    if columns.cache {
        total.push(right(format_number(
            report
                .roots
                .iter()
                .map(|r| r.summary.cache_creation_tokens)
                .sum(),
        )));
        total.push(right(format_number(
            report
                .roots
                .iter()
                .map(|r| r.summary.cache_read_tokens)
                .sum(),
        )));
        total.push(right(format_number(
            report.roots.iter().map(|r| r.summary.total_tokens()).sum(),
        )));
    }
    total.extend([right(format_currency(total_cost)), Cell::from("")]);

    let table = Table::new(body, widths)
        .header(Row::new(headers).style(Style::default().add_modifier(Modifier::BOLD)))
        .footer(Row::new(total).style(Style::default().fg(Color::Yellow)))
        .row_highlight_style(Style::default().bg(Color::Rgb(0x2a, 0x33, 0x40)))
        .block(Block::default().borders(Borders::ALL));
    state.table.select(Some(selected));
    frame.render_stateful_widget(table, area, &mut state.table);
}

fn table_row(row: &FlatRow<'_>, columns: Columns) -> Row<'static> {
    let node = row.node;
    let summary = &node.summary;
    let marker = match (row.has_children(), row.expanded) {
        (false, _) => "",
        (true, true) => "▾ ",
        (true, false) => "▸ ",
    };
    let family = dominant_family(summary);
    let task_style = if row.depth == 0 {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut cells = vec![
        Cell::from(format!(
            "{}{marker}{}",
            row.prefix,
            sanitize_terminal_text(&node.label)
        ))
        .style(task_style),
        Cell::from(sanitize_terminal_text(&node.agent_type))
            .style(Style::default().fg(Color::Gray)),
        Cell::from(model_list(&summary.models_used)).style(Style::default().fg(family.color())),
        right(format_number(summary.input_tokens)),
        right(format_number(summary.output_tokens)),
    ];
    if columns.cache {
        cells.push(right(format_number(summary.cache_creation_tokens)));
        cells.push(right(format_number(summary.cache_read_tokens)));
        cells.push(right(format_number(summary.total_tokens())));
    }
    cells.push(right(format_currency(summary.total_cost)));
    cells.push(share_cell(node.share, family, columns.bar));
    Row::new(cells)
}

fn right(text: String) -> Cell<'static> {
    Cell::from(Line::from(text).right_aligned())
}

fn model_list(models: &[String]) -> String {
    let mut names = models
        .iter()
        .map(|model| short_model_name(model))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names.join(" · ")
}

fn filled(share: f64, width: usize) -> usize {
    let share = share.clamp(0.0, 1.0);
    let steps = f64::from(u32::try_from(width).unwrap_or(u32::MAX));
    (0..width)
        .take_while(|step| {
            let step = f64::from(u32::try_from(*step).unwrap_or(u32::MAX));
            share >= (step + 0.5) / steps
        })
        .count()
}

fn share_cell(share: f64, family: Family, bar: bool) -> Cell<'static> {
    let percent = format!("{:>5.1}%", share * 100.0);
    if !bar {
        return Cell::from(Line::from(percent).right_aligned());
    }
    let full = filled(share, BAR_WIDTH);
    Cell::from(Line::from(vec![
        Span::styled("█".repeat(full), Style::default().fg(family.color())),
        Span::styled(
            "░".repeat(BAR_WIDTH - full),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(format!(" {percent}")),
    ]))
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, rows: &[FlatRow<'_>], row: &FlatRow<'_>) {
    let [left, right_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);
    let mut path = vec![sanitize_terminal_text(&row.node.label)];
    let mut parent = row.parent_key_path.as_deref();
    while let Some(key) = parent {
        let Some(ancestor) = rows.iter().find(|candidate| candidate.key_path == key) else {
            break;
        };
        path.push(sanitize_terminal_text(&ancestor.node.label));
        parent = ancestor.parent_key_path.as_deref();
    }
    path.reverse();
    let root_key = row.key_path.split('/').next().unwrap_or_default();
    let session_cost = rows
        .iter()
        .find(|candidate| candidate.key_path == root_key)
        .map_or(0.0, |root| root.node.summary.total_cost);
    let of_session = if session_cost > 0.0 {
        row.node.summary.total_cost / session_cost * 100.0
    } else {
        0.0
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(path.join(" › ")).style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from(format!(
                "cost {}    tokens {}    of session {of_session:.1}%",
                format_currency(row.node.summary.total_cost),
                format_number(row.node.summary.total_tokens()),
            )),
        ])
        .block(Block::default().borders(Borders::ALL).title("Selected")),
        left,
    );

    let total = row.node.summary.total_cost;
    let mut breakdowns = row.node.summary.model_breakdowns.iter().collect::<Vec<_>>();
    breakdowns.sort_by(|a, b| b.cost.total_cmp(&a.cost));
    let lines = breakdowns
        .into_iter()
        .map(|breakdown| {
            let share = if total > 0.0 {
                breakdown.cost / total
            } else {
                0.0
            };
            let family = Family::of(&breakdown.model_name);
            let full = filled(share, 20);
            Line::from(vec![
                Span::styled(
                    format!("{:<12}", short_model_name(&breakdown.model_name)),
                    Style::default().fg(family.color()),
                ),
                Span::styled("█".repeat(full), Style::default().fg(family.color())),
                Span::raw(" ".repeat(20 - full)),
                Span::raw(format!(
                    "  {}  {:.1}%",
                    format_currency(breakdown.cost),
                    share * 100.0
                )),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Cost by model (incl. subagents)"),
        ),
        right_area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
    let [keys, legend] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(28)]).areas(area);
    frame.render_widget(
        Paragraph::new("↑↓ move  ←→ collapse/expand  e expand all  c collapse all  s sort  q quit")
            .style(Style::default().fg(Color::Gray)),
        keys,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("■ opus ", Style::default().fg(Family::Opus.color())),
            Span::styled("■ sonnet ", Style::default().fg(Family::Sonnet.color())),
            Span::styled("■ haiku", Style::default().fg(Family::Haiku.color())),
        ]))
        .right_aligned(),
        legend,
    );
}

#[cfg(test)]
mod tests;
