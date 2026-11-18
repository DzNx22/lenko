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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Resposta ao "Quer que eu me conecte ao Claude Code?". `None` = ainda não perguntou.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<ClaudeCode>,
    /// Sons desligados (bandeja ou botão ♪).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
    /// Pixels de tela por pixel do sprite (ver `SCALES`).
    #[serde(default = "default_scale")]
    pub scale: u8,
    /// Id do mascote escolhido. `None` = preguiça padrão.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
}

/// Tamanhos oferecidos na bandeja. Só escalas inteiras, para o pixel art não borrar.
pub const SCALES: [(u8, &str); 3] = [(3, "Pequeno"), (4, "Médio"), (5, "Grande")];

fn default_scale() -> u8 {
    5
}

impl Default for Config {
    fn default() -> Self {
        Config { claude_code: None, muted: false, scale: default_scale(), skin: None }
    }
}

impl Config {
    /// Escala válida mesmo se o arquivo foi editado à mão.
    pub fn scale(&self) -> u8 {
        if SCALES.iter().any(|(s, _)| *s == self.scale) { self.scale } else { default_scale() }
    }
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
