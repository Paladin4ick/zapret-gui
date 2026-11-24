use crate::paths::AppPaths;
use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Flowseal/zapret-discord-youtube/releases/latest";

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    Checking,
    Available(String),
    Downloading(u8),
    Installing,
    Current(String),
    Completed(String),
}

#[derive(Clone, Debug)]
pub struct UpdateResult {
    pub version: String,
    pub changed: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Updater {
    paths: AppPaths,
    client: Client,
}

impl Updater {
    pub fn new(paths: AppPaths) -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("zapret-gui/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .build()
            .context("не удалось создать HTTP-клиент")?;
        Ok(Self { paths, client })
    }

    pub fn installed_version(&self) -> Option<String> {
        let service = fs::read_to_string(self.paths.zapret_dir.join("service.bat")).ok()?;
        let expression = Regex::new(r#"(?im)^set\s+"LOCAL_VERSION=([^"\r\n]+)""#).ok()?;
        expression
            .captures(&service)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().trim().to_string())
    }

    pub fn check_and_update(
        &self,
        force_install: bool,
        mut notify: impl FnMut(UpdateEvent),
    ) -> Result<UpdateResult> {
        notify(UpdateEvent::Checking);
        let release: Release = self
            .client
            .get(LATEST_RELEASE_API)
            .send()
            .context("не удалось проверить релиз Flowseal")?
            .error_for_status()
            .context("GitHub вернул ошибку при проверке релиза")?
            .json()
            .context("не удалось прочитать описание релиза")?;
        let installed = self.installed_version();
        if !force_install && installed.as_deref() == Some(release.tag_name.as_str()) {
            notify(UpdateEvent::Current(release.tag_name.clone()));
            return Ok(UpdateResult {
                version: release.tag_name,
                changed: false,
            });
        }
        notify(UpdateEvent::Available(release.tag_name.clone()));
        let asset = release
            .assets
            .iter()
            .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
            .max_by_key(|asset| {
                let name = asset.name.to_ascii_lowercase();
                usize::from(name.contains("zapret-discord-youtube")) * 2
                    + usize::from(name.contains(&release.tag_name.to_ascii_lowercase()))
            })
            .context("в релизе Flowseal нет ZIP-архива")?;
        let archive_path = self
            .paths
            .app_dir
            .join(format!(".zapret-download-{}.zip", std::process::id()));
        self.download(asset, &archive_path, &mut notify)?;
        verify_digest(&archive_path, asset.digest.as_deref())?;
        notify(UpdateEvent::Installing);
        let result = self.install_archive(&archive_path, &release.tag_name);
        let _ = fs::remove_file(&archive_path);
        result?;
        notify(UpdateEvent::Completed(release.tag_name.clone()));
        Ok(UpdateResult {
            version: release.tag_name,
            changed: true,
        })
    }

    fn download(
        &self,
        asset: &Asset,
        destination: &Path,
        notify: &mut impl FnMut(UpdateEvent),
    ) -> Result<()> {
        let mut response = self
            .client
            .get(&asset.browser_download_url)
            .send()
            .context("не удалось скачать архив Flowseal")?
            .error_for_status()
            .context("GitHub вернул ошибку при скачивании архива")?;
        let expected = response.content_length().unwrap_or(asset.size).max(1);
        let mut file = fs::File::create(destination).context("не удалось создать файл загрузки")?;
        let mut downloaded = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = response
                .read(&mut buffer)
                .context("ошибка чтения загружаемого архива")?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .context("ошибка записи загружаемого архива")?;
            downloaded += count as u64;
            notify(UpdateEvent::Downloading(
                ((downloaded.saturating_mul(100) / expected).min(100)) as u8,
            ));
        }
        file.sync_all()
            .context("не удалось завершить запись архива")?;
        if asset.size > 0 && downloaded != asset.size {
            bail!(
                "размер архива не совпал: ожидалось {}, получено {downloaded}",
                asset.size
            );
        }
        Ok(())
    }

    fn install_archive(&self, archive_path: &Path, version: &str) -> Result<()> {
        let staging = self
            .paths
            .app_dir
            .join(format!(".zapret-staging-{}", std::process::id()));
        if staging.exists() {
            fs::remove_dir_all(&staging)
                .context("не удалось очистить временную папку обновления")?;
        }
        fs::create_dir_all(&staging).context("не удалось создать временную папку обновления")?;
        let install_result = (|| -> Result<()> {
            let file = fs::File::open(archive_path).context("не удалось открыть архив")?;
            let mut archive =
                zip::ZipArchive::new(file).context("загруженный файл не является ZIP")?;
            archive
                .extract(&staging)
                .context("не удалось распаковать архив")?;
            let extracted_root =
                find_distribution_root(&staging).context("в архиве не найден service.bat")?;
            let preserved = collect_preserved_files(&self.paths.zapret_dir)?;
            for (relative, bytes) in preserved {
                let destination = extracted_root.join(relative);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(destination, bytes)?;
            }
            let backup = self
                .paths
                .app_dir
                .join(format!("zapret-backup-{}", sanitize_version(version)));
            if self.paths.zapret_dir.exists() {
                if backup.exists() {
                    bail!("резервная копия уже существует: {}", backup.display());
                }
                fs::rename(&self.paths.zapret_dir, &backup)
                    .context("не удалось создать резервную копию zapret")?;
            }
            if let Err(error) = fs::rename(&extracted_root, &self.paths.zapret_dir) {
                if backup.exists() && !self.paths.zapret_dir.exists() {
                    let _ = fs::rename(&backup, &self.paths.zapret_dir);
                }
                return Err(error).context("не удалось установить новую версию zapret");
            }
            Ok(())
        })();
        let _ = fs::remove_dir_all(&staging);
        install_result
    }
}

fn verify_digest(path: &Path, digest: Option<&str>) -> Result<()> {
    let Some(expected) = digest.and_then(|value| value.strip_prefix("sha256:")) else {
        return Ok(());
    };
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("SHA-256 загруженного архива не совпал")
    }
    Ok(())
}

fn find_distribution_root(root: &Path) -> Option<PathBuf> {
    if root.join("service.bat").is_file() {
        return Some(root.to_path_buf());
    }
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .find(|path| path.join("service.bat").is_file())
}

fn collect_preserved_files(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut relative_paths = vec![
        PathBuf::from("utils/game_filter.enabled"),
        PathBuf::from("bin/ACTIVE_DISCORD_UDP.bin"),
        PathBuf::from("bin/ACTIVE_GAME_UDP.bin"),
    ];
    let lists = root.join("lists");
    if lists.is_dir() {
        for entry in fs::read_dir(lists)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("-user")
            {
                relative_paths.push(PathBuf::from("lists").join(entry.file_name()));
            }
        }
    }
    relative_paths
        .into_iter()
        .filter(|relative| root.join(relative).is_file())
        .map(|relative| {
            fs::read(root.join(&relative))
                .map(|bytes| (relative, bytes))
                .context("не удалось сохранить пользовательский файл перед обновлением")
        })
        .collect()
}

fn sanitize_version(version: &str) -> String {
    version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
