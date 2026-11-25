use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub app_dir: PathBuf,
    pub zapret_dir: PathBuf,
    pub config_file: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let executable =
            std::env::current_exe().context("не удалось определить путь приложения")?;
        let app_dir = executable
            .parent()
            .map(Path::to_path_buf)
            .context("исполняемый файл не имеет родительской папки")?;
        Ok(Self {
            zapret_dir: app_dir.join("zapret"),
            config_file: app_dir.join("zapret-gui.json"),
            logs_dir: app_dir.join("logs"),
            app_dir,
        })
    }

    #[cfg(test)]
    pub fn for_test(app_dir: PathBuf) -> Self {
        Self {
            zapret_dir: app_dir.join("zapret"),
            config_file: app_dir.join("zapret-gui.json"),
            logs_dir: app_dir.join("logs"),
            app_dir,
        }
    }
}
