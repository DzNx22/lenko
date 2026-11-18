//! Ícone na bandeja do sistema: mostrar/ocultar, silenciar, mascote, ferramentas de IA, sair.
//!
//! Como a janela não aparece na barra de tarefas, a bandeja é o único jeito de
//! trazer a preguiça de volta depois de ocultá-la.

use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::{config, hooks, skin, update};

/// Itens que mudam de texto ou de marcação depois de criados.
struct Items {
    toggle: MenuItem<Wry>,
    mute: CheckMenuItem<Wry>,
    tools: Vec<(&'static hooks::Tool, CheckMenuItem<Wry>)>,
    sizes: Vec<(u8, CheckMenuItem<Wry>)>,
    skins: Submenu<Wry>,
    autostart: CheckMenuItem<Wry>,
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
    let skins = Submenu::new(app, "Mascote", true)?;
    fill_skins(app, &skins)?;
    let tools = hooks::TOOLS
        .into_iter()
        .map(|tool| {
            let (enabled, checked, label) = tool_state(app, tool);
            CheckMenuItem::with_id(app, format!("tool-{}", tool.id), label, enabled, checked, None::<&str>)
                .map(|item| (tool, item))
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let tool_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        tools.iter().map(|(_, item)| item as &dyn tauri::menu::IsMenuItem<Wry>).collect();
    let tools_menu = Submenu::with_items(app, "Conectar à IA", true, &tool_refs)?;
    let items = Items {
        toggle: MenuItem::with_id(app, "toggle", "Ocultar", true, None::<&str>)?,
        mute: CheckMenuItem::with_id(app, "mute", "Silenciar sons", true, muted, None::<&str>)?,
        tools,
        sizes,
        skins,
        autostart: CheckMenuItem::with_id(
            app,
            "autostart",
            "Iniciar com o sistema",
            true,
            app.autolaunch().is_enabled().unwrap_or(false),
            None::<&str>,
        )?,
    };
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &items.toggle,
            &items.mute,
            &size_menu,
            &items.skins,
            &tools_menu,
            &items.autostart,
            &MenuItem::with_id(app, "update", "Procurar atualizações", true, None::<&str>)?,
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
        "autostart" => toggle_autostart(app),
        "update" => check_update(app),
        "quit" => app.exit(0),
        "skin-remove" => {
            if let Err(text) = crate::remove_skin(app) {
                let _ = app.emit("notice", Notice { ok: false, text });
            }
        }
        id if id.starts_with("tool-") => {
            if let Some(tool) = hooks::find(&id["tool-".len()..]) {
                toggle_tool(app, tool);
            }
        }
        id if id.starts_with("skin-") => {
            if let Err(text) = crate::select_skin(app, &id["skin-".len()..]) {
                let _ = app.emit("notice", Notice { ok: false, text });
            }
        }
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

fn toggle_tool(app: &AppHandle, tool: &'static hooks::Tool) {
    let result = if crate::tool_connected(app, tool) {
        crate::tool_disconnect(app, tool).map(|_| format!("Desconectei do {}.", tool.name))
    } else {
        crate::tool_connect(app, tool).map(|_| tool.connected_text.to_string())
    };
    let notice = match result {
        Ok(text) => Notice { ok: true, text },
        Err(text) => Notice { ok: false, text },
    };
    let _ = app.emit("notice", notice);
}

/// Procura na hora. Se houver versão nova, o balão de atualização aparece pelo
/// evento `update-available`; senão, a preguiça diz que está tudo em dia.
fn check_update(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let notice = match update::check(&app).await {
            Ok(Some(_)) => return,
            Ok(None) => Notice { ok: true, text: "Já estou na versão mais nova.".into() },
            Err(e) => Notice { ok: false, text: format!("Não consegui procurar atualizações: {e}") },
        };
        let _ = app.emit("notice", notice);
    });
}

fn toggle_autostart(app: &AppHandle) {
    let item = &app.state::<Items>().autostart;
    // O item já trocou a própria marcação ao ser clicado
    let enable = item.is_checked().unwrap_or(false);
    let launcher = app.autolaunch();
    let result = if enable { launcher.enable() } else { launcher.disable() };
    if let Err(e) = result {
        let _ = item.set_checked(!enable);
        let text = format!("Não consegui mudar o início com o sistema: {e}");
        let _ = app.emit("notice", Notice { ok: false, text });
    }
}

/// (habilitado, marcado, texto) do item de uma ferramenta. Ferramenta não encontrada
/// e não conectada fica desabilitada, para não criar pastas de programas que não existem.
fn tool_state(app: &AppHandle, tool: &hooks::Tool) -> (bool, bool, String) {
    let connected = crate::tool_connected(app, tool);
    let found = crate::home_dir(app).is_ok_and(|home| hooks::detected(tool, &home));
    let label = if connected || found { tool.name.to_string() } else { format!("{} (não encontrado)", tool.name) };
    (connected || found, connected, label)
}

/// Atualiza as marcações depois de conectar ou desconectar por qualquer caminho.
pub fn refresh_tools(app: &AppHandle) {
    if let Some(items) = app.try_state::<Items>() {
        for (tool, item) in &items.tools {
            let (enabled, checked, label) = tool_state(app, tool);
            let _ = item.set_enabled(enabled);
            let _ = item.set_checked(checked);
            let _ = item.set_text(label);
        }
    }
}

/// Mantém a marcação em dia quando o som é trocado pelo botão ♪ da preguiça.
pub fn set_muted(app: &AppHandle, muted: bool) {
    if let Some(items) = app.try_state::<Items>() {
        let _ = items.mute.set_checked(muted);
    }
}

/// Lista os mascotes disponíveis, marca o atual e oferece remover os instalados.
fn fill_skins(app: &AppHandle, menu: &Submenu<Wry>) -> tauri::Result<()> {
    for item in menu.items()? {
        menu.remove(&item)?;
    }
    let active = crate::config_dir(app)
        .ok()
        .and_then(|dir| config::load(&dir).skin)
        .unwrap_or_else(|| skin::DEFAULT_ID.into());
    let installed = crate::data_dir(app).map(|dir| skin::list(&dir)).unwrap_or_default();
    let mut found = false;
    for m in &installed {
        let checked = m.id == active;
        found |= checked;
        menu.append(&CheckMenuItem::with_id(app, format!("skin-{}", m.id), &m.name, true, checked, None::<&str>)?)?;
    }
    if installed.is_empty() {
        // Sem pasta de dados: ainda assim mostra a preguiça
        let name = skin::default_manifest().name;
        menu.append(&CheckMenuItem::with_id(app, "skin-preguica", name, true, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let removable = found && active != skin::DEFAULT_ID;
    menu.append(&MenuItem::with_id(app, "skin-remove", "Remover este mascote", removable, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "skin-hint", "Para instalar, arraste um .mascote até mim", false, None::<&str>)?)?;
    Ok(())
}

/// Atualiza a lista depois de instalar, trocar ou remover um mascote.
pub fn refresh_skins(app: &AppHandle) {
    if let Some(items) = app.try_state::<Items>() {
        let _ = fill_skins(app, &items.skins);
    }
}
