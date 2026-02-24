use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

const ADJECTIVES: &[&str] = &[
    "amber", "bold", "bright", "calm", "clear", "crisp", "deep", "eager",
    "fast", "firm", "fresh", "keen", "light", "quick", "sharp", "slim",
    "soft", "swift", "tall", "warm", "wide", "wise", "young", "brave",
];

const NOUNS: &[&str] = &[
    "brook", "cave", "cliff", "cloud", "creek", "dawn", "dusk", "field",
    "fjord", "grove", "hill", "lake", "leaf", "moon", "peak", "pine",
    "rain", "reef", "ridge", "river", "rock", "star", "stone", "stream",
];

/// Generate a random worktree name like "swift-pine".
pub fn random_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.subsec_nanos() as usize) ^ (d.as_secs() as usize))
        .unwrap_or(12345);
    let pid = std::process::id() as usize;
    let seed = t ^ (pid.wrapping_shl(16)) ^ (pid.wrapping_shr(4));
    let adj = ADJECTIVES[seed % ADJECTIVES.len()];
    let noun = NOUNS[(seed / ADJECTIVES.len()) % NOUNS.len()];
    format!("{}-{}", adj, noun)
}

/// Return the git repository root for the current directory.
pub fn find_git_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git")?;
    if !output.status.success() {
        anyhow::bail!("Not inside a git repository");
    }
    let path = String::from_utf8(output.stdout)
        .context("Git output is not valid UTF-8")?
        .trim()
        .to_string();
    Ok(PathBuf::from(path))
}

/// Validates that `name` is safe for use in paths and branch names.
/// Allows only [A-Za-z0-9._-], rejects path separators and "..".
fn validate_worktree_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Worktree name cannot be empty");
    }
    if name.contains("..") {
        anyhow::bail!("Worktree name cannot contain '..'");
    }
    if name.contains(std::path::MAIN_SEPARATOR) || name.contains('/') {
        anyhow::bail!("Worktree name cannot contain path separators");
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-';
    if !name.chars().all(allowed) {
        anyhow::bail!(
            "Worktree name must be a safe slug (only A-Za-z0-9, ., _, - allowed), got: {:?}",
            name
        );
    }
    Ok(())
}

fn head_sha(dir: &PathBuf) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    pub name: String,
    initial_sha: String,
}

impl Worktree {
    /// Create a new git worktree at `<git_root>/.bamboo/worktrees/<name>` on a
    /// fresh branch `worktree-<name>`.
    pub fn create(name: &str) -> Result<Self> {
        validate_worktree_name(name)?;
        let git_root = find_git_root()?;
        let worktree_dir = git_root.join(".bamboo").join("worktrees").join(name);
        let branch = format!("worktree-{}", name);

        if let Some(parent) = worktree_dir.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create .bamboo/worktrees/ directory")?;
        }

        let output = Command::new("git")
            .args(["worktree", "add"])
            .arg(&worktree_dir)
            .args(["-b", &branch])
            .output()
            .context("Failed to run git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create worktree: {}", stderr.trim());
        }

        let initial_sha = head_sha(&worktree_dir).unwrap_or_default();

        Ok(Worktree {
            path: worktree_dir,
            branch,
            name: name.to_string(),
            initial_sha,
        })
    }

    /// Returns `true` if the worktree has uncommitted changes or new commits
    /// since it was created.
    pub fn has_changes(&self) -> bool {
        // Uncommitted (staged or unstaged) changes or untracked files
        if let Ok(output) = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.path)
            .output()
        {
            if !output.stdout.is_empty() {
                return true;
            }
        }

        // New commits: HEAD moved beyond the initial SHA
        if let Some(current) = head_sha(&self.path) {
            if current != self.initial_sha {
                return true;
            }
        }

        false
    }

    /// Remove the worktree directory and delete its branch.
    pub fn remove(&self) -> Result<()> {
        let output = Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                self.path.to_str().unwrap_or_default(),
            ])
            .output()
            .context("Failed to run git worktree remove")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to remove worktree: {}", stderr.trim());
        }

        // Best-effort branch deletion
        let _ = Command::new("git")
            .args(["branch", "-D", &self.branch])
            .output();

        Ok(())
    }
}
