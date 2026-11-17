//! Ícone na bandeja do sistema: mostrar/ocultar, silenciar, conectar/desconectar, sair.
//!
//! Como a janela não aparece na barra de tarefas, a bandeja é o único jeito de
//! trazer a preguiça de volta depois de ocultá-la.

use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::{config, hooks};

/// Itens que mudam de texto ou de marcação depois de criados.
struct Items {
    toggle: MenuItem<Wry>,
    mute: CheckMenuItem<Wry>,
    claude: MenuItem<Wry>,
    sizes: Vec<(u8, CheckMenuItem<Wry>)>,
}

/// Aviso mostrado no balão depois de uma ação feita pela bandeja.
#[derive(Clone, Serialize)]
struct Notice {
    ok: bool,
    text: String,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let cfg = crate::config_dir(app).map(|dir| config::load(&dir)).unwrap_or_default();
    let muted = cfg.muted;
    let sizes = config::SCALES
        .iter()
        .map(|&(s, name)| {
            CheckMenuItem::with_id(app, format!("size-{s}"), name, true, s == cfg.scale(), None::<&str>)
                .map(|item| (s, item))
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let size_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        sizes.iter().map(|(_, item)| item as &dyn tauri::menu::IsMenuItem<Wry>).collect();
    let size_menu = Submenu::with_items(app, "Tamanho", true, &size_refs)?;
    let items = Items {
        toggle: MenuItem::with_id(app, "toggle", "Ocultar", true, None::<&str>)?,
        mute: CheckMenuItem::with_id(app, "mute", "Silenciar sons", true, muted, None::<&str>)?,
        claude: MenuItem::with_id(app, "claude", claude_label(app), true, None::<&str>)?,
        sizes,
    };
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &items.toggle,
            &items.mute,
            &size_menu,
            &items.claude,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("LENKO")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_click);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(items);
    Ok(())
}

/// Clique esquerdo no ícone mostra ou oculta a preguiça.
fn on_click(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        toggle_window(tray.app_handle());
    }
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "toggle" => toggle_window(app),
        "mute" => {
            // O item já trocou a própria marcação ao ser clicado
            let muted = app.state::<Items>().mute.is_checked().unwrap_or(false);
            let _ = crate::save_muted(app, muted);
            let _ = app.emit("muted", muted);
        }
        "claude" => toggle_claude(app),
        "quit" => app.exit(0),
        id => {
            let Some(scale) = id.strip_prefix("size-").and_then(|s| s.parse().ok()) else { return };
            // Funciona como opção única: só o tamanho escolhido fica marcado
            for (s, item) in &app.state::<Items>().sizes {
                let _ = item.set_checked(*s == scale);
            }
            if let Err(e) = crate::set_scale(app, scale) {
                eprintln!("Não consegui mudar o tamanho: {e}");
            }
        }
    }
}

fn toggle_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let visible = win.is_visible().unwrap_or(true);
    let _ = if visible { win.hide() } else { win.show().and_then(|_| win.set_focus()) };
    if let Some(items) = app.try_state::<Items>() {
        let _ = items.toggle.set_text(if visible { "Mostrar" } else { "Ocultar" });
    }
}

fn toggle_claude(app: &AppHandle) {
    let connected = claude_connected(app);
    let result = if connected {
        crate::claude_disconnect(app.clone()).map(|_| "Desconectei do Claude Code.")
    } else {
        crate::claude_connect(app.clone()).map(|_| "Conectado! Reinicie o Claude Code para valer.")
    };
    let notice = match result {
        Ok(text) => Notice { ok: true, text: text.into() },
        Err(text) => Notice { ok: false, text },
    };
    let _ = app.emit("notice", notice);
}

/// Conectado = há entradas do LENKO no settings.json, mesmo que com caminho antigo.
fn claude_connected(app: &AppHandle) -> bool {
    let (Ok(settings), Ok(exe)) = (crate::claude_settings(app), crate::current_exe()) else {
        return false;
    };
    matches!(
        hooks::read_status(&settings, &exe),
        Ok(hooks::Status::Connected | hooks::Status::Outdated)
    )
}

fn claude_label(app: &AppHandle) -> &'static str {
    if claude_connected(app) {
        "Desconectar do Claude Code"
    } else {
        "Conectar ao Claude Code"
    }
}

/// Atualiza o texto do item depois de conectar ou desconectar por qualquer caminho.
pub fn refresh_claude(app: &AppHandle) {
    if let Some(items) = app.try_state::<Items>() {
        let _ = items.claude.set_text(claude_label(app));
    }
}

/// Mantém a marcação em dia quando o som é trocado pelo botão ♪ da preguiça.
pub fn set_muted(app: &AppHandle, muted: bool) {
    if let Some(items) = app.try_state::<Items>() {
        let _ = items.mute.set_checked(muted);
    }
}
