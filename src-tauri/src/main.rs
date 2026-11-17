// Esconde o console no Windows em builds de release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod hooks;
mod notify;
mod tray;

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::thread;
use tauri::{Emitter, Manager, PhysicalPosition};

/// Porta local onde os adaptadores (hooks) mandam eventos.
const ADDR: &str = "127.0.0.1:7777";

/// Contrato de evento. Qualquer ferramenta de IA que mande este JSON
/// via POST /event faz a preguiça reagir.
#[derive(Clone, Serialize, Deserialize, Debug)]
struct AiEvent {
    /// "claude-code", "codex", etc.
    tool: Option<String>,
    /// "working" | "done" | "waiting"
    #[serde(rename = "type")]
    kind: String,
    /// Nome do projeto/pasta
    project: Option<String>,
    /// Texto livre opcional (ex.: mensagem de permissão)
    message: Option<String>,
}

fn main() {
    // Chamado por um hook: manda o evento e sai, sem abrir janela nem iniciar o Tauri
    let args: Vec<String> = std::env::args().collect();
    if let Some(notify_args) = notify::parse_args(&args) {
        notify::run(notify_args);
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            claude_should_ask,
            claude_connect,
            claude_decline,
            claude_disconnect,
            muted_get,
            muted_set
        ])
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                position_bottom_right(&win);
            }
            repair_claude_hooks(app.handle());
            tray::build(app.handle())?;
            let handle = app.handle().clone();
            thread::spawn(move || run_server(handle));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o LENKO");
}

// ---------------------------------------------------------------------------
// Conexão com o Claude Code
// ---------------------------------------------------------------------------

fn config_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn claude_settings(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    Ok(hooks::settings_path(&home))
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

fn remember(app: &tauri::AppHandle, answer: config::ClaudeCode) -> Result<(), String> {
    let dir = config_dir(app)?;
    let mut cfg = config::load(&dir);
    cfg.claude_code = Some(answer);
    config::save(&dir, &cfg).map_err(|e| e.to_string())
}

/// Se o usuário já conectou e o executável mudou de lugar (reinstalação ou
/// atualização), aponta os hooks para o caminho novo sem perguntar de novo.
/// Se as entradas foram removidas à mão, respeita e não recria.
fn repair_claude_hooks(app: &tauri::AppHandle) {
    let Ok(dir) = config_dir(app) else { return };
    if config::load(&dir).claude_code != Some(config::ClaudeCode::Connected) {
        return;
    }
    let (Ok(settings), Ok(exe)) = (claude_settings(app), current_exe()) else { return };
    if hooks::read_status(&settings, &exe) == Ok(hooks::Status::Outdated) {
        if let Err(e) = hooks::connect(&settings, &exe) {
            eprintln!("Não consegui corrigir os hooks do Claude Code: {e}");
        }
    }
}

/// O balão "Quer que eu me conecte ao Claude Code?" só aparece se ainda não perguntou.
#[tauri::command]
fn claude_should_ask(app: tauri::AppHandle) -> bool {
    config_dir(&app).is_ok_and(|dir| config::load(&dir).claude_code.is_none())
}

#[tauri::command]
fn claude_connect(app: tauri::AppHandle) -> Result<(), String> {
    let result = hooks::connect(&claude_settings(&app)?, &current_exe()?).map_err(|e| e.to_string());
    tray::refresh_claude(&app);
    result?;
    remember(&app, config::ClaudeCode::Connected)
}

#[tauri::command]
fn claude_decline(app: tauri::AppHandle) -> Result<(), String> {
    remember(&app, config::ClaudeCode::Declined)
}

/// Remove só as entradas do LENKO (item da bandeja).
#[tauri::command]
fn claude_disconnect(app: tauri::AppHandle) -> Result<(), String> {
    let result = hooks::disconnect(&claude_settings(&app)?).map_err(|e| e.to_string());
    tray::refresh_claude(&app);
    result?;
    remember(&app, config::ClaudeCode::Declined)
}

// ---------------------------------------------------------------------------
// Som (lembrado entre aberturas)
// ---------------------------------------------------------------------------

fn save_muted(app: &tauri::AppHandle, muted: bool) -> Result<(), String> {
    let dir = config_dir(app)?;
    let mut cfg = config::load(&dir);
    cfg.muted = muted;
    config::save(&dir, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
fn muted_get(app: tauri::AppHandle) -> bool {
    config_dir(&app).is_ok_and(|dir| config::load(&dir).muted)
}

/// Chamado pelo botão ♪ da preguiça.
#[tauri::command]
fn muted_set(app: tauri::AppHandle, muted: bool) -> Result<(), String> {
    tray::set_muted(&app, muted);
    save_muted(&app, muted)
}

/// Coloca a janela no canto inferior direito do monitor atual.
fn position_bottom_right(win: &tauri::WebviewWindow) {
    let (Ok(Some(monitor)), Ok(size)) = (win.current_monitor(), win.outer_size()) else {
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.size();
    let origin = monitor.position();
    let margin_x = (16.0 * scale) as i32;
    let margin_y = (56.0 * scale) as i32; // folga para a barra de tarefas
    let x = origin.x + area.width as i32 - size.width as i32 - margin_x;
    let y = origin.y + area.height as i32 - size.height as i32 - margin_y;
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

/// Servidor HTTP mínimo: só escuta em localhost.
fn run_server(handle: tauri::AppHandle) {
    let server = match tiny_http::Server::http(ADDR) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Não consegui abrir {ADDR}: {e}. Já tem outro LENKO aberto?");
            return;
        }
    };

    for mut req in server.incoming_requests() {
        let mut body = String::new();
        let _ = req.as_reader().take(64 * 1024).read_to_string(&mut body);

        let (code, text) = match (req.method(), req.url()) {
            (tiny_http::Method::Post, "/event") => match serde_json::from_str::<AiEvent>(&body) {
                Ok(ev) => {
                    let _ = handle.emit("ai-event", ev);
                    (200, "ok")
                }
                Err(_) => (400, "JSON inválido: envie {\"type\": \"done\"}"),
            },
            (tiny_http::Method::Get, "/health") => (200, "preguiça acordada"),
            _ => (404, "rota não encontrada"),
        };

        let _ = req.respond(tiny_http::Response::from_string(text).with_status_code(code));
    }
}
