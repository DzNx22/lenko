// Esconde o console no Windows em builds de release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod focus;
mod hooks;
mod notify;
mod skin;
mod tray;
mod update;

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
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
        // Desligado por padrão: só registra quando o usuário marca na bandeja
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            claude_should_ask,
            claude_connect,
            claude_decline,
            claude_disconnect,
            muted_get,
            muted_set,
            scale_get,
            skin_get,
            skin_install,
            update::update_install
        ])
        .setup(|app| {
            let cfg = config_dir(app.handle()).map(|dir| config::load(&dir)).unwrap_or_default();
            let content = data_dir(app.handle())
                .map(|dir| skin::load(&dir, cfg.skin.as_deref()).content())
                .unwrap_or(DEFAULT_CONTENT);
            app.manage(Content(Mutex::new(content)));
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_size(window_size(cfg.scale(), content));
                position_bottom_right(&win);
            }
            repair_claude_hooks(app.handle());
            tray::build(app.handle())?;
            update::start(app.handle());
            focus::start(app.handle());
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

// ---------------------------------------------------------------------------
// Tamanho
// ---------------------------------------------------------------------------

/// Tamanho do mascote atual (prop + quadro), em pixels do sprite.
struct Content(Mutex<skin::Size>);

/// Tamanho da preguiça: 32 de largura, 4 do galho + 23 do corpo.
const DEFAULT_CONTENT: skin::Size = skin::Size { width: 32, height: 27 };

/// Janela proporcional ao mascote e à escala, com folga para o balão.
/// Com a preguiça dá 240×300 no Grande.
fn window_size(scale: u8, content: skin::Size) -> tauri::LogicalSize<f64> {
    let s = f64::from(scale);
    let (w, h) = (f64::from(content.width), f64::from(content.height));
    tauri::LogicalSize::new(120.0 + 0.75 * w * s, 150.0 + h * s * 10.0 / 9.0)
}

/// Redimensiona mantendo o canto inferior direito no lugar.
fn fit_window(app: &tauri::AppHandle, scale: u8, content: skin::Size) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("janela não encontrada")?;
    let (pos, old) = (win.outer_position().map_err(|e| e.to_string())?, win.outer_size().map_err(|e| e.to_string())?);
    let size = window_size(scale, content);
    let new = size.to_physical::<i32>(win.scale_factor().map_err(|e| e.to_string())?);
    win.set_size(size).map_err(|e| e.to_string())?;
    let _ = win.set_position(PhysicalPosition::new(
        pos.x + old.width as i32 - new.width,
        pos.y + old.height as i32 - new.height,
    ));
    Ok(())
}

fn content(app: &tauri::AppHandle) -> skin::Size {
    *app.state::<Content>().0.lock().unwrap()
}

#[tauri::command]
fn scale_get(app: tauri::AppHandle) -> u8 {
    config_dir(&app).map_or(5, |dir| config::load(&dir).scale())
}

/// Troca o tamanho, salva e avisa a preguiça.
fn set_scale(app: &tauri::AppHandle, scale: u8) -> Result<(), String> {
    fit_window(app, scale, content(app))?;
    let _ = app.emit("scale", scale);

    let dir = config_dir(app)?;
    let mut cfg = config::load(&dir);
    cfg.scale = scale;
    config::save(&dir, &cfg).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Mascotes
// ---------------------------------------------------------------------------

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Mascote escolhido, pronto para desenhar (a preguiça, se o escolhido sumiu).
#[tauri::command]
fn skin_get(app: tauri::AppHandle) -> Result<skin::Skin, String> {
    let id = config_dir(&app).ok().and_then(|dir| config::load(&dir).skin);
    Ok(skin::load(&data_dir(&app)?, id.as_deref()))
}

/// Instala um `.mascote` arrastado para a janela e já troca para ele.
#[tauri::command]
fn skin_install(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let manifest = skin::install(&data_dir(&app)?, std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    select_skin(&app, &manifest.id)?;
    Ok(manifest.name)
}

/// Troca o mascote: salva a escolha, ajusta a janela e avisa o webview e a bandeja.
fn select_skin(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let dir = config_dir(app)?;
    let mut cfg = config::load(&dir);
    cfg.skin = (id != skin::DEFAULT_ID).then(|| id.to_string());
    config::save(&dir, &cfg).map_err(|e| e.to_string())?;

    let size = skin::load(&data_dir(app)?, cfg.skin.as_deref()).content();
    *app.state::<Content>().0.lock().unwrap() = size;
    fit_window(app, cfg.scale(), size)?;
    let _ = app.emit("skin", ());
    tray::refresh_skins(app);
    Ok(())
}

/// Remove o mascote atual e volta para a preguiça.
fn remove_skin(app: &tauri::AppHandle) -> Result<(), String> {
    let id = config::load(&config_dir(app)?).skin.ok_or("A preguiça não pode ser removida.")?;
    skin::remove(&data_dir(app)?, &id).map_err(|e| e.to_string())?;
    select_skin(app, skin::DEFAULT_ID)
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
