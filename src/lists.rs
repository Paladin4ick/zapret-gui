use anyhow::{Context, Result, bail};
use std::{fs, path::Path};

#[derive(Clone, Debug)]
pub struct ListFile {
    pub file_name: String,
    pub title: String,
    pub editable: bool,
}

pub fn scan(zapret_dir: &Path) -> Result<Vec<ListFile>> {
    let lists_dir = zapret_dir.join("lists");
    if !lists_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut lists = Vec::new();
    for entry in fs::read_dir(lists_dir).context("не удалось прочитать папку списков")?
    {
        let entry = entry.context("не удалось прочитать файл списка")?;
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
            || path.extension().and_then(|extension| extension.to_str()) != Some("txt")
        {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with(".backup.txt") || file_name.ends_with(".test-backup.txt") {
            continue;
        }
        lists.push(ListFile {
            title: list_title(file_name),
            editable: file_name.contains("-user"),
            file_name: file_name.to_string(),
        });
    }
    lists.sort_by(|left, right| left.title.cmp(&right.title));
    Ok(lists)
}

pub fn read(zapret_dir: &Path, list: &ListFile) -> Result<String> {
    fs::read_to_string(zapret_dir.join("lists").join(&list.file_name))
        .with_context(|| format!("не удалось прочитать {}", list.file_name))
}

pub fn save(zapret_dir: &Path, list: &ListFile, content: &str) -> Result<()> {
    if !list.editable {
        bail!("системный список доступен только для чтения");
    }
    validate_entries(&list.file_name, content)?;
    let path = zapret_dir.join("lists").join(&list.file_name);
    let temporary = path.with_extension("txt.tmp");
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    fs::write(&temporary, normalized).context("не удалось записать временный список")?;
    fs::rename(&temporary, &path).context("не удалось заменить пользовательский список")
}

pub fn ensure_user_lists(zapret_dir: &Path) -> Result<()> {
    let lists = zapret_dir.join("lists");
    if !lists.is_dir() {
        return Ok(());
    }
    for (name, initial) in [
        ("ipset-exclude-user.txt", "203.0.113.113/32\n"),
        (
            "list-general-user.txt",
            "# Never leave this file empty\ndomain.example.abc\n",
        ),
        ("list-exclude-user.txt", "domain.example.abc\n"),
    ] {
        let path = lists.join(name);
        if !path.exists() {
            fs::write(path, initial).with_context(|| format!("не удалось создать {name}"))?;
        }
    }
    Ok(())
}

fn validate_entries(file_name: &str, content: &str) -> Result<()> {
    for (index, raw) in content.lines().enumerate() {
        let value = raw.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        let valid = if file_name.starts_with("ipset") {
            value.chars().all(|character| {
                character.is_ascii_hexdigit() || matches!(character, '.' | ':' | '/' | '-')
            })
        } else {
            !value.contains(char::is_whitespace)
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '.' | '-' | '_' | '*' | ':')
                })
        };
        if !valid {
            bail!("некорректная запись в строке {}: {value}", index + 1);
        }
    }
    Ok(())
}

fn list_title(file_name: &str) -> String {
    match file_name {
        "ipset-all.txt" => "IPSet: основной".into(),
        "ipset-exclude.txt" => "IPSet: исключения".into(),
        "ipset-exclude-user.txt" => "IPSet: пользовательские исключения".into(),
        "list-general.txt" => "Домены: основной список".into(),
        "list-general-user.txt" => "Домены: пользовательский список".into(),
        "list-google.txt" => "Домены: Google".into(),
        "list-exclude.txt" => "Домены: исключения".into(),
        "list-exclude-user.txt" => "Домены: пользовательские исключения".into(),
        other => other.trim_end_matches(".txt").replace('-', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_entries;

    #[test]
    fn validates_domain_and_ipset_entries() {
        assert!(validate_entries("list-general-user.txt", "example.com\n*.cdn.test").is_ok());
        assert!(validate_entries("list-general-user.txt", "bad domain").is_err());
        assert!(validate_entries("ipset-exclude-user.txt", "192.0.2.0/24\n2001:db8::/32").is_ok());
    }
}
