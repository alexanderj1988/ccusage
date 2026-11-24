use serde_json::{Value, json};

use super::{
    format_currency, format_number, print_estimated_pricing_notes_for_models,
    print_missing_pricing_warnings_for_models, sanitize_terminal_text,
    table::{empty_usage_table_message, should_use_compact_layout},
    totals_json,
};
use crate::{
    Color, ColumnKind, Result, SimpleTable, Totals, USAGE_COMPACT_WIDTH_THRESHOLD, UsageSummary,
    cli::SharedArgs,
    color, print_box_title, terminal_width,
    tree::{FlatRow, TreeNodeKind, UsageTreeNode, flatten_tree},
};

pub fn print_usage_tree_table(
    title: &str,
    roots: &[UsageTreeNode],
    shared: &SharedArgs,
) -> Result<()> {
    if roots.is_empty() {
        ccusage_terminal::errln!("{}", empty_usage_table_message());
        return Ok(());
    }
    let terminal_width = terminal_width();
    let compact = should_use_compact_layout(
        shared,
        ccusage_terminal::stdout_is_terminal(),
        terminal_width,
        USAGE_COMPACT_WIDTH_THRESHOLD,
    );
    print_box_title(title, shared)?;
    let (headers, columns) = tree_table_columns(compact, shared.no_cost);
    let mut table = SimpleTable::new(headers, columns, crate::terminal_style(shared))
        .with_terminal_width(terminal_width);
    let rows = flatten_tree(roots, &|_| true);
    for (index, row) in rows.iter().enumerate() {
        if row.depth == 0 && index > 0 {
            table.separator();
        }
        table.push(tree_row_values(row, compact, shared));
    }

    let summaries = root_summaries(roots);
    let totals = Totals::from_summaries(&summaries);
    let total = |value: String| color(shared, value, Color::Yellow);
    let mut total_row = vec![
        total("Total".to_string()),
        String::new(),
        String::new(),
        total(format_number(totals.input_tokens)),
        total(format_number(totals.output_tokens)),
    ];
    if !compact {
        total_row.push(total(format_number(totals.cache_creation_tokens)));
        total_row.push(total(format_number(totals.cache_read_tokens)));
        total_row.push(total(format_number(totals.total_tokens)));
    }
    if !shared.no_cost {
        total_row.push(total(format_currency(totals.total_cost)));
    }
    if !shared.no_cost {
        total_row.push(String::new());
    }
    table.separator();
    table.push(total_row);
    table.print()?;
    print_estimated_pricing_notes_for_models(totals.estimated_models.iter().map(String::as_str));
    print_missing_pricing_warnings_for_models(
        totals.unpriced_models.iter().map(String::as_str),
        shared.offline,
    );
    Ok(())
}

fn tree_table_columns(compact: bool, no_cost: bool) -> (Vec<&'static str>, Vec<ColumnKind>) {
    let mut headers = vec!["Task", "Agent", "Models", "Input", "Output"];
    let mut columns = vec![
        ColumnKind::Primary,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Numeric,
        ColumnKind::Numeric,
    ];
    if !compact {
        headers.extend(["Cache Create", "Cache Read", "Total Tokens"]);
        columns.extend([
            ColumnKind::Numeric,
            ColumnKind::Numeric,
            ColumnKind::Numeric,
        ]);
    }
    if !no_cost {
        headers.push("Cost (USD)");
        columns.push(ColumnKind::Numeric);
        headers.push("Share");
        columns.push(ColumnKind::Numeric);
    }
    (headers, columns)
}

pub(super) fn tree_row_values(
    row: &FlatRow<'_>,
    compact: bool,
    shared: &SharedArgs,
) -> Vec<String> {
    let node = row.node;
    let summary = &node.summary;
    let label = format!("{}{}", row.prefix, sanitize_terminal_text(&node.label));
    let label = if node.kind == TreeNodeKind::Session {
        color(shared, label, Color::Blue)
    } else {
        label
    };
    let mut values = vec![
        label,
        sanitize_terminal_text(&node.agent_type),
        node.model_label(),
        format_number(summary.input_tokens),
        format_number(summary.output_tokens),
    ];
    if !compact {
        values.push(format_number(summary.cache_creation_tokens));
        values.push(format_number(summary.cache_read_tokens));
        values.push(format_number(summary.total_tokens()));
    }
    if !shared.no_cost {
        values.push(format_currency(summary.total_cost));
        values.push(format!("{:.1}%", node.share * 100.0));
    }
    values
}

fn root_summaries(roots: &[UsageTreeNode]) -> Vec<UsageSummary> {
    roots.iter().map(|root| root.summary.clone()).collect()
}

pub fn tree_report_json(roots: &[UsageTreeNode]) -> Value {
    json!({
        "sessions": roots.iter().map(tree_node_json).collect::<Vec<_>>(),
        "totals": totals_json(&root_summaries(roots)),
    })
}

fn tree_node_json(node: &UsageTreeNode) -> Value {
    let summary = &node.summary;
    let mut value = json!({
        "kind": node.kind,
        "label": node.label,
        "agentType": node.agent_type,
        "inputTokens": summary.input_tokens,
        "outputTokens": summary.output_tokens,
        "cacheCreationTokens": summary.cache_creation_tokens,
        "cacheReadTokens": summary.cache_read_tokens,
        "totalTokens": summary.total_tokens(),
        "totalCost": summary.total_cost,
        "share": node.share,
        "firstActivity": summary.first_activity,
        "lastActivity": summary.last_activity,
        "modelsUsed": summary.models_used,
        "modelBreakdowns": summary.model_breakdowns,
        "children": node.children.iter().map(tree_node_json).collect::<Vec<_>>(),
    });
    let id_field = match node.kind {
        TreeNodeKind::Session => Some("sessionId"),
        TreeNodeKind::Subagent => Some("agentId"),
        TreeNodeKind::Workflow => Some("workflowId"),
        TreeNodeKind::Main | TreeNodeKind::Own => None,
    };
    if let Some(object) = value.as_object_mut() {
        if let Some(field) = id_field {
            object.insert(field.to_string(), json!(node.id));
        }
        if node.kind == TreeNodeKind::Session {
            object.insert("projectPath".to_string(), json!(summary.project_path));
        }
    }
    value
}

#[cfg(test)]
mod tests;
