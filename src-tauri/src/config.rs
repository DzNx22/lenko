//! Preferências do app em `app_config_dir/config.json`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeCode {
    Connected,
    Declined,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Resposta ao "Quer que eu me conecte ao Claude Code?". `None` = ainda não perguntou.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<ClaudeCode>,
    /// Sons desligados (bandeja ou botão ♪).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
}

fn file(dir: &Path) -> PathBuf {
    dir.join("config.json")
}

/// Arquivo ausente ou corrompido vira configuração padrão.
pub fn load(dir: &Path) -> Config {
    fs::read_to_string(file(dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, cfg: &Config) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(file(dir), serde_json::to_string_pretty(cfg)? + "\n")
}
