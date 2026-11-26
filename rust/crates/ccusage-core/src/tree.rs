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
    Model,
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
    pub fn model_label(&self) -> String {
        let names = distinct_model_names(&self.summary.models_used);
        match names.as_slice() {
            [single] => single.clone(),
            names => format!("{} models", names.len()),
        }
    }
}

fn distinct_model_names(models: &[String]) -> Vec<String> {
    let mut names = models
        .iter()
        .map(|model| crate::short_model_name(model))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// Gives every leaf that used several models one child row per model, most expensive first.
pub fn add_model_rows(nodes: &mut [UsageTreeNode]) {
    for node in nodes {
        if !node.children.is_empty() {
            add_model_rows(&mut node.children);
        } else if node.kind != TreeNodeKind::Model
            && distinct_model_names(&node.summary.models_used).len() > 1
        {
            node.children = model_rows(node);
        }
    }
}

fn model_rows(node: &UsageTreeNode) -> Vec<UsageTreeNode> {
    let mut breakdowns = node.summary.model_breakdowns.clone();
    breakdowns.sort_by(|a, b| b.cost.total_cmp(&a.cost));
    breakdowns
        .into_iter()
        .map(|breakdown| UsageTreeNode {
            kind: TreeNodeKind::Model,
            key: format!("model:{}", breakdown.model_name.replace('/', ":")),
            id: None,
            label: crate::short_model_name(&breakdown.model_name),
            agent_type: String::new(),
            summary: UsageSummary {
                input_tokens: breakdown.input_tokens,
                output_tokens: breakdown.output_tokens,
                cache_creation_tokens: breakdown.cache_creation_tokens,
                cache_read_tokens: breakdown.cache_read_tokens,
                extra_total_tokens: breakdown.extra_total_tokens,
                total_cost: breakdown.cost,
                models_used: vec![breakdown.model_name.clone()],
                model_breakdowns: vec![breakdown],
                ..UsageSummary::default()
            },
            first_timestamp: node.first_timestamp,
            share: 0.0,
            children: Vec::new(),
        })
        .collect()
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
        TreeNodeKind::Session
        | TreeNodeKind::Subagent
        | TreeNodeKind::Workflow
        | TreeNodeKind::Model => 1,
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
