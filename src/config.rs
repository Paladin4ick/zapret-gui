use crate::paths::AppPaths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub selected_strategy: Option<String>,
    pub minimize_to_tray: bool,
    pub auto_start_zapret: bool,
    pub auto_update: bool,
    pub game_filter_mode: String,
    pub game_filter_tcp: String,
    pub game_filter_udp: String,
    pub ipset_mode: String,
    pub discord_fake: Option<String>,
    pub game_fake: Option<String>,
    pub window_width: f32,
    pub window_height: f32,
}

pub fn apply_flowseal_settings(paths: &AppPaths, config: &AppConfig) -> Result<()> {
    if !validate_port_ranges(&config.game_filter_tcp)
        || !validate_port_ranges(&config.game_filter_udp)
    {
        anyhow::bail!("диапазоны портов должны содержать числа 1–65535");
    }
    let utils = paths.zapret_dir.join("utils");
    fs::create_dir_all(&utils).context("не удалось создать папку настроек Flowseal")?;
    fs::write(
        utils.join("game_filter.enabled"),
        format!(
            "mode={}\r\ntcp={}\r\nudp={}\r\n",
            config.game_filter_mode, config.game_filter_tcp, config.game_filter_udp
        ),
    )
    .context("не удалось сохранить настройки фильтра игр")?;
    apply_ipset_mode(paths, &config.ipset_mode)?;
    apply_fake(
        paths,
        config.discord_fake.as_deref(),
        "ACTIVE_DISCORD_UDP.bin",
    )?;
    apply_fake(paths, config.game_fake.as_deref(), "ACTIVE_GAME_UDP.bin")
}

pub fn scan_fake_files(paths: &AppPaths) -> Result<Vec<String>> {
    let bin = paths.zapret_dir.join("bin");
    if !bin.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(bin).context("не удалось прочитать папку fake-файлов")?
    {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if entry.file_type()?.is_file()
            && file_name.to_ascii_lowercase().ends_with(".bin")
            && !file_name.to_ascii_uppercase().starts_with("ACTIVE_")
        {
            files.push(file_name.trim_end_matches(".bin").to_string());
        }
    }
    files.sort();
    Ok(files)
}

pub fn detect_active_fake(
    paths: &AppPaths,
    active_file: &str,
    candidates: &[String],
) -> Option<String> {
    let bin = paths.zapret_dir.join("bin");
    let active = fs::read(bin.join(active_file)).ok()?;
    candidates.iter().find_map(|candidate| {
        let candidate_bytes = fs::read(bin.join(format!("{candidate}.bin"))).ok()?;
        (candidate_bytes == active).then(|| candidate.clone())
    })
}

fn apply_fake(paths: &AppPaths, selected: Option<&str>, active_file: &str) -> Result<()> {
    let Some(selected) = selected else {
        return Ok(());
    };
    if selected.contains(['/', '\\']) || selected.contains("..") {
        anyhow::bail!("некорректное имя fake-файла");
    }
    let bin = paths.zapret_dir.join("bin");
    let source = bin.join(format!("{selected}.bin"));
    if !source.is_file() {
        anyhow::bail!("fake-файл не найден: {selected}");
    }
    fs::copy(source, bin.join(active_file))
        .with_context(|| format!("не удалось заменить {active_file}"))?;
    Ok(())
}

fn apply_ipset_mode(paths: &AppPaths, mode: &str) -> Result<()> {
    let list = paths.zapret_dir.join("lists/ipset-all.txt");
    let backup = paths.zapret_dir.join("lists/ipset-all.txt.backup");
    if !list.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&list).unwrap_or_default();
    let is_loaded = !content.trim().is_empty() && !content.contains("203.0.113.113/32");
    match mode {
        "none" => {
            if is_loaded {
                fs::copy(&list, &backup).context("не удалось создать резервную копию IPSet")?;
            }
            fs::write(&list, "203.0.113.113/32\r\n")?;
        }
        "any" => {
            if is_loaded {
                fs::copy(&list, &backup).context("не удалось создать резервную копию IPSet")?;
            }
            fs::write(&list, "")?;
        }
        _ => {
            if backup.is_file() {
                fs::copy(&backup, &list).context("не удалось восстановить основной IPSet")?;
            }
        }
    }
    Ok(())
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_strategy: None,
            minimize_to_tray: true,
            auto_start_zapret: false,
            auto_update: true,
            game_filter_mode: "disabled".into(),
            game_filter_tcp: "1024-65535".into(),
            game_filter_udp: "1024-65535".into(),
            ipset_mode: "loaded".into(),
            discord_fake: None,
            game_fake: None,
            window_width: 440.0,
            window_height: 368.0,
        }
    }
}

impl AppConfig {
    pub fn load(paths: &AppPaths) -> Self {
        let Ok(data) = fs::read(&paths.config_file) else {
            return Self::default();
        };
        serde_json::from_slice(&data).unwrap_or_default()
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let data = serde_json::to_vec_pretty(self).context("не удалось сериализовать настройки")?;
        let temporary = paths.config_file.with_extension("json.tmp");
        fs::write(&temporary, data).context("не удалось записать временный файл настроек")?;
        fs::rename(&temporary, &paths.config_file)
            .context("не удалось безопасно заменить файл настроек")
    }
}

pub fn validate_port_ranges(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    value.split(',').all(|part| {
        let mut bounds = part.trim().split('-');
        let Some(start) = bounds.next().and_then(|v| v.parse::<u16>().ok()) else {
            return false;
        };
        let end = bounds
            .next()
            .map(str::parse::<u16>)
            .transpose()
            .ok()
            .flatten()
            .unwrap_or(start);
        bounds.next().is_none() && start > 0 && start <= end
    })
}

#[cfg(test)]
mod tests {
    use super::validate_port_ranges;

    #[test]
    fn validates_game_filter_ranges() {
        assert!(validate_port_ranges("1024-1934,1936-65535"));
        assert!(validate_port_ranges("443"));
        assert!(!validate_port_ranges("0-443"));
        assert!(!validate_port_ranges("500-100"));
        assert!(!validate_port_ranges("abc"));
    }
}
