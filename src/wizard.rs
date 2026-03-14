use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};

use anyhow::Result;

use crate::config::{Config, LayoutConfig, PaneConfig};

// ANSI color/style constants for terminal output.
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const BOLD_YELLOW: &str = "\x1b[1;33m";

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

    writeln!(out)?;
    writeln!(
        out,
        "{BOLD_CYAN}No .bamboo.toml found for this repo. Let's set it up.{RESET}"
    )?;
    writeln!(out, "{DIM}─────────────────────────────────────────────{RESET}")?;
    writeln!(out)?;

    // --- Shell ---
    let default_shell =
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell = prompt(
        &mut out,
        &format!("{BOLD_YELLOW}Shell{RESET} {DIM}[{default_shell}]{RESET}"),
    )?;
    let shell = if shell.is_empty() {
        default_shell
    } else {
        shell
    };

    // --- Layout ---
    let layout_input = prompt(
        &mut out,
        &format!("{BOLD_YELLOW}Layout{RESET} {DIM}(Scroll/Fixed){RESET} {DIM}[Scroll]{RESET}"),
    )?;
    let layout = match layout_input.to_lowercase().as_str() {
        "fixed" => LayoutConfig::Fixed,
        _ => LayoutConfig::Scroll,
    };

    // --- Panes ---
    writeln!(out)?;
    writeln!(
        out,
        "{BOLD_CYAN}Add panes{RESET} {DIM}(leave name blank to finish):{RESET}"
    )?;

    let mut panes: Vec<PaneConfig> = Vec::new();
    let mut idx = 1usize;

    loop {
        let default_name = if idx == 1 { "Shell".to_string() } else { String::new() };
        let name_hint = if default_name.is_empty() {
            format!("  {BOLD_YELLOW}Pane {idx} name{RESET} {DIM}(blank to finish){RESET}")
        } else {
            format!("  {BOLD_YELLOW}Pane {idx} name{RESET} {DIM}[{default_name}]{RESET}")
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
            &format!("  {CYAN}Pane {idx} command{RESET} {DIM}(blank for interactive shell){RESET}"),
        )?;
        let command = if command_input.is_empty() {
            None
        } else {
            Some(command_input)
        };

        let cwd_input = prompt(
            &mut out,
            &format!("  {CYAN}Pane {idx} cwd{RESET} {DIM}(blank for current directory){RESET}"),
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
    writeln!(out, "{DIM}─────────────────────────────────────────────{RESET}")?;
    let save = prompt(
        &mut out,
        &format!("{BOLD}Save config to .bamboo.toml?{RESET} {DIM}[Y/n]{RESET}"),
    )?;
    if save.is_empty() || save.to_lowercase().starts_with('y') {
        write_config(&config)?;
        writeln!(out, "{BOLD_GREEN}Saved .bamboo.toml{RESET}")?;
    } else {
        writeln!(
            out,
            "{YELLOW}Skipped saving — config applies to this session only.{RESET}"
        )?;
    }

    writeln!(out)?;

    Ok(config)
}

fn prompt(out: &mut impl Write, label: &str) -> Result<String> {
    write!(out, "{}{BOLD}{GREEN}>{RESET} ", label)?;
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
