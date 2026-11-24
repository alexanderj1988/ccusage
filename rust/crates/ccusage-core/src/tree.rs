use serde::Serialize;

use crate::{TimestampMs, UsageSummary};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeNodeKind {
    Session,
    Main,
    Subagent,
    Own,
    Workflow,
}

#[derive(Clone, Debug)]
pub struct UsageTreeNode {
    pub kind: TreeNodeKind,
    /// Unique among siblings and free of `/`, so joined keys identify a node across re-sorting.
    pub key: String,
    pub id: Option<String>,
    pub label: String,
    pub agent_type: String,
    pub summary: UsageSummary,
    pub first_timestamp: TimestampMs,
    pub share: f64,
    pub children: Vec<UsageTreeNode>,
}

impl UsageTreeNode {
    /// Parents leave the cell empty; their children name the models row by row.
    pub fn model_label(&self) -> String {
        if self.children.is_empty() {
            crate::format_model_list(&self.summary.models_used)
        } else {
            String::new()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeOrder {
    Chronological,
    CostDesc,
}

pub fn sort_nodes(nodes: &mut [UsageTreeNode], order: TreeOrder) {
    match order {
        TreeOrder::Chronological => nodes.sort_by(|a, b| {
            thread_rank(a)
                .cmp(&thread_rank(b))
                .then(a.first_timestamp.cmp(&b.first_timestamp))
        }),
        TreeOrder::CostDesc => {
            nodes.sort_by(|a, b| b.summary.total_cost.total_cmp(&a.summary.total_cost));
        }
    }
    for node in nodes {
        sort_nodes(&mut node.children, order);
    }
}

fn thread_rank(node: &UsageTreeNode) -> u8 {
    match node.kind {
        TreeNodeKind::Main | TreeNodeKind::Own => 0,
        TreeNodeKind::Session | TreeNodeKind::Subagent | TreeNodeKind::Workflow => 1,
    }
}

pub fn assign_shares(roots: &mut [UsageTreeNode]) {
    let total = roots.iter().map(|root| root.summary.total_cost).sum();
    assign_level(roots, total);
}

fn assign_level(nodes: &mut [UsageTreeNode], parent_cost: f64) {
    for node in nodes {
        node.share = if parent_cost > 0.0 {
            node.summary.total_cost / parent_cost
        } else {
            0.0
        };
        let cost = node.summary.total_cost;
        assign_level(&mut node.children, cost);
    }
}

#[derive(Debug)]
pub struct FlatRow<'a> {
    pub node: &'a UsageTreeNode,
    pub key_path: String,
    pub parent_key_path: Option<String>,
    pub depth: usize,
    pub prefix: String,
    pub expanded: bool,
}

impl FlatRow<'_> {
    pub fn has_children(&self) -> bool {
        !self.node.children.is_empty()
    }
}

pub fn flatten_tree<'a>(
    roots: &'a [UsageTreeNode],
    is_expanded: &dyn Fn(&str) -> bool,
) -> Vec<FlatRow<'a>> {
    let mut rows = Vec::new();
    for root in roots {
        push_rows(&mut rows, root, None, "", "", is_expanded);
    }
    rows
}

fn push_rows<'a>(
    rows: &mut Vec<FlatRow<'a>>,
    node: &'a UsageTreeNode,
    parent: Option<(&str, usize)>,
    indent: &str,
    branch: &str,
    is_expanded: &dyn Fn(&str) -> bool,
) {
    let key_path = parent.map_or_else(
        || node.key.clone(),
        |(parent_path, _)| format!("{parent_path}/{}", node.key),
    );
    let depth = parent.map_or(0, |(_, parent_depth)| parent_depth + 1);
    let expanded = !node.children.is_empty() && is_expanded(&key_path);
    rows.push(FlatRow {
        node,
        key_path: key_path.clone(),
        parent_key_path: parent.map(|(parent_path, _)| parent_path.to_string()),
        depth,
        prefix: format!("{indent}{branch}"),
        expanded,
    });
    if !expanded {
        return;
    }
    let child_indent = match (depth, branch) {
        (0, _) => String::new(),
        (_, "└─ ") => format!("{indent}   "),
        _ => format!("{indent}│  "),
    };
    let last = node.children.len().saturating_sub(1);
    for (index, child) in node.children.iter().enumerate() {
        let child_branch = if index == last { "└─ " } else { "├─ " };
        push_rows(
            rows,
            child,
            Some((&key_path, depth)),
            &child_indent,
            child_branch,
            is_expanded,
        );
    }
}

#[cfg(test)]
mod tests;
