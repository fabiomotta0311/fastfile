//! `hotkey`
//!
//! Gerenciamento de atalho global de teclado para trazer o FastFile para primeiro plano (Issue #17).
//! No Windows, registra atalho global no sistema operacional via Win32 API (`RegisterHotKey`).
//! Em plataformas não-Windows (ex.: Linux headless), opera como no-op seguro.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Identificador e controlador do atalho global.
#[derive(Clone)]
pub struct GlobalHotkeyListener {
    triggered: Arc<AtomicBool>,
}

impl Default for GlobalHotkeyListener {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalHotkeyListener {
    /// Inicializa e registra o atalho global do sistema (Ctrl+Alt+F / Win+Shift+F).
    #[must_use]
    pub fn new() -> Self {
        let triggered = Arc::new(AtomicBool::new(false));

        #[cfg(windows)]
        {
            let flag = Arc::clone(&triggered);
            std::thread::Builder::new()
                .name("ff-global-hotkey".into())
                .spawn(move || {
                    run_windows_hotkey_loop(flag);
                })
                .ok();
        }

        Self { triggered }
    }

    /// Verifica se o atalho global foi acionado desde a última verificação.
    pub fn check_and_reset(&self) -> bool {
        self.triggered.swap(false, Ordering::Relaxed)
    }
}

#[cfg(windows)]
fn run_windows_hotkey_loop(flag: Arc<AtomicBool>) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
    };

    const HOTKEY_CTRL_ALT_F: i32 = 1001;
    const HOTKEY_WIN_SHIFT_F: i32 = 1002;
    const VK_F: u32 = 0x46; // Tecla 'F'

    unsafe {
        // Registra Ctrl + Alt + F
        let _ = RegisterHotKey(
            std::ptr::null_mut(),
            HOTKEY_CTRL_ALT_F,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            VK_F,
        );

        // Registra Win + Shift + F
        let _ = RegisterHotKey(
            std::ptr::null_mut(),
            HOTKEY_WIN_SHIFT_F,
            MOD_WIN | MOD_SHIFT | MOD_NOREPEAT,
            VK_F,
        );

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                flag.store(true, Ordering::Relaxed);
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        UnregisterHotKey(std::ptr::null_mut(), HOTKEY_CTRL_ALT_F);
        UnregisterHotKey(std::ptr::null_mut(), HOTKEY_WIN_SHIFT_F);
    }
}
