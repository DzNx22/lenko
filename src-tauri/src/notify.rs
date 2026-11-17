//! Modo `--notify`: o próprio executável funciona como adaptador de hook.
//!
//! `lenko.exe --notify done --tool claude-code` lê o JSON do hook pelo stdin,
//! manda o evento para a instância aberta e sai, sem abrir janela nem iniciar o Tauri.
//! Mesmo comportamento de `scripts/notify.js`: sempre sai com código 0, em menos de 1 s.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Espera máxima pelo stdin do hook.
const STDIN_WAIT: Duration = Duration::from_millis(500);
/// Orçamento total do processo, contando o stdin.
const TOTAL_BUDGET: Duration = Duration::from_millis(900);

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
    });
    std::process::exit(0);
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
    let cwd = hook
        .get("cwd")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let project = Path::new(&cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let (kind, message) = classify(&args.kind, &hook);
    let mut ev = json!({ "tool": args.tool, "type": kind, "project": project });
    if let Some(m) = message {
        ev["message"] = Value::String(m);
    }
    ev.to_string()
}

/// O hook Notification do Claude Code dispara em dois casos bem diferentes:
/// pedido de aprovação (precisa do dev) e "ocioso há 60 s" (a tarefa já terminou).
/// Também chega com texto em inglês, então traduzimos aqui.
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
    const PERM: &str = "permission to use ";
    if let Some(i) = lower.find(PERM) {
        let tool = text[i + PERM.len()..].trim();
        if !tool.is_empty() {
            return (kind.into(), Some(format!("Posso usar {tool}?")));
        }
    }
    if ntype == "permission_prompt" || lower.contains("permission") {
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
    let mut stream = TcpStream::connect_timeout(&addr, left())?;
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
        assert_eq!(classify("done", &json!({})), ("done".into(), None));
    }
}
