use crate::{paths::AppPaths, process};
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Clone, Debug)]
pub enum TestEvent {
    Progress { current: usize, total: usize },
    Log(String),
}

#[derive(Clone, Debug, Default)]
pub struct AutoSelector {
    cancelled: Arc<AtomicBool>,
    process_id: Arc<AtomicU32>,
}

impl AutoSelector {
    pub fn run(&self, paths: &AppPaths, mut notify: impl FnMut(TestEvent)) -> Result<String> {
        self.cancelled.store(false, Ordering::SeqCst);
        let script = paths.zapret_dir.join("utils/test zapret.ps1");
        if !script.is_file() {
            bail!("оригинальный тестовый сценарий Flowseal не найден");
        }
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .current_dir(&paths.zapret_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .context("не удалось запустить тест Flowseal")?;
        self.process_id.store(child.id(), Ordering::SeqCst);
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(b"2\n1\n\n")
                .context("не удалось передать параметры тесту")?;
        }
        let stdout = child
            .stdout
            .take()
            .context("тест не предоставил поток вывода")?;
        let progress = Regex::new(r"\[(\d+)\s*/\s*(\d+)\]")?;
        let best = Regex::new(r"(?i)Best (?:config|strategy):\s*(.+?)\s*$")?;
        let mut best_strategy = None;
        for line in BufReader::new(stdout).lines() {
            if self.cancelled.load(Ordering::SeqCst) {
                let _ = process::kill_process_tree(child.id());
                self.process_id.store(0, Ordering::SeqCst);
                bail!("тестирование отменено");
            }
            let line = line.context("не удалось прочитать вывод теста")?;
            if let Some(captures) = progress.captures(&line) {
                let current = captures[1].parse().unwrap_or_default();
                let total = captures[2].parse().unwrap_or_default();
                notify(TestEvent::Progress { current, total });
            }
            if let Some(captures) = best.captures(&line) {
                best_strategy = Some(captures[1].trim().to_string());
            }
            notify(TestEvent::Log(line));
        }
        let status = child
            .wait()
            .context("не удалось дождаться завершения теста")?;
        self.process_id.store(0, Ordering::SeqCst);
        if self.cancelled.load(Ordering::SeqCst) {
            bail!("тестирование отменено");
        }
        if !status.success() {
            bail!("тест Flowseal завершился с кодом {status}");
        }
        best_strategy.context("тест не сообщил лучшую стратегию")
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let pid = self.process_id.swap(0, Ordering::SeqCst);
        if pid != 0 {
            let _ = process::kill_process_tree(pid);
        }
    }
}
