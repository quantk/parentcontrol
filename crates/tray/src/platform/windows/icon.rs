//! The notification-area icon, run on a thread with a Win32 message loop.
//!
//! The async side queues commands and wakes the thread with a window
//! message; the window procedure applies them. Explorer restarts are
//! handled by re-adding the icon on `TaskbarCreated`.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError, mpsc};
use std::{io, ptr, thread};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, IDI_INFORMATION,
    LoadIconW, MSG, PostMessageW, RegisterClassW, RegisterWindowMessageW, TranslateMessage, WM_APP,
    WNDCLASSW,
};

/// Wakes the UI thread to apply queued commands.
const WM_COMMANDS: u32 = WM_APP + 1;

/// Commands for the UI thread and the tooltip it shows.
static STATE: Mutex<State> = Mutex::new(State {
    queue: VecDeque::new(),
    tooltip: Vec::new(),
});

struct State {
    queue: VecDeque<Command>,
    /// The current tooltip, to restore after Explorer restarts.
    tooltip: Vec<u16>,
}

enum Command {
    Tooltip(Vec<u16>),
    Notify { title: Vec<u16>, body: Vec<u16> },
}

/// A handle to the icon's window.
#[derive(Debug)]
pub(super) struct Icon {
    window: Window,
}

/// A window handle, which is a plain value valid from any thread.
#[derive(Debug, Clone, Copy)]
struct Window(HWND);

// SAFETY: window handles are process-wide identifiers; posting messages to
// them is explicitly allowed from any thread.
unsafe impl Send for Window {}

impl Icon {
    /// Starts the UI thread and waits until the icon exists.
    pub(super) fn start() -> io::Result<Self> {
        let (created, window) = mpsc::channel();
        thread::Builder::new()
            .name("tray-icon".to_owned())
            .spawn(move || {
                match create_window() {
                    Ok(window) => {
                        // The receiver waits for this; it cannot be gone.
                        let _ = created.send(Ok(Window(window)));
                        add_icon(window);
                        message_loop();
                    }
                    Err(error) => drop(created.send(Err(error))),
                }
            })?;
        let window = window.recv().map_err(io::Error::other)??;
        Ok(Self { window })
    }

    /// Replaces the tooltip text.
    pub(super) fn set_tooltip(&self, text: &str) {
        self.send(Command::Tooltip(wide(text)));
    }

    /// Shows a notification.
    pub(super) fn notify(&self, title: &str, body: &str) {
        self.send(Command::Notify {
            title: wide(title),
            body: wide(body),
        });
    }

    fn send(&self, command: Command) {
        STATE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .queue
            .push_back(command);
        // SAFETY: posting to a window of this process; a failure only
        // means the UI thread is gone, and then nothing can be shown.
        unsafe { PostMessageW(self.window.0, WM_COMMANDS, 0, 0) };
    }
}

fn create_window() -> io::Result<HWND> {
    let class = wide("ParentControlTray");
    // SAFETY: null asks for the module of this executable.
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let class_info = WNDCLASSW {
        lpfnWndProc: Some(window_procedure),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..WNDCLASSW::default()
    };
    // SAFETY: `class_info` and the class name outlive the call.
    if unsafe { RegisterClassW(&raw const class_info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a message-only window of the class just registered.
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            ptr::null_mut(),
            instance,
            ptr::null(),
        )
    };
    if window.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(window)
}

fn message_loop() {
    let mut message = MSG::default();
    // SAFETY: `message` is a valid out-pointer; the loop ends on WM_QUIT
    // (0) or an error (-1).
    while unsafe { GetMessageW(&raw mut message, ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: `message` was filled by `GetMessageW`.
        unsafe {
            TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}

unsafe extern "system" fn window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_COMMANDS {
        apply_commands(window);
        return 0;
    }
    if message == taskbar_created() {
        add_icon(window);
        return 0;
    }
    // SAFETY: default handling with the arguments we received.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn taskbar_created() -> u32 {
    let name = wide("TaskbarCreated");
    // SAFETY: `name` is NUL-terminated. Registering is idempotent.
    unsafe { RegisterWindowMessageW(name.as_ptr()) }
}

fn add_icon(window: HWND) {
    let tooltip = STATE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .tooltip
        .clone();
    let mut data = icon_data(window);
    data.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    // SAFETY: plain call for a stock icon.
    data.hIcon = unsafe { LoadIconW(ptr::null_mut(), IDI_INFORMATION) };
    copy_into(&mut data.szTip, &tooltip);
    // SAFETY: `data` is fully initialized for this icon.
    unsafe { Shell_NotifyIconW(NIM_ADD, &raw const data) };
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    // SAFETY: as above.
    unsafe { Shell_NotifyIconW(NIM_SETVERSION, &raw const data) };
}

fn apply_commands(window: HWND) {
    let commands: Vec<Command> = {
        let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
        state.queue.drain(..).collect()
    };
    for command in commands {
        let mut data = icon_data(window);
        match command {
            Command::Tooltip(text) => {
                data.uFlags = NIF_TIP | NIF_SHOWTIP;
                copy_into(&mut data.szTip, &text);
                STATE.lock().unwrap_or_else(PoisonError::into_inner).tooltip = text;
            }
            Command::Notify { title, body } => {
                data.uFlags = NIF_INFO;
                data.dwInfoFlags = NIIF_INFO;
                copy_into(&mut data.szInfoTitle, &title);
                copy_into(&mut data.szInfo, &body);
            }
        }
        // SAFETY: `data` is fully initialized for this icon.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &raw const data) };
    }
}

fn icon_data(window: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(u32::MAX),
        hWnd: window,
        uID: 1,
        ..NOTIFYICONDATAW::default()
    }
}

/// Copies `text` into a fixed buffer, truncated and NUL-terminated.
fn copy_into(buffer: &mut [u16], text: &[u16]) {
    let length = text
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(text.len())
        .min(buffer.len() - 1);
    buffer[..length].copy_from_slice(&text[..length]);
    buffer[length] = 0;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
