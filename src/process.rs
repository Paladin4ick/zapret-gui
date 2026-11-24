use crate::{paths::AppPaths, strategy::Strategy};
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, OpenOptions},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct ProcessManager {
    paths: AppPaths,
}

impl ProcessManager {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn is_running(&self) -> bool {
        process_exists("winws.exe")
    }

    pub fn start(&self, strategy: &Strategy) -> Result<()> {
        if self.is_running() {
            bail!("zapret уже запущен");
        }
        let script = self.paths.zapret_dir.join(&strategy.file_name);
        if !script.is_file() {
            bail!("файл стратегии не найден: {}", strategy.file_name);
        }
        let log_path = self.paths.app_dir.join("zapret-launch.log");
        let stdout = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
            .context("не удалось открыть журнал запуска zapret")?;
        let stderr = stdout
            .try_clone()
            .context("не удалось подготовить журнал ошибок zapret")?;
        let mut command = Command::new("cmd.exe");
        command
            .arg("/d")
            .arg("/s")
            .arg("/c")
            .arg(format!("\"{}\"", script.display()))
            .current_dir(&self.paths.zapret_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        hide_window(&mut command);
        let mut child = command.spawn().context("не удалось запустить стратегию")?;
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut early_exit = None;
        while Instant::now() < deadline {
            if self.is_running() {
                return Ok(());
            }
            if early_exit.is_none() {
                early_exit = child
                    .try_wait()
                    .context("не удалось проверить запуск стратегии")?;
            }
            thread::sleep(Duration::from_millis(200));
        }
        let details = fs::read_to_string(&log_path).unwrap_or_default();
        if let Some(status) = early_exit {
            bail!(
                "сценарий стратегии завершился с кодом {status}\n{}",
                details.trim()
            );
        }
        bail!(
            "winws.exe не появился после запуска стратегии\n{}",
            details.trim()
        )
    }

    pub fn stop(&self) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }
        let mut command = Command::new("taskkill.exe");
        command.args(["/F", "/T", "/IM", "winws.exe"]);
        hide_window(&mut command);
        let status = command
            .status()
            .context("не удалось остановить winws.exe")?;
        if !status.success() && self.is_running() {
            bail!("taskkill не смог остановить winws.exe");
        }
        Ok(())
    }

    pub fn restart(&self, strategy: &Strategy) -> Result<()> {
        self.stop()?;
        self.start(strategy)
    }
}

pub fn process_exists(image_name: &str) -> bool {
    let mut command = Command::new("tasklist.exe");
    command.args(["/NH", "/FI", &format!("IMAGENAME eq {image_name}")]);
    hide_window(&mut command);
    command
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains(&image_name.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

pub fn kill_process_tree(pid: u32) -> Result<()> {
    let mut command = Command::new("taskkill.exe");
    command.args(["/F", "/T", "/PID", &pid.to_string()]);
    hide_window(&mut command);
    let _ = command.status();
    Ok(())
}

pub fn run_hidden(program: &Path, arguments: &[&str], working_dir: &Path) -> Result<()> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_window(&mut command);
    command.spawn().context("не удалось запустить процесс")?;
    Ok(())
}

pub fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}
