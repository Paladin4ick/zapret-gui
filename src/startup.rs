use crate::process::hide_window;
use anyhow::{Context, Result, bail};
use std::{path::Path, process::Command};

const TASK_NAME: &str = "Paladin4ick zapret GUI";

pub fn sync(enabled: bool, executable: &Path) -> Result<()> {
    if enabled {
        create_task(executable)
    } else {
        remove_task()
    }
}

fn create_task(executable: &Path) -> Result<()> {
    let action = format!("\"{}\" --autostart", executable.display());
    let mut command = Command::new("schtasks.exe");
    command.args([
        "/Create", "/TN", TASK_NAME, "/TR", &action, "/SC", "ONLOGON", "/RL", "HIGHEST", "/IT",
        "/F",
    ]);
    run(&mut command, "не удалось настроить автозапуск приложения")
}

fn remove_task() -> Result<()> {
    if !task_exists() {
        return Ok(());
    }
    let mut command = Command::new("schtasks.exe");
    command.args(["/Delete", "/TN", TASK_NAME, "/F"]);
    run(&mut command, "не удалось отключить автозапуск приложения")
}

fn task_exists() -> bool {
    let mut command = Command::new("schtasks.exe");
    command.args(["/Query", "/TN", TASK_NAME]);
    hide_window(&mut command);
    command.status().is_ok_and(|status| status.success())
}

fn run(command: &mut Command, context: &str) -> Result<()> {
    hide_window(command);
    let output = command.output().with_context(|| context.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let details = if output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout)
    } else {
        String::from_utf8_lossy(&output.stderr)
    };
    let details = details.trim();
    if details.is_empty() {
        bail!("{context}: {}", output.status)
    }
    bail!("{context}: {details}")
}
