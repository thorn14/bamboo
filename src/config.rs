use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LayoutConfig {
    Scroll,
    Fixed,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self::Scroll
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_shell")]
    pub default_shell: String,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub panes: Vec<PaneConfig>,
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_shell: default_shell(),
            layout: LayoutConfig::default(),
            panes: vec![PaneConfig {
                name: "Shell".to_string(),
                command: None,
                cwd: None,
                env: HashMap::new(),
            }],
        }
    }
}

/// Whether a config file was found or the interactive wizard should be invoked.
///
/// `NeedsWizard` is returned only when *no* config file exists at any of the
/// standard locations and no explicit `--config` path was supplied.  In
/// non-TTY environments (CI, piped stdin) the caller should fall back to
/// `Config::default()` rather than attempting to run the wizard.
pub enum ConfigSource {
    /// Config was loaded from an explicit path or a discovered file.
    File(Config),
    /// No config file was found anywhere; the wizard should run (TTY only).
    NeedsWizard,
}

impl Config {
    /// Look for a config file in priority order:
    ///
    /// 1. Explicit `--config <path>` flag
    /// 2. `.bamboo.toml` in the current directory
    /// 3. `~/.config/bamboo/config.toml` / `$XDG_CONFIG_HOME/bamboo/config.toml`
    ///
    /// Returns `ConfigSource::NeedsWizard` only when none of the above exist.
    /// A global config therefore bypasses the wizard — users who have set one
    /// up are not prompted on every new repo.
    pub fn load(path: Option<&str>) -> Result<ConfigSource> {
        // Explicit --config path always wins.
        if let Some(p) = path {
            let config_path = PathBuf::from(p);
            return Self::read_file(&config_path).map(ConfigSource::File);
        }

        // Local .bamboo.toml takes next priority.
        let local = PathBuf::from(".bamboo.toml");
        if local.exists() {
            return Self::read_file(&local).map(ConfigSource::File);
        }

        // Fall back to global config locations.  If one exists we use it
        // directly without prompting — the wizard is only for repos that have
        // no configuration anywhere.
        let global_candidates = [
            dirs::home_dir().map(|h| h.join(".config").join("bamboo").join("config.toml")),
            dirs::config_dir().map(|d| d.join("bamboo").join("config.toml")),
        ];
        if let Some(global) = global_candidates.into_iter().flatten().find(|p| p.exists()) {
            return Self::read_file(&global).map(ConfigSource::File);
        }

        // No config found anywhere → run the interactive wizard.
        Ok(ConfigSource::NeedsWizard)
    }

    fn read_file(config_path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config from {}", config_path.display()))?;

        let mut config: Config =
            toml::from_str(&contents).with_context(|| "Failed to parse config TOML")?;

        if config.panes.is_empty() {
            config.panes.push(PaneConfig {
                name: "Shell".to_string(),
                command: None,
                cwd: None,
                env: HashMap::new(),
            });
        }

        Ok(config)
    }

    pub fn resolve_cwd(cwd: &Option<String>) -> Option<PathBuf> {
        cwd.as_ref().map(|s| {
            if s.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    home.join(s.strip_prefix("~/").unwrap_or(s))
                } else {
                    PathBuf::from(s)
                }
            } else {
                PathBuf::from(s)
            }
        })
    }
}
