use anyhow::{Context, Result};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::Read;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::config::{Config, PaneConfig};

#[derive(Debug)]
pub enum PtyEvent {
    Data(#[allow(dead_code)] Vec<u8>),
    Closed,
}

pub struct SpawnedPty {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
    pub reader: Box<dyn Read + Send>,
    #[allow(dead_code)]
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
}

pub fn spawn_pty(
    pane_config: &PaneConfig,
    default_shell: &str,
    cols: u16,
    rows: u16,
) -> Result<SpawnedPty> {
    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to open PTY")?;

    let command_str = pane_config.command.as_deref().unwrap_or(default_shell);
    let mut parts = command_str.split_whitespace();
    let executable = parts.next().unwrap_or(default_shell);
    let mut cmd = CommandBuilder::new(executable);
    for arg in parts {
        cmd.arg(arg);
    }

    let cwd = Config::resolve_cwd(&pane_config.cwd)
        .or_else(|| std::env::current_dir().ok());
    if let Some(cwd) = cwd {
        cmd.cwd(cwd);
    }

    for (k, v) in &pane_config.env {
        cmd.env(k, v);
    }

    let child = pair.slave.spawn_command(cmd).context("Failed to spawn child process")?;
    drop(pair.slave);

    let writer = Arc::new(Mutex::new(
        pair.master
            .take_writer()
            .context("Failed to take PTY writer")?,
    ));

    let reader = pair
        .master
        .try_clone_reader()
        .context("Failed to clone PTY reader")?;

    Ok(SpawnedPty {
        master: pair.master,
        writer,
        reader,
        child,
    })
}

pub fn launch_reader_task(
    mut reader: Box<dyn Read + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    tx: mpsc::UnboundedSender<PtyEvent>,
) {
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(PtyEvent::Closed);
                    break;
                }
                Ok(n) => {
                    let bytes = buf[..n].to_vec();
                    parser.lock().process(&bytes);
                    if tx.send(PtyEvent::Data(bytes)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(PtyEvent::Closed);
                    break;
                }
            }
        }
    });
}
