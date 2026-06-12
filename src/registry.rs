//! Workspace registry persisted at ~/.beads-deck/registry.json.

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WorkspaceEntry {
    pub(crate) name: String,
    pub(crate) path: String,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Registry {
    #[serde(default)]
    pub(crate) workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    pub(crate) last: Option<String>,
}

pub(crate) fn registry_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::Path::new(&home).join(".beads-deck").join("registry.json"))
}

pub(crate) fn load_registry() -> Registry {
    if let Some(p) = registry_path() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(r) = serde_json::from_str::<Registry>(&s) {
                return r;
            }
        }
    }
    import_from_beadbox().unwrap_or_default()
}

pub(crate) fn save_registry(reg: &Registry) {
    if let Some(p) = registry_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(reg) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// First-run seed: import local workspaces from Beadbox's registry if present.
pub(crate) fn import_from_beadbox() -> Option<Registry> {
    let home = std::env::var("HOME").ok()?;
    let p = std::path::Path::new(&home).join(".beadbox").join("registry.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
    let mut workspaces = Vec::new();
    for w in v.get("workspaces")?.as_array()? {
        let name = w.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if let Some(path) = w.get("local").and_then(|l| l.get("path")).and_then(|x| x.as_str()) {
            // Beadbox stores the `.beads` dir; we want the project dir.
            let path = path.strip_suffix("/.beads").unwrap_or(path).to_string();
            workspaces.push(WorkspaceEntry { name, path });
        }
    }
    Some(Registry { workspaces, last: None })
}
