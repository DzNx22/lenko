//! Conecta e desconecta o LENKO das ferramentas de IA de código editando o arquivo
//! de hooks de cada uma (Claude Code, Codex, Gemini CLI, Cursor).
//!
//! Mexe só nas entradas marcadas com `--preguica`, preserva hooks de terceiros e faz
//! backup em `<arquivo>.bak-preguica`. A marca mantém o nome antigo para reconhecer
//! entradas de quem instalou antes.

use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const MARK: &str = "--preguica";
const BACKUP_SUFFIX: &str = ".bak-preguica";

/// Formato do arquivo de hooks.
#[derive(Debug, PartialEq)]
pub enum Layout {
    /// `{"hooks": {"Evento": [{"hooks": [{"type": "command", "command": "..."}]}]}}`
    /// (Claude Code, Codex, Gemini CLI)
    Nested,
    /// `{"version": 1, "hooks": {"evento": [{"command": "..."}]}}` (Cursor)
    Flat,
}

/// Shell em que a ferramenta roda o comando do hook.
#[derive(Debug, PartialEq)]
pub enum Shell {
    /// Git Bash no Windows (Claude Code): caminho entre aspas funciona.
    Bash,
    /// PowerShell ou cmd no Windows, conforme a versão. No PowerShell, um comando que
    /// começa com aspas não executa; por isso usamos o nome curto (8.3), sem espaços.
    Any,
}

pub struct Tool {
    /// Vai no `--tool` do evento.
    pub id: &'static str,
    pub name: &'static str,
    /// Pasta na home onde a ferramenta guarda a configuração.
    pub dir: &'static str,
    pub file: &'static str,
    pub layout: Layout,
    pub shell: Shell,
    /// Hook da ferramenta → estado do mascote.
    pub events: &'static [(&'static str, &'static str)],
    /// Balão depois de conectar.
    pub connected_text: &'static str,
}

pub const CLAUDE: Tool = Tool {
    id: "claude-code",
    name: "Claude Code",
    dir: ".claude",
    file: "settings.json",
    layout: Layout::Nested,
    shell: Shell::Bash,
    events: &[
        ("UserPromptSubmit", "working"), // você mandou um prompt → acordou
        ("PostToolUse", "working"),      // usou uma ferramenta → você aprovou, voltou a trabalhar
        ("Stop", "done"),                // terminou a resposta
        ("Notification", "waiting"),     // pediu permissão / está esperando você
    ],
    connected_text: "Conectado! Reinicie o Claude Code para valer.",
};

pub const CODEX: Tool = Tool {
    id: "codex",
    name: "Codex",
    dir: ".codex",
    file: "hooks.json",
    layout: Layout::Nested,
    shell: Shell::Any,
    events: &[
        ("UserPromptSubmit", "working"),
        ("PostToolUse", "working"),
        ("Stop", "done"),
        ("PermissionRequest", "waiting"),
    ],
    connected_text: "Conectado! No Codex, aprove os hooks novos em /hooks.",
};

pub const GEMINI: Tool = Tool {
    id: "gemini-cli",
    name: "Gemini CLI",
    dir: ".gemini",
    file: "settings.json",
    layout: Layout::Nested,
    shell: Shell::Any,
    events: &[
        ("BeforeAgent", "working"),
        ("AfterTool", "working"),
        ("AfterAgent", "done"),
        ("Notification", "waiting"),
    ],
    connected_text: "Conectado! Reinicie o Gemini CLI para valer.",
};

/// O Cursor não tem hook de "pediu aprovação", então a preguiça só acorda e avisa o fim.
pub const CURSOR: Tool = Tool {
    id: "cursor",
    name: "Cursor",
    dir: ".cursor",
    file: "hooks.json",
    layout: Layout::Flat,
    shell: Shell::Any,
    events: &[
        ("beforeSubmitPrompt", "working"),
        ("postToolUse", "working"),
        ("stop", "done"),
    ],
    connected_text: "Conectado ao Cursor! Ele recarrega os hooks sozinho.",
};

pub const TOOLS: [&Tool; 4] = [&CLAUDE, &CODEX, &GEMINI, &CURSOR];

pub fn find(id: &str) -> Option<&'static Tool> {
    TOOLS.into_iter().find(|t| t.id == id)
}

#[derive(Debug, PartialEq)]
pub enum Status {
    /// Nenhuma entrada do LENKO.
    NotConnected,
    /// Todas as entradas existem e apontam para este executável.
    Connected,
    /// Há entradas do LENKO, mas com outro caminho ou incompletas.
    Outdated,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// O arquivo existe mas não é um JSON no formato esperado. Nada foi alterado.
    InvalidJson(&'static str),
    Io(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::InvalidJson(file) => write!(
                f,
                "O arquivo {file} está com erro (ou tem comentários). Corrija e tente de novo."
            ),
            Error::Io(e) => write!(f, "Não consegui acessar o arquivo de hooks: {e}"),
        }
    }
}

/// Caminho do arquivo de hooks, ex.: `~/.claude/settings.json`.
pub fn settings_path(tool: &Tool, home: &Path) -> PathBuf {
    home.join(tool.dir).join(tool.file)
}

/// A ferramenta parece instalada (a pasta de configuração existe).
pub fn detected(tool: &Tool, home: &Path) -> bool {
    home.join(tool.dir).is_dir()
}

/// Comando do hook apontando para o executável. Usa barras normais: funcionam no
/// Windows e não viram escape no bash.
pub fn command_for(tool: &Tool, exe: &Path, kind: &str) -> String {
    let args = format!("--notify {kind} --tool {} {MARK}", tool.id);
    let exe = exe.to_string_lossy();
    let exe = exe.strip_prefix(r"\\?\").unwrap_or(&exe);
    match tool.shell {
        Shell::Bash => format!("\"{}\" {args}", exe.replace('\\', "/")),
        Shell::Any => format!("{} {args}", any_shell(&short_path(exe))),
    }
}

/// Caminho que roda igual em cmd, PowerShell e bash: sem espaços, vai sem aspas.
/// Com espaços (sem nome curto disponível), aspas: funciona em cmd e bash.
fn any_shell(path: &str) -> String {
    let path = path.replace('\\', "/");
    if path.contains(' ') { format!("\"{path}\"") } else { path }
}

/// Nome curto 8.3 do Windows (`C:\Users\ANALIS~1\...`), que não tem espaços.
/// Se o volume não gera nomes curtos ou o arquivo não existe, devolve o original.
#[cfg(windows)]
fn short_path(path: &str) -> String {
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let mut buf = vec![0u16; 1024];
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if len == 0 || len >= buf.len() {
        return path.to_string();
    }
    String::from_utf16_lossy(&buf[..len])
}

#[cfg(not(windows))]
fn short_path(path: &str) -> String {
    path.to_string()
}

// ---------------------------------------------------------------------------
// Funções puras (testáveis): recebem o conteúdo do arquivo e devolvem o novo.
// ---------------------------------------------------------------------------

fn parse(tool: &Tool, text: Option<&str>) -> Result<Map<String, Value>, Error> {
    let Some(text) = text else { return Ok(Map::new()) };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => validate(tool, m),
        _ => Err(Error::InvalidJson(tool.file)),
    }
}

/// Garante que `hooks` e as listas que vamos mexer têm o formato esperado.
fn validate(tool: &Tool, m: Map<String, Value>) -> Result<Map<String, Value>, Error> {
    match m.get("hooks") {
        None => {}
        Some(Value::Object(h)) => {
            for (event, _) in tool.events {
                if !matches!(h.get(*event), None | Some(Value::Array(_))) {
                    return Err(Error::InvalidJson(tool.file));
                }
            }
        }
        Some(_) => return Err(Error::InvalidJson(tool.file)),
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
pub fn merge(tool: &Tool, text: Option<&str>, exe: Option<&Path>) -> Result<String, Error> {
    let mut settings = parse(tool, text)?;
    if tool.layout == Layout::Flat && exe.is_some() && !settings.contains_key("version") {
        settings.insert("version".into(), json!(1));
    }
    // Edita no lugar (entry) para não mudar a ordem das chaves do usuário
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("validado em parse");

    for (event, kind) in tool.events {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("validado em parse");
        match tool.layout {
            Layout::Nested => groups.retain_mut(|g| {
                let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) else {
                    return true;
                };
                let before = list.len();
                list.retain(|h| !is_ours(h));
                !(list.is_empty() && before > 0)
            }),
            Layout::Flat => groups.retain(|h| !is_ours(h)),
        }
        if let Some(exe) = exe {
            let command = command_for(tool, exe, kind);
            groups.push(match tool.layout {
                Layout::Nested => json!({ "hooks": [{ "type": "command", "command": command }] }),
                Layout::Flat => json!({ "command": command }),
            });
        }
        if groups.is_empty() {
            hooks.shift_remove(*event);
        }
    }
    if hooks.is_empty() {
        settings.shift_remove("hooks");
        // O "version" que nós mesmos criamos não fica sozinho no arquivo
        if tool.layout == Layout::Flat && settings.len() == 1 && settings.get("version") == Some(&json!(1)) {
            settings.shift_remove("version");
        }
    }

    let mut out = serde_json::to_string_pretty(&Value::Object(settings)).expect("JSON serializável");
    out.push('\n');
    Ok(out)
}

/// Comandos nossos registrados para um evento.
fn ours<'a>(tool: &Tool, hooks: Option<&'a Map<String, Value>>, event: &str) -> Vec<&'a str> {
    let entries = hooks.and_then(|h| h.get(event)).and_then(Value::as_array).into_iter().flatten();
    let commands: Vec<&Value> = match tool.layout {
        Layout::Nested => entries
            .filter_map(|g| g.get("hooks").and_then(Value::as_array))
            .flatten()
            .collect(),
        Layout::Flat => entries.collect(),
    };
    commands
        .into_iter()
        .filter(|h| is_ours(h))
        .filter_map(|h| h.get("command").and_then(Value::as_str))
        .collect()
}

/// Diz se o arquivo aponta para este executável.
pub fn status(tool: &Tool, text: Option<&str>, exe: &Path) -> Result<Status, Error> {
    let settings = parse(tool, text)?;
    let hooks = settings.get("hooks").and_then(Value::as_object);
    let mut found_any = false;
    let mut all_ok = true;

    for (event, kind) in tool.events {
        let ours = ours(tool, hooks, event);
        found_any |= !ours.is_empty();
        all_ok &= ours == [command_for(tool, exe, kind).as_str()];
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

fn write(tool: &Tool, path: &Path, exe: Option<&Path>) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::Io(e.to_string());
    let old = read(path)?;
    let new = merge(tool, old.as_deref(), exe)?; // JSON inválido para aqui, sem tocar em nada
    if let Some(old) = &old {
        let mut backup = path.as_os_str().to_owned();
        backup.push(BACKUP_SUFFIX);
        fs::write(backup, old).map_err(io)?;
    } else if exe.is_none() {
        return Ok(()); // desconectar sem arquivo: nada a fazer
    } else if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    fs::write(path, new).map_err(io)
}

/// Conecta (ou corrige o caminho) apontando os hooks para `exe`.
pub fn connect(tool: &Tool, settings: &Path, exe: &Path) -> Result<(), Error> {
    write(tool, settings, Some(exe))
}

/// Remove só as entradas do LENKO.
pub fn disconnect(tool: &Tool, settings: &Path) -> Result<(), Error> {
    write(tool, settings, None)
}

pub fn read_status(tool: &Tool, settings: &Path, exe: &Path) -> Result<Status, Error> {
    status(tool, read(settings)?.as_deref(), exe)
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
            command_for(&CLAUDE, exe(), "done"),
            r#""C:/Program Files/LENKO/lenko.exe" --notify done --tool claude-code --preguica"#
        );
        // Sem espaços: sem aspas, roda igual em cmd, PowerShell e bash
        assert_eq!(
            command_for(&CODEX, Path::new(r"C:\LENKO\lenko.exe"), "done"),
            "C:/LENKO/lenko.exe --notify done --tool codex --preguica"
        );
        assert_eq!(any_shell(r"C:\Program Files\x.exe"), r#""C:/Program Files/x.exe""#);
    }

    #[cfg(windows)]
    #[test]
    fn nome_curto_nao_tem_espacos() {
        let dir = std::env::temp_dir().join(format!("lenko com espaço {}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("lenko.exe");
        fs::write(&file, b"").unwrap();
        let cmd = command_for(&GEMINI, &file, "done");
        fs::remove_dir_all(&dir).unwrap();
        // Volumes sem nomes curtos caem nas aspas; os dois casos são aceitáveis
        assert!(!cmd.split(" --notify").next().unwrap().contains(' ') || cmd.starts_with('"'), "{cmd}");
    }

    #[test]
    fn arquivo_inexistente_cria_todos_os_hooks() {
        for tool in TOOLS {
            let out = v(&merge(tool, None, Some(exe())).unwrap());
            for (event, kind) in tool.events {
                let entry = &out["hooks"][event][0];
                let cmd = match tool.layout {
                    Layout::Nested => &entry["hooks"][0]["command"],
                    Layout::Flat => &entry["command"],
                };
                assert_eq!(cmd, &Value::String(command_for(tool, exe(), kind)), "{}", tool.id);
            }
            assert_eq!(status(tool, Some(&out.to_string()), exe()), Ok(Status::Connected), "{}", tool.id);
        }
    }

    #[test]
    fn cursor_usa_formato_proprio_com_versao() {
        let out = v(&merge(&CURSOR, None, Some(exe())).unwrap());
        assert_eq!(out["version"], 1);
        assert!(out["hooks"]["stop"][0].get("hooks").is_none());

        let mine = r#"{"version":1,"hooks":{"stop":[{"command":"./hooks/meu.sh"}]}}"#;
        let connected = merge(&CURSOR, Some(mine), Some(exe())).unwrap();
        assert_eq!(v(&connected)["hooks"]["stop"].as_array().unwrap().len(), 2);
        assert_eq!(v(&merge(&CURSOR, Some(&connected), None).unwrap()), v(mine));
        // Desconectar remove também o "version" que criamos
        let alone = merge(&CURSOR, None, Some(exe())).unwrap();
        assert_eq!(v(&merge(&CURSOR, Some(&alone), None).unwrap()), json!({}));
    }

    #[test]
    fn preserva_hooks_e_configuracoes_de_terceiros() {
        let out = v(&merge(&CLAUDE, Some(THIRD_PARTY), Some(exe())).unwrap());
        assert_eq!(out["theme"], "dark");
        assert_eq!(out["hooks"]["PreToolUse"], v(THIRD_PARTY)["hooks"]["PreToolUse"]);
        let stop = out["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "outro-app --avisa");
    }

    #[test]
    fn reinstalar_nao_duplica_e_corrige_o_caminho() {
        let old_exe = Path::new(r"C:\versao-antiga\lenko.exe");
        let first = merge(&CLAUDE, Some(THIRD_PARTY), Some(old_exe)).unwrap();
        assert_eq!(status(&CLAUDE, Some(&first), exe()), Ok(Status::Outdated));

        let second = merge(&CLAUDE, Some(&first), Some(exe())).unwrap();
        let third = merge(&CLAUDE, Some(&second), Some(exe())).unwrap();
        assert_eq!(second, third);
        assert_eq!(v(&third)["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(!third.contains("versao-antiga"));
        assert_eq!(status(&CLAUDE, Some(&third), exe()), Ok(Status::Connected));
    }

    #[test]
    fn reconhece_instalacao_feita_pelo_script_node() {
        let node = r#"{ "hooks": { "Stop": [{ "hooks": [{ "type": "command",
            "command": "node \"C:/dev/notify.js\" done claude-code --preguica" }] }] } }"#;
        assert_eq!(status(&CLAUDE, Some(node), exe()), Ok(Status::Outdated));
        let out = merge(&CLAUDE, Some(node), Some(exe())).unwrap();
        assert!(!out.contains("notify.js"));
    }

    #[test]
    fn desconectar_remove_so_o_que_e_nosso() {
        let connected = merge(&CLAUDE, Some(THIRD_PARTY), Some(exe())).unwrap();
        let out = v(&merge(&CLAUDE, Some(&connected), None).unwrap());
        assert_eq!(out, v(THIRD_PARTY));
        assert_eq!(status(&CLAUDE, Some(&out.to_string()), exe()), Ok(Status::NotConnected));
    }

    #[test]
    fn desconectar_sem_terceiros_remove_a_chave_hooks() {
        let connected = merge(&GEMINI, Some(r#"{"theme":"dark"}"#), Some(exe())).unwrap();
        assert_eq!(v(&merge(&GEMINI, Some(&connected), None).unwrap()), json!({"theme":"dark"}));
    }

    #[test]
    fn mantem_a_ordem_das_chaves_do_usuario() {
        let out = merge(&CLAUDE, Some(r#"{"zeta":1,"alfa":2}"#), Some(exe())).unwrap();
        assert!(out.find("zeta").unwrap() < out.find("alfa").unwrap());
    }

    #[test]
    fn json_invalido_nao_mexe_em_nada() {
        let bad_files = ["{ isso não é json", "[1, 2]", r#"{"hooks": 3}"#, r#"{"hooks": {"Stop": {}}}"#,
            "{ // comentário\n \"theme\": 1 }"];
        for bad in bad_files {
            assert_eq!(merge(&CLAUDE, Some(bad), Some(exe())), Err(Error::InvalidJson("settings.json")), "{bad}");
            assert_eq!(merge(&CLAUDE, Some(bad), None), Err(Error::InvalidJson("settings.json")), "{bad}");
        }
    }

    #[test]
    fn arquivo_real_backup_e_json_invalido() {
        let dir = std::env::temp_dir().join(format!("lenko-teste-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let settings = settings_path(&CLAUDE, &dir);

        // Desconectar sem arquivo não cria nada
        disconnect(&CLAUDE, &settings).unwrap();
        assert!(!settings.exists());

        // Inexistente: cria a pasta e o arquivo, sem backup
        connect(&CLAUDE, &settings, exe()).unwrap();
        assert_eq!(read_status(&CLAUDE, &settings, exe()), Ok(Status::Connected));
        let backup = dir.join(".claude").join("settings.json.bak-preguica");
        assert!(!backup.exists());

        // Desconectar faz backup do conteúdo anterior
        let before = fs::read_to_string(&settings).unwrap();
        disconnect(&CLAUDE, &settings).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), before);
        assert_eq!(read_status(&CLAUDE, &settings, exe()), Ok(Status::NotConnected));

        // JSON inválido: erro, arquivo e backup intactos
        fs::write(&settings, "{ quebrado").unwrap();
        let backup_before = fs::read_to_string(&backup).unwrap();
        assert_eq!(connect(&CLAUDE, &settings, exe()), Err(Error::InvalidJson("settings.json")));
        assert_eq!(fs::read_to_string(&settings).unwrap(), "{ quebrado");
        assert_eq!(fs::read_to_string(&backup).unwrap(), backup_before);

        fs::remove_dir_all(&dir).unwrap();
    }
}
