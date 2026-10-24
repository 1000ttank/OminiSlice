use crate::window_state::{AppState, DisplayState, InteractionState, WindowId};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager, Window};
use windows::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{Dwm::*, Gdi::*},
        UI::{
            Input::KeyboardAndMouse::*,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
};

#[derive(Default)]
struct NativeState {
    guarded: u32,
    preview_blocked: bool,
    layered_before_passthrough: Option<bool>,
}
static NATIVE: OnceLock<Mutex<BTreeMap<isize, NativeState>>> = OnceLock::new();
static RECOVER: AtomicBool = AtomicBool::new(false);
static CURSOR_UNAVAILABLE: AtomicBool = AtomicBool::new(false);
static POLICY: Mutex<()> = Mutex::new(());
fn native() -> &'static Mutex<BTreeMap<isize, NativeState>> {
    NATIVE.get_or_init(Mutex::default)
}

pub fn install(window: &Window) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let raw = hwnd.0 as isize;
    let register_hotkey = window.label() == "recovery-host";
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let _ = send.send(install_on_ui(HWND(raw as _), register_hotkey));
        })
        .map_err(|e| e.to_string())?;
    receive
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
}

fn install_on_ui(hwnd: HWND, register_hotkey: bool) -> Result<(), String> {
    native()
        .lock()
        .map_err(|_| "Native state unavailable")?
        .insert(hwnd.0 as isize, NativeState::default());
    if !unsafe { SetWindowSubclass(hwnd, Some(subclass), 0x4f4d4e49, 0) }.as_bool() {
        return Err("Unable to install native window handler".into());
    }
    if register_hotkey {
        if let Err(error) = unsafe {
            RegisterHotKey(
                Some(hwnd),
                0x4f4d,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                0x4f,
            )
        } {
            eprintln!("Recovery hotkey registration failed; use tray restore: {error}");
        }
    }
    Ok(())
}

pub fn watch_children(window: &Window) -> Result<(), String> {
    let root = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    window
        .run_on_main_thread(move || unsafe {
            let _ = EnumChildWindows(Some(HWND(root as _)), Some(install_child), LPARAM(root));
        })
        .map_err(|e| e.to_string())
}

unsafe extern "system" fn install_child(hwnd: HWND, root: LPARAM) -> BOOL {
    let mut owner = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut owner));
    }
    if owner != std::process::id() {
        return BOOL(1);
    }
    if !unsafe { SetWindowSubclass(hwnd, Some(child_subclass), 0x4f4d4348, root.0 as usize) }
        .as_bool()
    {
        eprintln!("Unable to protect child HWND {hwnd:?}");
    }
    BOOL(1)
}

unsafe extern "system" fn child_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    root: usize,
) -> LRESULT {
    if let Ok(mut states) = native().lock() {
        if let Some(state) = states.get_mut(&(root as isize)) {
            match message {
                WM_IME_STARTCOMPOSITION => state.guarded |= 4,
                WM_IME_ENDCOMPOSITION | WM_KILLFOCUS => state.guarded &= !4,
                WM_ENTERMENULOOP => state.guarded |= 2,
                WM_EXITMENULOOP => state.guarded &= !2,
                WM_CANCELMODE => state.guarded = 0,
                _ => {}
            }
        }
    }
    if message == WM_NCDESTROY {
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(child_subclass), 0x4f4d4348) };
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if message == WM_HOTKEY && wparam.0 == 0x4f4d {
        RECOVER.store(true, Ordering::SeqCst);
        return LRESULT(0);
    }
    let mut bitmap = false;
    if let Ok(mut states) = native().lock() {
        if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
            match message {
                WM_ENTERSIZEMOVE => state.guarded |= 1,
                WM_EXITSIZEMOVE => state.guarded &= !1,
                WM_ENTERMENULOOP => state.guarded |= 2,
                WM_EXITMENULOOP => state.guarded &= !2,
                WM_IME_STARTCOMPOSITION => state.guarded |= 4,
                WM_IME_ENDCOMPOSITION | WM_KILLFOCUS => state.guarded &= !4,
                WM_CANCELMODE => state.guarded = 0,
                WM_DWMSENDICONICTHUMBNAIL | WM_DWMSENDICONICLIVEPREVIEWBITMAP => {
                    bitmap = state.preview_blocked
                }
                _ => {}
            }
        }
        if message == WM_NCDESTROY {
            states.remove(&(hwnd.0 as isize));
        }
    }
    if bitmap {
        let result = if message == WM_DWMSENDICONICTHUMBNAIL {
            let width = ((lparam.0 as u32 >> 16) & 0xffff).clamp(1, 320);
            let height = (lparam.0 as u32 & 0xffff).clamp(1, 200);
            placeholder(hwnd, width, height, false)
        } else {
            placeholder(hwnd, 320, 200, true)
        };
        if let Err(error) = result {
            eprintln!("Preview placeholder failed: {error}");
        }
        return LRESULT(0);
    }
    if message == WM_NCDESTROY {
        let _ = unsafe { UnregisterHotKey(Some(hwnd), 0x4f4d) };
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(subclass), 0x4f4d4e49) };
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn placeholder(hwnd: HWND, width: u32, height: u32, live: bool) -> windows::core::Result<()> {
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let center =
                    x > width / 3 && x < width * 2 / 3 && y > height / 3 && y < height * 2 / 3;
                pixels[(y * width + x) as usize] = if center { 0xff8acfc2 } else { 0xff172129 };
            }
        }
        let result = if live {
            DwmSetIconicLivePreviewBitmap(hwnd, bitmap, None, 0)
        } else {
            DwmSetIconicThumbnail(hwnd, bitmap, 0)
        };
        let _ = DeleteObject(bitmap.into());
        result
    }
}

fn preview(hwnd: HWND, blocked: bool) -> Result<(), String> {
    if let Some(state) = native()
        .lock()
        .map_err(|_| "Native state unavailable")?
        .get_mut(&(hwnd.0 as isize))
    {
        state.preview_blocked = blocked;
    }
    let value = BOOL::from(blocked);
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_FORCE_ICONIC_REPRESENTATION,
            &value as *const _ as _,
            std::mem::size_of::<BOOL>() as u32,
        )
        .map_err(|e| e.to_string())?;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_HAS_ICONIC_BITMAP,
            &value as *const _ as _,
            std::mem::size_of::<BOOL>() as u32,
        )
        .map_err(|e| e.to_string())?;
        DwmInvalidateIconicBitmaps(hwnd).map_err(|e| e.to_string())?;
    }
    if blocked {
        placeholder(hwnd, 128, 80, false).map_err(|e| e.to_string())?;
        placeholder(hwnd, 320, 200, true).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn raise(window: &Window, pinned: bool) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        SetWindowPos(
            hwnd,
            Some(if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST }),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    }
    .map_err(|e| e.to_string())
}

pub fn request_recovery() {
    RECOVER.store(true, Ordering::SeqCst);
}

fn set_passthrough(hwnd: HWND, enabled: bool) -> Result<(), String> {
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let layered = {
        let mut states = native().lock().map_err(|_| "Native state unavailable")?;
        let state = states
            .get_mut(&(hwnd.0 as isize))
            .ok_or("Native window not registered")?;
        if enabled {
            if state.layered_before_passthrough.is_none() {
                state.layered_before_passthrough = Some(current & WS_EX_LAYERED.0 as isize != 0);
            }
            true
        } else {
            state
                .layered_before_passthrough
                .take()
                .unwrap_or(current & WS_EX_LAYERED.0 as isize != 0)
        }
    };
    let mut updated = current;
    if enabled {
        updated |= WS_EX_TRANSPARENT.0 as isize;
    } else {
        updated &= !(WS_EX_TRANSPARENT.0 as isize);
    }
    if layered {
        updated |= WS_EX_LAYERED.0 as isize;
    } else {
        updated &= !(WS_EX_LAYERED.0 as isize);
    }
    unsafe {
        windows::Win32::Foundation::SetLastError(windows::Win32::Foundation::WIN32_ERROR(0));
        let previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        if previous == 0 && windows::Win32::Foundation::GetLastError().0 != 0 {
            return Err(windows::core::Error::from_thread().to_string());
        }
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut counter = 0;
        loop {
            let recover = RECOVER.swap(false, Ordering::SeqCst);
            let ids = match app.state::<AppState>().lock() {
                Ok(registry) => registry.windows.keys().cloned().collect::<Vec<_>>(),
                Err(error) => {
                    eprintln!("Privacy registry failed: {error}");
                    break;
                }
            };
            if recover && ids.is_empty() {
                if let Err(error) = recover_empty_workspace(&app) {
                    eprintln!("Unable to create recovery window: {error}");
                }
            }
            for id in ids {
                if let Err(error) = tick(&app, &id, recover) {
                    eprintln!("Privacy policy failed for {}: {error}", id.0);
                }
            }
            counter += 1;
            if counter % 10 == 0 {
                if let Err(error) = crate::storage::flush_if_dirty(&app) {
                    eprintln!("Workspace save failed: {error}");
                }
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    });
}

fn recover_empty_workspace(app: &AppHandle) -> Result<(), String> {
    let host = app
        .state::<AppState>()
        .lock()
        .map_err(|_| "Window registry unavailable")?
        .reserve(crate::window_state::WindowMode::AlwaysVisible, None)?;
    super::create_host(app, &host)?;
    crate::storage::dirty();
    Ok(())
}

pub fn tick(app: &AppHandle, id: &WindowId, recover: bool) -> Result<(), String> {
    let _policy = POLICY.lock().map_err(|_| "Privacy policy unavailable")?;
    let Some(window) = app.get_window(&id.0) else {
        return Ok(());
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let mut cursor = POINT::default();
    let mut rect = RECT::default();
    let cursor_available = match unsafe { GetCursorPos(&mut cursor) } {
        Ok(()) => {
            if CURSOR_UNAVAILABLE.swap(false, Ordering::SeqCst) {
                eprintln!("Input desktop cursor access restored");
            }
            true
        }
        Err(error) => {
            if !CURSOR_UNAVAILABLE.swap(true, Ordering::SeqCst) {
                eprintln!("Cursor unavailable; privacy windows remain hidden: {error}");
            }
            false
        }
    };
    unsafe {
        GetWindowRect(hwnd, &mut rect).map_err(|e| format!("Read Slice bounds: {e}"))?;
    }
    let inside = cursor_available
        && cursor.x >= rect.left
        && cursor.x < rect.right
        && cursor.y >= rect.top
        && cursor.y < rect.bottom;
    let native_guard = native()
        .lock()
        .map_err(|_| "Native state unavailable")?
        .get(&(hwnd.0 as isize))
        .is_some_and(|state| state.guarded != 0);
    let pointer_down = unsafe {
        GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0
            || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0
            || GetAsyncKeyState(VK_MBUTTON.0 as i32) < 0
    };
    let mut gui = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // 查询前台输入线程的真实捕获和菜单归属，其他应用按住鼠标不能阻止本窗隐身。
    let input_guard = cursor_available
        && unsafe { GetGUIThreadInfo(0, &mut gui) }.is_ok()
        && ((pointer_down && owns_input(hwnd, gui.hwndCapture))
            || (gui.flags.0 & (GUI_INMENUMODE.0 | GUI_POPUPMENUMODE.0 | GUI_SYSTEMMENUMODE.0)
                != 0
                && owns_input(hwnd, gui.hwndMenuOwner)));
    let (options, old_hidden, selecting) = {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        let host = registry.windows.get_mut(id).ok_or("Unknown window")?;
        if recover {
            host.options.manual_hidden = false;
            host.options.manual_passthrough = false;
            host.options.privacy_enabled = false;
            host.revision += 1;
            crate::storage::dirty();
        }
        let options = host.options.clone();
        let content = registry.content.get(id).ok_or("No content state")?;
        let composing = content.composing && unsafe { GetForegroundWindow() } == hwnd;
        (options, content.hidden, content.selecting || composing)
    };
    let protected = cursor_available && !old_hidden && (native_guard || input_guard || selecting);
    let hidden = crate::policy::should_hide(&options, inside, protected);
    if let Some(view) = app.get_webview(&crate::content::content_label(id)) {
        let previous = {
            let state = app.state::<AppState>();
            let registry = state.lock().map_err(|_| "Window registry unavailable")?;
            registry.content.get(id).ok_or("No content")?.previous_muted
        };
        if hidden && !options.hidden_audio_allowed && previous.is_none() {
            let previous = super::webview::mute(&view, Some(true))?;
            let state = app.state::<AppState>();
            state
                .lock()
                .map_err(|_| "Window registry unavailable")?
                .content
                .get_mut(id)
                .ok_or("No content")?
                .previous_muted = Some(previous);
        } else if !hidden || options.hidden_audio_allowed {
            if let Some(previous) = previous {
                super::webview::mute(&view, Some(previous))?;
                let state = app.state::<AppState>();
                let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
                registry
                    .content
                    .get_mut(id)
                    .ok_or("No content")?
                    .previous_muted = None;
            }
        }
    }
    let preview_blocked = native()
        .lock()
        .map_err(|_| "Native state unavailable")?
        .get(&(hwnd.0 as isize))
        .is_some_and(|state| state.preview_blocked);
    if preview_blocked != (hidden && !options.hidden_preview_allowed) {
        preview(hwnd, hidden && !options.hidden_preview_allowed)?;
    }
    if hidden != old_hidden {
        // 空区域保留原窗口矩形供全局鼠标命中，但不绘制内容，也不会抢焦点。
        if hidden {
            let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
            if region.is_invalid() {
                return Err("Unable to create hidden window region".into());
            }
            if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
                unsafe {
                    let _ = DeleteObject(region.into());
                }
                return Err("Unable to hide native window region".into());
            }
        } else if unsafe { SetWindowRgn(hwnd, None, true) } == 0 {
            return Err("Unable to restore native window region".into());
        }
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry.content.get_mut(id).ok_or("No content")?.hidden = hidden;
        let host = registry.windows.get_mut(id).ok_or("Unknown window")?;
        host.display = if !hidden {
            DisplayState::Visible
        } else if options.manual_hidden {
            DisplayState::ManualHidden
        } else {
            DisplayState::PrivacyHidden
        };
    }
    let passthrough = hidden || options.manual_passthrough;
    let actual_passthrough =
        unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize != 0;
    let changed = actual_passthrough != passthrough || {
        let state = app.state::<AppState>();
        let registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry.windows.get(id).is_some_and(|host| {
            matches!(host.interaction, InteractionState::Passthrough) != passthrough
        })
    };
    if changed {
        set_passthrough(hwnd, passthrough)?;
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry
            .windows
            .get_mut(id)
            .ok_or("Unknown window")?
            .interaction = if passthrough {
            InteractionState::Passthrough
        } else {
            InteractionState::Interactive
        };
    }
    Ok(())
}

fn owns_input(root: HWND, target: HWND) -> bool {
    !target.0.is_null()
        && (target == root
            || unsafe { GetAncestor(target, GA_ROOT) } == root
            || unsafe { GetAncestor(target, GA_ROOTOWNER) } == root)
}
