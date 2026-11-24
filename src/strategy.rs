use anyhow::{Context, Result};
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
