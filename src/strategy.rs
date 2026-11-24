use anyhow::{Context, Result};
use regex::Regex;
use std::{fs, path::Path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Strategy {
    pub name: String,
    pub file_name: String,
}

pub fn scan(zapret_dir: &Path) -> Result<Vec<Strategy>> {
    if !zapret_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut strategies = Vec::new();
    for entry in fs::read_dir(zapret_dir).context("не удалось просканировать папку zapret")?
    {
        let entry = entry.context("не удалось прочитать элемент папки zapret")?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let lower = file_name.to_ascii_lowercase();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
            || !lower.starts_with("general")
            || !lower.ends_with(".bat")
        {
            continue;
        }
        let name = file_name
            .strip_suffix(".bat")
            .unwrap_or(file_name)
            .trim()
            .to_string();
        strategies.push(Strategy {
            name,
            file_name: file_name.to_string(),
        });
    }
    strategies.sort_by_key(|strategy| natural_key(&strategy.name));
    Ok(strategies)
}

pub fn prepare_background_launch(zapret_dir: &Path) -> Result<usize> {
    if !zapret_dir.is_dir() {
        return Ok(0);
    }
    let start_pattern =
        Regex::new(r#"(?im)^start\s+("zapret:[^"]*")\s+/min\s+("%BIN%winws\.exe")"#)?;
    let mut changed = 0;
    for strategy in scan(zapret_dir)? {
        let path = zapret_dir.join(strategy.file_name);
        let source = fs::read_to_string(&path)
            .with_context(|| format!("не удалось прочитать {}", path.display()))?;
        let patched = start_pattern.replace_all(&source, "start $1 /b /min $2");
        if patched != source {
            fs::write(&path, patched.as_bytes())
                .with_context(|| format!("не удалось подготовить {}", path.display()))?;
            changed += 1;
        }
    }
    let test_script = zapret_dir.join("utils/test zapret.ps1");
    if test_script.is_file() {
        let source = fs::read_to_string(&test_script)
            .context("не удалось прочитать тестовый сценарий Flowseal")?;
        let patched = source.replace("-WindowStyle Minimized", "-WindowStyle Hidden");
        if patched != source {
            fs::write(&test_script, patched.as_bytes())
                .context("не удалось подготовить фоновый запуск тестов")?;
            changed += 1;
        }
    }
    Ok(changed)
}

fn natural_key(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 8);
    let mut digits = String::new();
    for character in value.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
        } else {
            if !digits.is_empty() {
                result.push_str(&format!("{:08}", digits.parse::<u32>().unwrap_or_default()));
                digits.clear();
            }
            result.push(character.to_ascii_lowercase());
        }
    }
    if !digits.is_empty() {
        result.push_str(&format!("{:08}", digits.parse::<u32>().unwrap_or_default()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::natural_key;

    #[test]
    fn natural_order_keeps_alt_numbers_ordered() {
        assert!(natural_key("ALT9") < natural_key("ALT10"));
    }
}
