//! Percebe quando o dev volta para o terminal ou editor e avisa a preguiça (`dev-focus`),
//! para ela parar de insistir no "Terminei!" sem precisar de clique.
//!
//! Nenhum hook do Claude Code avisa que o dev voltou, então olhamos a janela em primeiro
//! plano: quando ela passa de outro programa para um terminal/editor, emitimos o evento.
//! Só existe no Windows por enquanto; nos outros sistemas não faz nada.

use tauri::AppHandle;

/// Executáveis de terminais e editores onde uma IA de código costuma rodar.
const DEV_APPS: &[&str] = &[
    // terminais
    "windowsterminal.exe", "openconsole.exe", "conhost.exe", "cmd.exe", "powershell.exe",
    "pwsh.exe", "wezterm-gui.exe", "alacritty.exe", "mintty.exe", "hyper.exe", "tabby.exe",
    "warp.exe", "conemu64.exe", "conemu.exe", "kitty.exe",
    // editores e IDEs
    "code.exe", "code - insiders.exe", "cursor.exe", "windsurf.exe", "zed.exe", "devenv.exe",
    "idea64.exe", "pycharm64.exe", "webstorm64.exe", "rider64.exe", "goland64.exe",
    "clion64.exe", "phpstorm64.exe", "rustrover64.exe", "datagrip64.exe", "studio64.exe",
    "sublime_text.exe",
    // apps do Claude
    "claude.exe",
];

/// Diz se o caminho do executável é de um terminal ou editor.
pub fn is_dev_app(exe_path: &str) -> bool {
    let name = exe_path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
    DEV_APPS.contains(&name.as_str())
}

pub fn start(app: &AppHandle) {
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::spawn(move || windows::watch(app));
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
mod windows {
    use super::is_dev_app;
    use std::time::Duration;
    use tauri::{AppHandle, Emitter};
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    const POLL: Duration = Duration::from_millis(700);

    pub fn watch(app: AppHandle) {
        let mut last_hwnd = 0usize;
        let mut was_dev = true; // não dispara ao abrir o app com o terminal em foco
        loop {
            std::thread::sleep(POLL);
            let hwnd = unsafe { GetForegroundWindow() };
            if hwnd as usize == last_hwnd {
                continue; // mesma janela: nada mudou
            }
            last_hwnd = hwnd as usize;
            let is_dev = foreground_exe(hwnd).is_some_and(|exe| is_dev_app(&exe));
            if is_dev && !was_dev {
                let _ = app.emit("dev-focus", ());
            }
            was_dev = is_dev;
        }
    }

    fn foreground_exe(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<String> {
        if hwnd.is_null() {
            return None;
        }
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconhece_terminais_e_editores() {
        assert!(is_dev_app(r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"));
        assert!(is_dev_app(r"C:\Users\x\AppData\Local\Programs\Microsoft VS Code\Code.exe"));
        assert!(is_dev_app("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"));
        assert!(!is_dev_app(r"C:\Program Files\Google\Chrome\Application\chrome.exe"));
        assert!(!is_dev_app(r"C:\Program Files\LENKO\lenko.exe"));
        assert!(!is_dev_app(""));
    }
}
