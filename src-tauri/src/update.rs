//! Atualização automática pelas Releases do GitHub.
//!
//! Procura ao abrir e a cada poucas horas. Nunca instala sozinho: avisa a preguiça,
//! que pergunta no balão, e só baixa quando o usuário clica em "Atualizar".
//! O pacote é verificado com a chave pública do `tauri.conf.json` antes de instalar.

use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Atualização encontrada e ainda não instalada.
#[derive(Default)]
struct Pending(Mutex<Option<Update>>);

/// Começa as verificações periódicas em segundo plano.
pub fn start(app: &AppHandle) {
    app.manage(Pending::default());
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(FIRST_CHECK); // deixa o app abrir sem disputar rede
        loop {
            if let Err(e) = tauri::async_runtime::block_on(check(&app)) {
                eprintln!("Não consegui procurar atualizações: {e}");
            }
            thread::sleep(EVERY);
        }
    });
}

/// Procura uma versão nova. Se achar, guarda e avisa a preguiça (`update-available`).
pub async fn check(app: &AppHandle) -> Result<Option<String>, String> {
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let Some(update) = found else { return Ok(None) };
    let version = update.version.clone();
    *app.state::<Pending>().0.lock().unwrap() = Some(update);
    let _ = app.emit("update-available", &version);
    Ok(Some(version))
}

/// Chamado pelo botão "Atualizar" do balão: baixa, instala e reinicia.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = app
        .state::<Pending>()
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("Nenhuma atualização pendente.")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Não consegui atualizar: {e}"))?;
    // No Windows o instalador já fecha o app; nos outros sistemas reinicia aqui
    app.restart();
}
