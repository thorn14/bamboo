use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};

use anyhow::Result;

use crate::config::{Config, LayoutConfig, PaneConfig};

/// Run the interactive setup wizard and return the resulting `Config`.
///
/// Returns `Config::default()` without prompting when stdin is not a TTY
/// (e.g. piped input, CI environments).
pub fn run_wizard() -> Result<Config> {
    if !io::stdin().is_terminal() {
        return Ok(Config::default());
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    writeln!(out, "\nNo .bamboo.toml found for this repo. Let's set it up.")?;
    writeln!(out)?;

    // --- Shell ---
    let default_shell =
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell = prompt(&mut out, &format!("Shell [{}]", default_shell))?;
    let shell = if shell.is_empty() {
        default_shell
    } else {
        shell
    };

    // --- Layout ---
    let layout_input = prompt(&mut out, "Layout (Scroll/Fixed) [Scroll]")?;
    let layout = match layout_input.to_lowercase().as_str() {
        "fixed" => LayoutConfig::Fixed,
        _ => LayoutConfig::Scroll,
    };

    // --- Panes ---
    writeln!(out)?;
    writeln!(out, "Add panes (leave name blank to finish):")?;

    let mut panes: Vec<PaneConfig> = Vec::new();
    let mut idx = 1usize;

    loop {
        let default_name = if idx == 1 { "Shell".to_string() } else { String::new() };
        let name_hint = if default_name.is_empty() {
            format!("  Pane {} name (blank to finish)", idx)
        } else {
            format!("  Pane {} name [{}]", idx, default_name)
        };

        let name_input = prompt(&mut out, &name_hint)?;
        let name = if name_input.is_empty() {
            if default_name.is_empty() {
                // blank with no default → done
                break;
            }
            default_name
        } else {
            name_input
        };

        let command_input = prompt(
            &mut out,
            &format!("  Pane {} command (blank for interactive shell)", idx),
        )?;
        let command = if command_input.is_empty() {
            None
        } else {
            Some(command_input)
        };

        let cwd_input = prompt(
            &mut out,
            &format!("  Pane {} cwd (blank for current directory)", idx),
        )?;
        let cwd = if cwd_input.is_empty() {
            None
        } else {
            Some(cwd_input)
        };

        writeln!(out)?;

        panes.push(PaneConfig {
            name,
            command,
            cwd,
            env: HashMap::new(),
        });

        idx += 1;
    }

    if panes.is_empty() {
        panes.push(PaneConfig {
            name: "Shell".to_string(),
            command: None,
            cwd: None,
            env: HashMap::new(),
        });
    }

    let config = Config {
        default_shell: shell,
        layout,
        panes,
    };

    // --- Save prompt ---
    writeln!(out)?;
    let save = prompt(&mut out, "Save config to .bamboo.toml? [Y/n]")?;
    if save.is_empty() || save.to_lowercase().starts_with('y') {
        write_config(&config)?;
        writeln!(out, "Saved .bamboo.toml")?;
    } else {
        writeln!(out, "Skipped saving — config applies to this session only.")?;
    }

    writeln!(out)?;

    Ok(config)
}

fn prompt(out: &mut impl Write, label: &str) -> Result<String> {
    write!(out, "{}: ", label)?;
    out.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Serialize `config` to TOML and write it to `.bamboo.toml` in the current directory.
pub fn write_config(config: &Config) -> Result<()> {
    let toml_str = toml::to_string_pretty(config)?;
    std::fs::write(".bamboo.toml", toml_str)?;
    Ok(())
}
