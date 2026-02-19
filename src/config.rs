use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct PaneConfig {
    pub name: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
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

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            Some(PathBuf::from(p))
        } else {
            // Priority: local .notebook-tui.toml > global config
            let local = PathBuf::from(".bamboo.toml");
            if local.exists() {
                Some(local)
            } else {
                let candidates = [
                    dirs::home_dir().map(|h| h.join(".config").join("bamboo").join("config.toml")),
                    dirs::config_dir().map(|d| d.join("bamboo").join("config.toml")),
                ];
                candidates.into_iter().flatten().find(|p| p.exists())
            }
        };

        let config_path = match config_path {
            Some(p) if p.exists() => p,
            _ => return Ok(Config::default()),
        };

        let contents = std::fs::read_to_string(&config_path)
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
