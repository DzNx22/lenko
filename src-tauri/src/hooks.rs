//! Conecta e desconecta o LENKO do Claude Code editando `~/.claude/settings.json`.
//!
//! Porte de `scripts/install-hooks.js`: mexe só nas entradas marcadas com `--preguica`,
//! preserva hooks de terceiros e faz backup em `settings.json.bak-preguica`.
//! A marca mantém o nome antigo para reconhecer entradas de quem instalou antes.

use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const MARK: &str = "--preguica";
const BACKUP_SUFFIX: &str = ".bak-preguica";

/// Hook do Claude Code → estado do mascote.
pub const EVENTS: [(&str, &str); 3] = [
    ("UserPromptSubmit", "working"), // você mandou um prompt → acordou
    ("Stop", "done"),                // terminou a resposta
    ("Notification", "waiting"),     // pediu permissão / está esperando você
];

#[derive(Debug, PartialEq)]
pub enum Status {
    /// Nenhuma entrada do LENKO.
    NotConnected,
    /// As três entradas existem e apontam para este executável.
    Connected,
    /// Há entradas do LENKO, mas com outro caminho ou incompletas.
    Outdated,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// O settings.json existe mas não é um JSON no formato esperado. Nada foi alterado.
    InvalidJson,
    Io(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::InvalidJson => write!(
                f,
                "O settings.json do Claude Code está com erro. Corrija o arquivo e tente de novo."
            ),
            Error::Io(e) => write!(f, "Não consegui acessar o settings.json: {e}"),
        }
    }
}

/// Caminho padrão: `~/.claude/settings.json`.
pub fn settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

/// Comando do hook apontando para o executável (caminho absoluto, entre aspas).
/// Usa barras normais: funcionam no Windows e não viram escape no Git Bash,
/// que é o shell usado pelo Claude Code para rodar hooks no Windows.
pub fn command_for(exe: &Path, kind: &str) -> String {
    let exe = exe.to_string_lossy();
    let exe = exe.strip_prefix(r"\\?\").unwrap_or(&exe).replace('\\', "/");
    format!("\"{exe}\" --notify {kind} --tool claude-code {MARK}")
}

// ---------------------------------------------------------------------------
// Funções puras (testáveis): recebem o conteúdo do arquivo e devolvem o novo.
// ---------------------------------------------------------------------------

fn parse(text: Option<&str>) -> Result<Map<String, Value>, Error> {
    let Some(text) = text else { return Ok(Map::new()) };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => validate(m),
        _ => Err(Error::InvalidJson),
    }
}

/// Garante que `hooks` e as listas que vamos mexer têm o formato esperado.
fn validate(m: Map<String, Value>) -> Result<Map<String, Value>, Error> {
    match m.get("hooks") {
        None => {}
        Some(Value::Object(h)) => {
            for (event, _) in EVENTS {
                if !matches!(h.get(event), None | Some(Value::Array(_))) {
                    return Err(Error::InvalidJson);
                }
            }
        }
        Some(_) => return Err(Error::InvalidJson),
    }
    Ok(m)
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(MARK))
}

/// Instala (`exe = Some`) ou remove (`exe = None`) as entradas do LENKO.
/// Grupos de terceiros ficam intactos; grupos que só tinham entradas nossas somem.
pub fn merge(text: Option<&str>, exe: Option<&Path>) -> Result<String, Error> {
    let mut settings = parse(text)?;
    // Edita no lugar (entry) para não mudar a ordem das chaves do usuário
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("validado em parse");

    for (event, kind) in EVENTS {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("validado em parse");
        groups.retain_mut(|g| {
            let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = list.len();
            list.retain(|h| !is_ours(h));
            !(list.is_empty() && before > 0)
        });
        if let Some(exe) = exe {
            groups.push(json!({ "hooks": [{ "type": "command", "command": command_for(exe, kind) }] }));
        }
        if groups.is_empty() {
            hooks.shift_remove(event);
        }
    }
    if hooks.is_empty() {
        settings.shift_remove("hooks");
    }

    let mut out = serde_json::to_string_pretty(&Value::Object(settings)).expect("JSON serializável");
    out.push('\n');
    Ok(out)
}

/// Diz se o settings.json aponta para este executável.
pub fn status(text: Option<&str>, exe: &Path) -> Result<Status, Error> {
    let settings = parse(text)?;
    let hooks = settings.get("hooks").and_then(Value::as_object);
    let mut found_any = false;
    let mut all_ok = true;

    for (event, kind) in EVENTS {
        let ours: Vec<&str> = hooks
            .and_then(|h| h.get(event))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.get("hooks").and_then(Value::as_array))
            .flatten()
            .filter(|h| is_ours(h))
            .filter_map(|h| h.get("command").and_then(Value::as_str))
            .collect();
        found_any |= !ours.is_empty();
        all_ok &= ours == [command_for(exe, kind).as_str()];
    }
    Ok(match (found_any, all_ok) {
        (false, _) => Status::NotConnected,
        (true, true) => Status::Connected,
        (true, false) => Status::Outdated,
    })
}

// ---------------------------------------------------------------------------
// Acesso ao arquivo.
// ---------------------------------------------------------------------------

fn read(path: &Path) -> Result<Option<String>, Error> {
    match fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e.to_string())),
    }
}

fn write(path: &Path, exe: Option<&Path>) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::Io(e.to_string());
    let old = read(path)?;
    let new = merge(old.as_deref(), exe)?; // JSON inválido para aqui, sem tocar em nada
    if let Some(old) = &old {
        let mut backup = path.as_os_str().to_owned();
        backup.push(BACKUP_SUFFIX);
        fs::write(backup, old).map_err(io)?;
    } else if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    fs::write(path, new).map_err(io)
}

/// Conecta (ou corrige o caminho) apontando os hooks para `exe`.
pub fn connect(settings: &Path, exe: &Path) -> Result<(), Error> {
    write(settings, Some(exe))
}

/// Remove só as entradas do LENKO.
pub fn disconnect(settings: &Path) -> Result<(), Error> {
    write(settings, None)
}

pub fn read_status(settings: &Path, exe: &Path) -> Result<Status, Error> {
    status(read(settings)?.as_deref(), exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Program Files\LENKO\lenko.exe";

    fn exe() -> &'static Path {
        Path::new(EXE)
    }

    fn v(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    const THIRD_PARTY: &str = r#"{
      "theme": "dark",
      "hooks": {
        "Stop": [{ "hooks": [{ "type": "command", "command": "outro-app --avisa" }] }],
        "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "lint" }] }]
      }
    }"#;

    #[test]
    fn comando_aponta_para_o_executavel() {
        assert_eq!(
            command_for(exe(), "done"),
            r#""C:/Program Files/LENKO/lenko.exe" --notify done --tool claude-code --preguica"#
        );
    }

    #[test]
    fn arquivo_inexistente_cria_os_tres_hooks() {
        let out = v(&merge(None, Some(exe())).unwrap());
        for (event, kind) in EVENTS {
            assert_eq!(out["hooks"][event][0]["hooks"][0]["command"], command_for(exe(), kind));
        }
        assert_eq!(status(Some(&out.to_string()), exe()), Ok(Status::Connected));
    }

    #[test]
    fn preserva_hooks_e_configuracoes_de_terceiros() {
        let out = v(&merge(Some(THIRD_PARTY), Some(exe())).unwrap());
        assert_eq!(out["theme"], "dark");
        assert_eq!(out["hooks"]["PreToolUse"], v(THIRD_PARTY)["hooks"]["PreToolUse"]);
        let stop = out["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "outro-app --avisa");
    }

    #[test]
    fn reinstalar_nao_duplica_e_corrige_o_caminho() {
        let old_exe = Path::new(r"C:\versao-antiga\lenko.exe");
        let first = merge(Some(THIRD_PARTY), Some(old_exe)).unwrap();
        assert_eq!(status(Some(&first), exe()), Ok(Status::Outdated));

        let second = merge(Some(&first), Some(exe())).unwrap();
        let third = merge(Some(&second), Some(exe())).unwrap();
        assert_eq!(second, third);
        assert_eq!(v(&third)["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(!third.contains("versao-antiga"));
        assert_eq!(status(Some(&third), exe()), Ok(Status::Connected));
    }

    #[test]
    fn reconhece_instalacao_feita_pelo_script_node() {
        let node = r#"{ "hooks": { "Stop": [{ "hooks": [{ "type": "command",
            "command": "node \"C:/dev/notify.js\" done claude-code --preguica" }] }] } }"#;
        assert_eq!(status(Some(node), exe()), Ok(Status::Outdated));
        let out = merge(Some(node), Some(exe())).unwrap();
        assert!(!out.contains("notify.js"));
    }

    #[test]
    fn desconectar_remove_so_o_que_e_nosso() {
        let connected = merge(Some(THIRD_PARTY), Some(exe())).unwrap();
        let out = v(&merge(Some(&connected), None).unwrap());
        assert_eq!(out, v(THIRD_PARTY));
        assert_eq!(status(Some(&out.to_string()), exe()), Ok(Status::NotConnected));
    }

    #[test]
    fn desconectar_sem_terceiros_remove_a_chave_hooks() {
        let connected = merge(Some(r#"{"theme":"dark"}"#), Some(exe())).unwrap();
        assert_eq!(v(&merge(Some(&connected), None).unwrap()), json!({"theme":"dark"}));
    }

    #[test]
    fn mantem_a_ordem_das_chaves_do_usuario() {
        let out = merge(Some(r#"{"zeta":1,"alfa":2}"#), Some(exe())).unwrap();
        assert!(out.find("zeta").unwrap() < out.find("alfa").unwrap());
    }

    #[test]
    fn json_invalido_nao_mexe_em_nada() {
        for bad in ["{ isso não é json", "[1, 2]", r#"{"hooks": 3}"#, r#"{"hooks": {"Stop": {}}}"#] {
            assert_eq!(merge(Some(bad), Some(exe())), Err(Error::InvalidJson), "{bad}");
            assert_eq!(merge(Some(bad), None), Err(Error::InvalidJson), "{bad}");
        }
    }

    #[test]
    fn arquivo_real_backup_e_json_invalido() {
        let dir = std::env::temp_dir().join(format!("lenko-teste-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let settings = settings_path(&dir);

        // Inexistente: cria a pasta e o arquivo, sem backup
        connect(&settings, exe()).unwrap();
        assert_eq!(read_status(&settings, exe()), Ok(Status::Connected));
        let backup = dir.join(".claude").join("settings.json.bak-preguica");
        assert!(!backup.exists());

        // Desconectar faz backup do conteúdo anterior
        let before = fs::read_to_string(&settings).unwrap();
        disconnect(&settings).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), before);
        assert_eq!(read_status(&settings, exe()), Ok(Status::NotConnected));

        // JSON inválido: erro, arquivo e backup intactos
        fs::write(&settings, "{ quebrado").unwrap();
        let backup_before = fs::read_to_string(&backup).unwrap();
        assert_eq!(connect(&settings, exe()), Err(Error::InvalidJson));
        assert_eq!(fs::read_to_string(&settings).unwrap(), "{ quebrado");
        assert_eq!(fs::read_to_string(&backup).unwrap(), backup_before);

        fs::remove_dir_all(&dir).unwrap();
    }
}
