use std::{collections::HashSet, path::Path, sync::Arc};

use ccusage_core::{
    Result, SessionAccumulator, TimestampMs, UsageSummary,
    cli::SortOrder,
    fast::FxHashMap,
    tree::{TreeNodeKind, TreeOrder, UsageTreeNode, assign_shares, sort_nodes},
};

use super::source::{
    AgentMeta, EntryOrigin, classify_origin, main_transcript_path, read_agent_meta,
    read_session_title, tool_use_ids,
};
use crate::ClaudeEntry;

pub(crate) struct SessionTrees {
    pub(crate) roots: Vec<UsageTreeNode>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn build_session_trees(
    entries: &[ClaudeEntry],
    root_order: SortOrder,
) -> Result<SessionTrees> {
    let mut warnings = Vec::new();
    let mut roots = Vec::new();
    for group in group_sessions(entries) {
        if let Some(root) = build_session(&group, &mut warnings)? {
            roots.push(root);
        }
    }
    make_root_keys_unique(&mut roots);
    for root in &mut roots {
        sort_nodes(&mut root.children, TreeOrder::Chronological);
    }
    roots.sort_by(|a, b| match root_order {
        SortOrder::Asc => a.summary.total_cost.total_cmp(&b.summary.total_cost),
        SortOrder::Desc => b.summary.total_cost.total_cmp(&a.summary.total_cost),
    });
    assign_shares(&mut roots);
    Ok(SessionTrees { roots, warnings })
}

struct SessionGroup<'a> {
    session_id: Arc<str>,
    entries: Vec<&'a ClaudeEntry>,
}

fn group_sessions(entries: &[ClaudeEntry]) -> Vec<SessionGroup<'_>> {
    let mut groups = Vec::<SessionGroup<'_>>::new();
    let mut indexes = FxHashMap::<(Arc<str>, Arc<str>), usize>::default();
    for entry in entries {
        let key = (
            Arc::clone(&entry.project_path),
            Arc::clone(&entry.session_id),
        );
        let index = *indexes.entry(key).or_insert_with(|| {
            groups.push(SessionGroup {
                session_id: Arc::clone(&entry.session_id),
                entries: Vec::new(),
            });
            groups.len() - 1
        });
        groups[index].entries.push(entry);
    }
    groups
}

struct AgentDraft<'a> {
    agent_id: String,
    workflow: Option<String>,
    transcript: Arc<Path>,
    meta: AgentMeta,
    entries: Vec<&'a ClaudeEntry>,
}

impl AgentDraft<'_> {
    fn load(
        agent_id: String,
        workflow: Option<String>,
        transcript: &Arc<Path>,
        warnings: &mut Vec<String>,
    ) -> Self {
        let meta = read_agent_meta(transcript).unwrap_or_else(|| {
            warnings.push(format!(
                "Missing or unreadable subagent metadata for {}",
                transcript.display()
            ));
            AgentMeta::default()
        });
        Self {
            agent_id,
            workflow,
            transcript: Arc::clone(transcript),
            meta,
            entries: Vec::new(),
        }
    }

    fn spawn_depth(&self) -> u32 {
        self.meta.spawn_depth.unwrap_or(1)
    }
}

fn build_session(
    group: &SessionGroup<'_>,
    warnings: &mut Vec<String>,
) -> Result<Option<UsageTreeNode>> {
    let session = summarize(&group.entries)?;
    if session.input_tokens == 0
        && session.output_tokens == 0
        && session.cache_creation_tokens == 0
        && session.cache_read_tokens == 0
    {
        return Ok(None);
    }

    let mut main_entries = Vec::new();
    let mut agents = Vec::<AgentDraft<'_>>::new();
    let mut files = FxHashMap::<Arc<Path>, Option<usize>>::default();
    for &entry in &group.entries {
        let slot = *files
            .entry(Arc::clone(&entry.source_file))
            .or_insert_with(|| match classify_origin(&entry.source_file) {
                EntryOrigin::Main => None,
                EntryOrigin::Subagent { agent_id, workflow } => {
                    agents.push(AgentDraft::load(
                        agent_id,
                        workflow,
                        &entry.source_file,
                        warnings,
                    ));
                    Some(agents.len() - 1)
                }
            });
        match slot {
            Some(index) => agents[index].entries.push(entry),
            None => main_entries.push(entry),
        }
    }

    let parents = resolve_parents(&agents);
    let mut children_of = vec![Vec::new(); agents.len()];
    let mut top = Vec::new();
    let mut workflows = Vec::<(String, Vec<usize>)>::new();
    for (index, agent) in agents.iter().enumerate() {
        match (parents[index], &agent.workflow) {
            (Some(parent), _) => children_of[parent].push(index),
            (None, Some(name)) => match workflows.iter_mut().find(|(known, _)| known == name) {
                Some((_, members)) => members.push(index),
                None => workflows.push((name.clone(), vec![index])),
            },
            (None, None) => top.push(index),
        }
    }

    let mut children = Vec::new();
    if !main_entries.is_empty() {
        children.push(node(
            TreeNodeKind::Main,
            "main".to_string(),
            None,
            "main thread".to_string(),
            "main".to_string(),
            &main_entries,
            Vec::new(),
        )?);
    }
    for &index in &top {
        children.push(agent_node(index, &agents, &children_of)?);
    }
    for (name, members) in workflows {
        let nodes = members
            .iter()
            .map(|&index| agent_node(index, &agents, &children_of))
            .collect::<Result<Vec<_>>>()?;
        let entries = members
            .iter()
            .flat_map(|&index| subtree_entries(index, &agents, &children_of))
            .collect::<Vec<_>>();
        children.push(node(
            TreeNodeKind::Workflow,
            format!("wf:{name}"),
            Some(name.clone()),
            format!("workflow {name}"),
            "workflow".to_string(),
            &entries,
            nodes,
        )?);
    }

    if children.len() == 1 && children[0].kind == TreeNodeKind::Main {
        children.clear();
    }

    let title = group
        .entries
        .first()
        .and_then(|entry| main_transcript_path(&entry.source_file))
        .and_then(|path| read_session_title(&path))
        .unwrap_or_else(|| group.session_id.chars().take(8).collect());
    Ok(Some(UsageTreeNode {
        kind: TreeNodeKind::Session,
        key: group.session_id.to_string(),
        id: Some(group.session_id.to_string()),
        label: title,
        agent_type: "Claude · main".to_string(),
        first_timestamp: first_timestamp(&group.entries),
        summary: session,
        share: 0.0,
        children,
    }))
}

fn resolve_parents(agents: &[AgentDraft<'_>]) -> Vec<Option<usize>> {
    let mut parents = vec![None; agents.len()];
    let nested = |agent: &AgentDraft<'_>| {
        agent.workflow.is_none() && agent.spawn_depth() >= 2 && agent.meta.tool_use_id.is_some()
    };
    if !agents.iter().any(nested) {
        return parents;
    }
    let mut spawned_by = FxHashMap::<String, usize>::default();
    for (index, agent) in agents.iter().enumerate() {
        if agent.workflow.is_none() {
            for id in tool_use_ids(&agent.transcript) {
                spawned_by.entry(id).or_insert(index);
            }
        }
    }
    for (index, agent) in agents.iter().enumerate() {
        if nested(agent) {
            parents[index] = agent
                .meta
                .tool_use_id
                .as_ref()
                .and_then(|id| spawned_by.get(id))
                .copied()
                .filter(|parent| *parent != index);
        }
    }
    break_cycles(&mut parents);
    parents
}

fn break_cycles(parents: &mut [Option<usize>]) {
    for start in 0..parents.len() {
        let mut seen = vec![false; parents.len()];
        let mut current = start;
        while let Some(parent) = parents[current] {
            if parent == start || seen[parent] {
                parents[start] = None;
                break;
            }
            seen[current] = true;
            current = parent;
        }
    }
}

fn agent_node(
    index: usize,
    agents: &[AgentDraft<'_>],
    children_of: &[Vec<usize>],
) -> Result<UsageTreeNode> {
    let agent = &agents[index];
    let mut children = children_of[index]
        .iter()
        .map(|&child| agent_node(child, agents, children_of))
        .collect::<Result<Vec<_>>>()?;
    let agent_type = agent
        .meta
        .agent_type
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    if !children.is_empty() && !agent.entries.is_empty() {
        children.insert(
            0,
            node(
                TreeNodeKind::Own,
                "own".to_string(),
                None,
                "own turns".to_string(),
                agent_type.clone(),
                &agent.entries,
                Vec::new(),
            )?,
        );
    }
    let label = agent
        .meta
        .description
        .clone()
        .filter(|description| !description.trim().is_empty())
        .unwrap_or_else(|| format!("agent-{}", agent.agent_id));
    node(
        TreeNodeKind::Subagent,
        format!("agent-{}", agent.agent_id),
        Some(agent.agent_id.clone()),
        label,
        agent_type,
        &subtree_entries(index, agents, children_of),
        children,
    )
}

fn subtree_entries<'a>(
    index: usize,
    agents: &[AgentDraft<'a>],
    children_of: &[Vec<usize>],
) -> Vec<&'a ClaudeEntry> {
    let mut entries = agents[index].entries.clone();
    for &child in &children_of[index] {
        entries.extend(subtree_entries(child, agents, children_of));
    }
    entries
}

fn node(
    kind: TreeNodeKind,
    key: String,
    id: Option<String>,
    label: String,
    agent_type: String,
    entries: &[&ClaudeEntry],
    children: Vec<UsageTreeNode>,
) -> Result<UsageTreeNode> {
    Ok(UsageTreeNode {
        kind,
        key,
        id,
        label,
        agent_type,
        summary: summarize(entries)?,
        first_timestamp: first_timestamp(entries),
        share: 0.0,
        children,
    })
}

fn summarize(entries: &[&ClaudeEntry]) -> Result<UsageSummary> {
    let mut accumulator = SessionAccumulator::default();
    for entry in entries {
        accumulator.add_entry(entry);
    }
    accumulator.into_summary()
}

fn first_timestamp(entries: &[&ClaudeEntry]) -> TimestampMs {
    entries
        .iter()
        .map(|entry| entry.timestamp)
        .min()
        .unwrap_or(TimestampMs::UNIX_EPOCH)
}

fn make_root_keys_unique(roots: &mut [UsageTreeNode]) {
    let mut seen = HashSet::new();
    for root in roots {
        let base = root.key.clone();
        let mut suffix = 1;
        while !seen.insert(root.key.clone()) {
            suffix += 1;
            root.key = format!("{base}#{suffix}");
        }
    }
}
