//! Slash-command catalog for the composer picker.
//!
//! Merges three sources:
//! 1. Built-in omp slash commands (bundled `commands.json`).
//! 2. Global skills under `~/.omp/agent/skills/*/SKILL.md` (and the
//!    `omp-config/global/skills` tree it usually links to).
//! 3. Workspace custom commands under `<cwd>/.omp/commands/*.md` and
//!    `<cwd>/.agents/commands/*.md` when a pane cwd is supplied.
//!
//! Skills and custom commands are real slash targets the agent accepts as
//! `/name` text — same path the TUI uses.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
struct CatalogEntry {
    name: String,
    aliases: Vec<String>,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<String>,
    /// built-in | skill | workspace
    source: String,
}

pub fn build(cwd: Option<&str>) -> Value {
    let mut by_name: BTreeMap<String, CatalogEntry> = BTreeMap::new();

    for entry in builtin_commands() {
        by_name.insert(entry.name.clone(), entry);
    }
    for entry in discover_skills() {
        // Skills win over a built-in of the same name only when the skill
        // name is distinct; keep both when names collide by preferring the
        // skill (operator intent).
        by_name.insert(entry.name.clone(), entry);
    }
    if let Some(cwd) = cwd {
        for entry in discover_workspace_commands(Path::new(cwd)) {
            by_name.insert(entry.name.clone(), entry);
        }
    }

    let mut commands: Vec<CatalogEntry> = by_name.into_values().collect();
    // Stable, human order: built-ins first, then skills, then workspace.
    commands.sort_by(|a, b| {
        source_rank(&a.source)
            .cmp(&source_rank(&b.source))
            .then_with(|| a.name.cmp(&b.name))
    });
    json!({ "commands": commands })
}

fn source_rank(source: &str) -> u8 {
    match source {
        "built-in" => 0,
        "skill" => 1,
        "workspace" => 2,
        _ => 3,
    }
}

fn builtin_commands() -> Vec<CatalogEntry> {
    let raw: Value = serde_json::from_str(include_str!("../commands.json")).unwrap_or(Value::Null);
    let Some(list) = raw.get("commands").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|c| {
            let name = c.get("name")?.as_str()?.to_string();
            if name.is_empty() {
                return None;
            }
            let aliases = c
                .get("aliases")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Some(CatalogEntry {
                name,
                aliases,
                description: c
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                usage: c
                    .get("usage")
                    .and_then(Value::as_str)
                    .map(String::from),
                source: "built-in".to_string(),
            })
        })
        .collect()
}

fn discover_skills() -> Vec<CatalogEntry> {
    let mut out = Vec::new();
    for root in skill_roots() {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&skill_md) else {
                continue;
            };
            let meta = parse_frontmatter(&text);
            let fallback = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let name = meta
                .name
                .filter(|n| !n.is_empty())
                .unwrap_or(fallback);
            if name.is_empty() {
                continue;
            }
            let description = meta
                .description
                .unwrap_or_else(|| format!("Skill: {name}"));
            out.push(CatalogEntry {
                name,
                aliases: meta.aliases,
                description,
                usage: None,
                source: "skill".to_string(),
            });
        }
    }
    out
}

fn skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let agent = PathBuf::from(&home).join(".omp/agent/skills");
        // resolve_symlink so a link to omp-config/global/skills is walked once
        if let Ok(resolved) = fs::canonicalize(&agent) {
            roots.push(resolved);
        } else if agent.is_dir() {
            roots.push(agent);
        }
        let global = PathBuf::from(&home).join(".omp/global/skills");
        if let Ok(resolved) = fs::canonicalize(&global) {
            if !roots.iter().any(|r| r == &resolved) {
                roots.push(resolved);
            }
        }
    }
    roots
}

fn discover_workspace_commands(cwd: &Path) -> Vec<CatalogEntry> {
    let mut out = Vec::new();
    for rel in [".omp/commands", ".agents/commands", ".claude/commands"] {
        let dir = cwd.join(rel);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if stem.is_empty() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let meta = parse_frontmatter(&text);
            let name = meta.name.filter(|n| !n.is_empty()).unwrap_or(stem);
            let description = meta
                .description
                .or_else(|| first_prose_line(&text))
                .unwrap_or_else(|| format!("Workspace command: {name}"));
            out.push(CatalogEntry {
                name,
                aliases: meta.aliases,
                description,
                usage: None,
                source: "workspace".to_string(),
            });
        }
    }
    out
}

struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    aliases: Vec<String>,
}

fn parse_frontmatter(text: &str) -> Frontmatter {
    let mut out = Frontmatter {
        name: None,
        description: None,
        aliases: Vec::new(),
    };
    let Some(rest) = text.strip_prefix("---") else {
        return out;
    };
    let Some(end) = rest.find("\n---") else {
        return out;
    };
    let block = &rest[..end];
    // Minimal YAML: key: value, and folded description via `|`.
    let mut lines = block.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let raw = raw.trim();
        match key {
            "name" => {
                out.name = Some(strip_quotes(raw).to_string());
            }
            "description" => {
                if raw == "|" || raw == ">" || raw.is_empty() {
                    let mut body = String::new();
                    while let Some(next) = lines.peek() {
                        let n = *next;
                        if n.starts_with(' ') || n.starts_with('\t') || n.trim().is_empty() {
                            let taken = lines.next().unwrap_or("");
                            let piece = taken.trim();
                            if !piece.is_empty() {
                                if !body.is_empty() {
                                    body.push(' ');
                                }
                                body.push_str(piece);
                            }
                        } else {
                            break;
                        }
                    }
                    if !body.is_empty() {
                        out.description = Some(body);
                    }
                } else {
                    out.description = Some(strip_quotes(raw).to_string());
                }
            }
            "aliases" => {
                // [a, b] or bare comma list
                let inner = raw
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']');
                out.aliases = inner
                    .split(',')
                    .map(|s| strip_quotes(s.trim()).to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    out
}

fn strip_quotes(s: &str) -> &str {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
}

fn first_prose_line(text: &str) -> Option<String> {
    let body = if let Some(rest) = text.strip_prefix("---") {
        rest.find("\n---")
            .map(|i| &rest[i + 4..])
            .unwrap_or(text)
    } else {
        text
    };
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_and_skills_present() {
        let catalog = build(None);
        let commands = catalog
            .get("commands")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(commands.len() > 60, "expected builtins+skills, got {}", commands.len());
        let names: Vec<&str> = commands
            .iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"settings"), "missing builtin settings");
        // dispatch is a global skill on this machine
        assert!(
            names.iter().any(|n| *n == "dispatch" || *n == "research"),
            "expected at least one skill name in {names:?}"
        );
        let sources: Vec<&str> = commands
            .iter()
            .filter_map(|c| c.get("source").and_then(|s| s.as_str()))
            .collect();
        assert!(sources.contains(&"built-in"));
        assert!(sources.contains(&"skill"), "skills not discovered: {sources:?}");
    }
}
