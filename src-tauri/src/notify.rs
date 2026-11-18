//! Modo `--notify`: o próprio executável funciona como adaptador de hook.
//!
//! `lenko.exe --notify done --tool claude-code` lê o JSON do hook pelo stdin,
//! manda o evento para a instância aberta e sai, sem abrir janela nem iniciar o Tauri.
//! Mesmo comportamento de `scripts/notify.js`: sempre sai com código 0, em menos de 1 s.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Espera máxima pelo stdin do hook.
const STDIN_WAIT: Duration = Duration::from_millis(500);
/// Orçamento total do processo, contando o stdin.
const TOTAL_BUDGET: Duration = Duration::from_millis(900);
/// Em localhost o app aceita a conexão em menos de 1 ms. Com o app fechado, o Windows
/// leva ~2 s para recusar (tenta de novo várias vezes), então não vale esperar mais que isso.
const CONNECT_WAIT: Duration = Duration::from_millis(150);

#[derive(Debug, PartialEq)]
pub struct NotifyArgs {
    pub kind: String,
    pub tool: String,
}

/// Retorna `Some` quando o executável foi chamado em modo `--notify`.
/// Argumentos desconhecidos (como a marca `--preguica`) são ignorados.
pub fn parse_args(args: &[String]) -> Option<NotifyArgs> {
    let pos = args.iter().position(|a| a == "--notify")?;
    let kind = match args.get(pos + 1) {
        Some(k) if !k.starts_with("--") => k.clone(),
        _ => "done".into(),
    };
    let tool = args
        .iter()
        .position(|a| a == "--tool")
        .and_then(|i| args.get(i + 1))
        .filter(|t| !t.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "claude-code".into());
    Some(NotifyArgs { kind, tool })
}

/// Executa o modo `--notify` e encerra o processo com código 0, aconteça o que acontecer.
pub fn run(args: NotifyArgs) -> ! {
    let start = Instant::now();
    let _ = std::panic::catch_unwind(|| {
        let raw = read_stdin(STDIN_WAIT);
        let body = build_body(&args, &raw);
        let remaining = TOTAL_BUDGET.saturating_sub(start.elapsed());
        let _ = post(&body, remaining);
        reply(&args.tool);
    });
    std::process::exit(0);
}

/// Resposta no stdout. O Claude Code e o Codex aceitam saída vazia; o Gemini CLI exige
/// JSON, e o Cursor espera `continue` no beforeSubmitPrompt (vale para os outros eventos).
fn reply(tool: &str) {
    let out = match tool {
        "gemini-cli" => "{}",
        "cursor" => r#"{"continue":true}"#,
        _ => return,
    };
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{out}");
    let _ = stdout.flush();
}

/// Lê o stdin numa thread separada para nunca esperar mais que `wait`.
fn read_stdin(wait: Duration) -> String {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::stdin().take(1024 * 1024).read_to_string(&mut s);
        let _ = tx.send(s);
    });
    rx.recv_timeout(wait).unwrap_or_default()
}

/// Monta o JSON do contrato de evento a partir do JSON do hook (inválido vira `{}`).
pub fn build_body(args: &NotifyArgs, raw: &str) -> String {
    let hook: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    // O Cursor manda as pastas abertas em vez do cwd
    let cwd = hook
        .get("cwd")
        .or_else(|| hook.get("workspace_roots").and_then(|r| r.get(0)))
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    // Divide nos dois separadores: o hook pode mandar caminho do Windows ou Unix
    let project = cwd
        .rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or_default()
        .to_string();

    let (kind, message) = classify(&args.kind, &hook);
    let mut ev = json!({ "tool": args.tool, "type": kind, "project": project });
    if let Some(m) = message {
        ev["message"] = Value::String(m);
    }
    ev.to_string()
}

/// O hook Notification do Claude Code dispara em dois casos bem diferentes:
/// pedido de aprovação (precisa do dev) e "ocioso há 60 s" (a tarefa já terminou).
/// Também chega com texto em inglês, então traduzimos aqui. O Codex (PermissionRequest)
/// manda o nome da ferramenta em `tool_name`; o Gemini CLI, `notification_type`
/// "ToolPermission".
pub fn classify(kind: &str, hook: &Value) -> (String, Option<String>) {
    if kind != "waiting" {
        return (kind.into(), None);
    }
    let ntype = hook.get("notification_type").and_then(Value::as_str).unwrap_or("");
    let text = hook.get("message").and_then(Value::as_str).unwrap_or("");
    let lower = text.to_ascii_lowercase();

    if ntype == "idle_prompt" || lower.contains("waiting for your input") {
        return ("done".into(), None); // terminou e o dev ainda não voltou
    }
    if let Some(tool) = hook.get("tool_name").and_then(Value::as_str).filter(|t| !t.is_empty()) {
        return (kind.into(), Some(format!("Posso usar {tool}?")));
    }
    const PERM: &str = "permission to use ";
    if let Some(i) = lower.find(PERM) {
        let tool = text[i + PERM.len()..].trim();
        if !tool.is_empty() {
            return (kind.into(), Some(format!("Posso usar {tool}?")));
        }
    }
    if ntype.to_ascii_lowercase().contains("permission") || lower.contains("permission") {
        return (kind.into(), Some("Preciso da sua aprovação.".into()));
    }
    (kind.into(), Some("Preciso de você aqui.".into()))
}

/// POST mínimo em HTTP/1.1 usando só a biblioteca padrão.
fn post(body: &str, budget: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + budget;
    let left = || {
        deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1))
    };
    let addr: SocketAddr = crate::ADDR.parse().expect("endereço fixo válido");
    let mut stream = TcpStream::connect_timeout(&addr, left().min(CONNECT_WAIT))?;
    stream.set_write_timeout(Some(left()))?;
    write!(
        stream,
        "POST /event HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        crate::ADDR,
        body.len(),
        body
    )?;
    // Espera a resposta para garantir que o app processou o evento
    stream.set_read_timeout(Some(left()))?;
    let mut buf = [0u8; 64];
    let _ = stream.read(&mut buf)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn le_argumentos_do_hook() {
        let a = parse_args(&args(&["lenko.exe", "--notify", "working", "--tool", "codex", "--preguica"]));
        assert_eq!(a, Some(NotifyArgs { kind: "working".into(), tool: "codex".into() }));
        let a = parse_args(&args(&["lenko.exe", "--notify", "--preguica"])).unwrap();
        assert_eq!((a.kind.as_str(), a.tool.as_str()), ("done", "claude-code"));
        assert_eq!(parse_args(&args(&["lenko.exe"])), None);
    }

    #[test]
    fn monta_evento_com_nome_do_projeto() {
        let a = NotifyArgs { kind: "done".into(), tool: "claude-code".into() };
        let v: Value = serde_json::from_str(&build_body(&a, r#"{"cwd":"C:\\dev\\meu-app"}"#)).unwrap();
        assert_eq!(v, json!({"tool":"claude-code","type":"done","project":"meu-app"}));
    }

    #[test]
    fn cursor_usa_a_primeira_pasta_aberta() {
        let a = NotifyArgs { kind: "done".into(), tool: "cursor".into() };
        let raw = r#"{"hook_event_name":"stop","workspace_roots":["/home/eu/meu-site","/outro"]}"#;
        let v: Value = serde_json::from_str(&build_body(&a, raw)).unwrap();
        assert_eq!(v["project"], "meu-site");
        assert_eq!(v["tool"], "cursor");
    }

    #[test]
    fn json_invalido_nao_quebra() {
        let a = NotifyArgs { kind: "working".into(), tool: "claude-code".into() };
        let v: Value = serde_json::from_str(&build_body(&a, "{isso não é json")).unwrap();
        assert_eq!(v["type"], "working");
    }

    #[test]
    fn traduz_avisos_do_claude_code() {
        let c = |h: Value| classify("waiting", &h);
        assert_eq!(c(json!({"notification_type":"idle_prompt"})), ("done".into(), None));
        assert_eq!(c(json!({"message":"Claude is waiting for your input"})), ("done".into(), None));
        assert_eq!(
            c(json!({"message":"Claude needs your permission to use Bash"})),
            ("waiting".into(), Some("Posso usar Bash?".into()))
        );
        assert_eq!(
            c(json!({"notification_type":"permission_prompt"})),
            ("waiting".into(), Some("Preciso da sua aprovação.".into()))
        );
        assert_eq!(c(json!({})), ("waiting".into(), Some("Preciso de você aqui.".into())));
        // Codex: PermissionRequest
        assert_eq!(
            c(json!({"hook_event_name":"PermissionRequest","tool_name":"shell"})),
            ("waiting".into(), Some("Posso usar shell?".into()))
        );
        // Gemini CLI: Notification
        assert_eq!(
            c(json!({"notification_type":"ToolPermission","message":"Tool requires confirmation"})),
            ("waiting".into(), Some("Preciso da sua aprovação.".into()))
        );
        assert_eq!(classify("done", &json!({})), ("done".into(), None));
    }
}
