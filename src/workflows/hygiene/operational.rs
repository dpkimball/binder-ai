use std::collections::BTreeSet;

/// Live operational state from scanner / dashboard / skill. Not bindb.
///
/// Dirty trees, PR heads, and the current branch are GitHub/git facts.
/// HygieneWorkflow consumes them; it does not discover them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalContext {
    pub default_branch: String,
    pub current_branch: String,
    pub dirty: bool,
    pub open_pr_heads: Vec<String>,
}

impl Default for OperationalContext {
    fn default() -> Self {
        Self {
            default_branch: "master".into(),
            current_branch: "master".into(),
            dirty: false,
            open_pr_heads: Vec::new(),
        }
    }
}

impl OperationalContext {
    /// Same prune guards as repo-dashboard hygiene and the Cursor skill.
    pub fn delete_blocked_reason(&self, branch: &str) -> Option<String> {
        if branch.is_empty() {
            return Some("will not delete: empty branch name".into());
        }
        if branch == self.default_branch {
            return Some(format!("will not delete default branch `{branch}`"));
        }
        let heads: BTreeSet<&str> = self.open_pr_heads.iter().map(String::as_str).collect();
        if heads.contains(branch) {
            return Some(format!("will not delete `{branch}`: it is an open PR head"));
        }
        if self.current_branch == branch && self.dirty {
            return Some(format!(
                "will not delete `{branch}`: it is checked out and dirty"
            ));
        }
        None
    }

    pub fn needs_checkout_before_delete(&self, branch: &str) -> bool {
        self.current_branch == branch && !self.dirty
    }
}
