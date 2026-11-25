use anyhow::{Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_SIZE: u64 = 2 * 1024 * 1024;
const BACKUP_COUNT: usize = 5;
static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn initialize(logs_dir: &Path) -> Result<()> {
    fs::create_dir_all(logs_dir).context("не удалось создать папку logs")
}

pub fn write(logs_dir: &Path, category: &str, message: impl AsRef<str>) {
    let _ = write_inner(logs_dir, category, message.as_ref());
}

pub fn prepare_file(path: &Path) -> Result<u64> {
    let _guard = lock();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("не удалось создать папку для журнала")?;
    }
    rotate_if_needed(path)?;
    Ok(fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0))
}

fn write_inner(logs_dir: &Path, category: &str, message: &str) -> Result<()> {
    let _guard = lock();
    fs::create_dir_all(logs_dir).context("не удалось создать папку logs")?;
    let path = logs_dir.join("app.log");
    rotate_if_needed(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .context("не удалось открыть app.log")?;
    for line in message.lines().filter(|line| !line.is_empty()) {
        writeln!(file, "[{}] [{category}] {line}", timestamp())?;
    }
    Ok(())
}

fn rotate_if_needed(path: &Path) -> Result<()> {
    if fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        < MAX_LOG_SIZE
    {
        return Ok(());
    }
    for index in (1..=BACKUP_COUNT).rev() {
        let source = backup_path(path, index - 1);
        if !source.exists() {
            continue;
        }
        let destination = backup_path(path, index);
        if destination.exists() {
            fs::remove_file(&destination)
                .with_context(|| format!("не удалось удалить {}", destination.display()))?;
        }
        fs::rename(&source, &destination)
            .with_context(|| format!("не удалось ротировать {}", source.display()))?;
    }
    Ok(())
}

fn backup_path(path: &Path, index: usize) -> std::path::PathBuf {
    if index == 0 {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("log");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("log");
    path.with_file_name(format!("{stem}.{index}.{extension}"))
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::backup_path;
    use std::path::Path;

    #[test]
    fn creates_numbered_backup_names() {
        assert_eq!(
            backup_path(Path::new("logs/app.log"), 2),
            Path::new("logs/app.2.log")
        );
    }
}
