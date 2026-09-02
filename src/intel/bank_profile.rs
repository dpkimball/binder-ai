//! Workspace profile: bank ID prefixes (spec 017 Phase 1).
//!
//! Library default is **empty**. DPK (and other operators) load
//! `binder-profile.toml` via `BINDER_WORKSPACE_PROFILE` or
//! `$BINDER_AI_WORKSPACE/binder-profile.toml`.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;

/// Repo name → question-bank ID prefix (`codeintel` → `C`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BankPrefixRegistry {
    prefixes: BTreeMap<String, String>,
}

impl BankPrefixRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_map(prefixes: BTreeMap<String, String>) -> Self {
        Self { prefixes }
    }

    pub fn prefix_for(&self, repo: &str) -> Option<&str> {
        self.prefixes.get(repo).map(String::as_str)
    }

    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.prefixes
    }

    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct ProfileFile {
    #[serde(default)]
    bank_prefixes: BTreeMap<String, String>,
}

/// Parse a binder-profile.toml (or any TOML with `[bank_prefixes]`).
pub fn parse_profile_toml(text: &str) -> Result<BankPrefixRegistry, String> {
    let parsed: ProfileFile =
        toml::from_str(text).map_err(|err| format!("binder-profile.toml: {err}"))?;
    Ok(BankPrefixRegistry::from_map(parsed.bank_prefixes))
}

pub fn load_profile_from_path(path: &Path) -> Result<BankPrefixRegistry, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("read profile {}: {err}", path.display()))?;
    parse_profile_toml(&text)
}

/// `BINDER_WORKSPACE_PROFILE` or `$BINDER_AI_WORKSPACE/binder-profile.toml` if present.
pub fn resolve_profile_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("BINDER_WORKSPACE_PROFILE") {
        let path = PathBuf::from(p);
        if path.as_os_str().is_empty() {
            return None;
        }
        return Some(path);
    }
    if let Ok(ws) = env::var("BINDER_AI_WORKSPACE") {
        let root = PathBuf::from(ws);
        for rel in [
            "binder-profile.toml",
            "binder-workspace/binder-profile.toml",
        ] {
            let candidate = root.join(rel);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Load and install from env resolution. Returns path loaded, if any.
pub fn install_bank_prefixes_from_env() -> Result<Option<PathBuf>, String> {
    let Some(path) = resolve_profile_path() else {
        install_bank_prefix_registry(BankPrefixRegistry::empty());
        return Ok(None);
    };
    let reg = load_profile_from_path(&path)?;
    install_bank_prefix_registry(reg);
    Ok(Some(path))
}

static REGISTRY: OnceLock<RwLock<BankPrefixRegistry>> = OnceLock::new();

fn registry() -> &'static RwLock<BankPrefixRegistry> {
    REGISTRY.get_or_init(|| RwLock::new(BankPrefixRegistry::empty()))
}

/// Replace the process-wide bank prefix map (serve/plan startup and tests).
pub fn install_bank_prefix_registry(reg: BankPrefixRegistry) {
    *registry().write().expect("bank prefix registry write") = reg;
}

/// Snapshot of the installed map (for parity tests / debugging).
pub fn bank_prefix_map() -> BTreeMap<String, String> {
    registry()
        .read()
        .expect("bank prefix registry read")
        .as_map()
        .clone()
}

/// Question-bank ID prefix for a repo (`C1` → codeintel when profile says so).
pub fn guidance_id_prefix(repo: &str) -> Option<String> {
    registry()
        .read()
        .expect("bank prefix registry read")
        .prefix_for(repo)
        .map(str::to_string)
}

/// DPK map used only in unit tests and as the checked-in `binder-profile.toml` content.
/// Not the library default.
#[cfg(test)]
pub fn dpk_bank_prefixes_fixture() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("codeintel".into(), "C".into()),
        ("binder-ai".into(), "A".into()),
        ("bindb".into(), "B".into()),
        ("foreman".into(), "F".into()),
        ("binder-llm".into(), "L".into()),
        ("repo-dashboard".into(), "R".into()),
        ("dpk-build".into(), "DB".into()),
        ("dpk-ci".into(), "DC".into()),
        ("dpk-infra".into(), "DI".into()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_has_no_prefixes() {
        let reg = BankPrefixRegistry::empty();
        assert!(reg.prefix_for("codeintel").is_none());
    }

    #[test]
    fn parse_and_lookup() {
        let toml = r#"
profile_id = "acme"
[bank_prefixes]
acme-api = "Z"
"#;
        let reg = parse_profile_toml(toml).unwrap();
        assert_eq!(reg.prefix_for("acme-api"), Some("Z"));
        assert!(reg.prefix_for("codeintel").is_none());
    }

    #[test]
    fn install_updates_guidance_id_prefix() {
        let mut map = BTreeMap::new();
        map.insert("acme-api".into(), "Z".into());
        install_bank_prefix_registry(BankPrefixRegistry::from_map(map));
        assert_eq!(guidance_id_prefix("acme-api").as_deref(), Some("Z"));
        assert!(guidance_id_prefix("codeintel").is_none());
        // Restore DPK fixture so parallel repo_ask tests keep working.
        install_bank_prefix_registry(BankPrefixRegistry::from_map(dpk_bank_prefixes_fixture()));
    }
}
