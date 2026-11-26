use std::path::{Path, PathBuf};

use memchr::memmem;
use serde::Deserialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EntryOrigin {
    Main,
    Subagent {
        agent_id: String,
        workflow: Option<String>,
    },
}

fn file_name(path: Option<&Path>) -> Option<&str> {
    path.and_then(Path::file_name)
        .and_then(|name| name.to_str())
}

pub(crate) fn classify_origin(path: &Path) -> EntryOrigin {
    let Some(agent_id) = file_name(Some(path))
        .and_then(|name| name.strip_suffix(".jsonl"))
        .and_then(|stem| stem.strip_prefix("agent-"))
    else {
        return EntryOrigin::Main;
    };
    let parent = path.parent();
    if file_name(parent) == Some("subagents") {
        return EntryOrigin::Subagent {
            agent_id: agent_id.to_string(),
            workflow: None,
        };
    }
    let workflows = parent.and_then(Path::parent);
    if file_name(workflows) == Some("workflows")
        && file_name(workflows.and_then(Path::parent)) == Some("subagents")
        && let Some(workflow) = file_name(parent)
    {
        return EntryOrigin::Subagent {
            agent_id: agent_id.to_string(),
            workflow: Some(workflow.to_string()),
        };
    }
    EntryOrigin::Main
}

pub(crate) fn main_transcript_path(path: &Path) -> Option<PathBuf> {
    match classify_origin(path) {
        EntryOrigin::Main => Some(path.to_path_buf()),
        EntryOrigin::Subagent { .. } => {
            let subagents = path
                .ancestors()
                .find(|ancestor| file_name(Some(ancestor)) == Some("subagents"))?;
            let session_dir = subagents.parent()?;
            let session_id = file_name(Some(session_dir))?;
            Some(session_dir.with_file_name(format!("{session_id}.jsonl")))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMeta {
    pub(crate) agent_type: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) tool_use_id: Option<String>,
    pub(crate) spawn_depth: Option<u32>,
}

pub(crate) fn read_agent_meta(transcript: &Path) -> Option<AgentMeta> {
    let bytes = std::fs::read(transcript.with_extension("meta.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TitleRecord {
    #[serde(rename = "type")]
    kind: String,
    custom_title: Option<String>,
    ai_title: Option<String>,
}

pub(crate) fn read_session_title(main_transcript: &Path) -> Option<String> {
    let bytes = std::fs::read(main_transcript).ok()?;
    let marker = memmem::Finder::new(b"-title\"");
    let mut custom = None;
    let mut ai = None;
    for line in bytes.split(|byte| *byte == b'\n') {
        if marker.find(line).is_none() {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<TitleRecord>(line) else {
            continue;
        };
        match record.kind.as_str() {
            "custom-title" => custom = non_blank(record.custom_title).or(custom),
            "ai-title" => ai = non_blank(record.ai_title).or(ai),
            _ => {}
        }
    }
    custom.or(ai)
}

fn non_blank(title: Option<String>) -> Option<String> {
    title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
}

pub(crate) fn tool_use_ids(transcript: &Path) -> Vec<String> {
    const NEEDLE: &[u8] = b"\"id\":\"toolu_";
    const ID_OFFSET: usize = b"\"id\":\"".len();
    let Ok(bytes) = std::fs::read(transcript) else {
        return Vec::new();
    };
    memmem::find_iter(&bytes, NEEDLE)
        .filter_map(|start| {
            let id_start = start + ID_OFFSET;
            let len = memchr::memchr(b'"', &bytes[id_start..])?;
            std::str::from_utf8(&bytes[id_start..id_start + len])
                .ok()
                .map(str::to_string)
        })
        .collect()
}
