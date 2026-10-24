use crate::geometry::{contain, Slot};
use crate::window_state::{Crop, Viewport};
use std::sync::mpsc;
use std::time::Duration;
use tauri::Webview;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Controller2, ICoreWebView2Controller3, ICoreWebView2_8,
    COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS, COREWEBVIEW2_COLOR,
};
use windows::core::Interface;
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER},
};

pub fn configure(
    view: &Webview,
    slot: Slot,
    crop: &Crop,
    viewport: &Viewport,
    dpi: f64,
) -> Result<(), String> {
    let placement = contain(slot, crop, viewport, dpi)?;
    let divisor = gcd(viewport.width as u32, viewport.height as u32).max(1) as f64;
    let content_scale = (placement.scale * divisor).floor() / divisor;
    let fit = content_scale / dpi;
    if fit <= 0.0 {
        return Err("Window is too small for this layout viewport".into());
    }
    let width = crop.width * fit;
    let height = crop.height * fit;
    let left = (-crop.x * content_scale).round() as i32;
    let top = (-crop.y * content_scale).round() as i32;
    let parent_x = ((slot.x + (slot.width - width) / 2.0) * dpi).round() as i32;
    let parent_y = ((slot.y + (slot.height - height) / 2.0) * dpi).round() as i32;
    let layout_width = (viewport.width * content_scale).round() as i32;
    let layout_height = (viewport.height * content_scale).round() as i32;
    // 只有裁剪矩形有原生子窗口，contain 留白不能暴露相邻网页区域。
    platform_call(view, move |platform| unsafe {
        let controller = platform.controller();
        let mut parent = HWND::default();
        controller.ParentWindow(&mut parent)?;
        // 同步调整原生容器，避免 wry 的异步 WM_SIZE 覆盖最终 controller bounds。
        SetWindowPos(
            parent,
            None,
            parent_x,
            parent_y,
            (width * dpi).round() as i32,
            (height * dpi).round() as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )?;
        let controller3 = controller.cast::<ICoreWebView2Controller3>()?;
        controller3.SetShouldDetectMonitorScaleChanges(false)?;
        controller3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS)?;
        controller3.SetRasterizationScale(dpi)?;
        controller
            .cast::<ICoreWebView2Controller2>()?
            .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 0,
                R: 0,
                G: 0,
                B: 0,
            })?;
        controller.SetBoundsAndZoomFactor(
            RECT {
                left,
                top,
                right: left + layout_width,
                bottom: top + layout_height,
            },
            fit,
        )?;
        controller.NotifyParentWindowPositionChanged()?;
        Ok(())
    })
}

pub fn notify_moved(view: &Webview) -> Result<(), String> {
    view.with_webview(|platform| {
        if let Err(error) = unsafe { platform.controller().NotifyParentWindowPositionChanged() } {
            eprintln!("WebView parent position notification failed: {error}");
        }
    })
    .map_err(|e| e.to_string())
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

pub fn install_interactions(
    view: &Webview,
    app: tauri::AppHandle,
    id: crate::window_state::WindowId,
) -> Result<(), String> {
    use tauri::Manager;
    use webview2_com::{take_pwstr, WebMessageReceivedEventHandler};
    use windows::core::PWSTR;
    platform_call(view, move |platform| unsafe {
        let core = platform.controller().CoreWebView2()?;
        let mut token = 0;
        core.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut raw = PWSTR::null();
                args.WebMessageAsJson(&mut raw)?;
                let raw = take_pwstr(raw);
                if raw.len() > 512 {
                    return Ok(());
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                    return Ok(());
                };
                let Some(value) = value.get("omnisliceInteraction") else {
                    return Ok(());
                };
                let (Some(nonce), Some(active)) = (
                    value.get("nonce").and_then(|value| value.as_str()),
                    value.get("active").and_then(|value| value.as_bool()),
                ) else {
                    return Ok(());
                };
                let mut source = PWSTR::null();
                args.Source(&mut source)?;
                let source = take_pwstr(source);
                if crate::commands::validate_http_url(&source).is_err() {
                    return Ok(());
                }
                let Some(window) = app.get_window(&id.0) else {
                    return Ok(());
                };
                let Ok(hwnd) = window.hwnd() else {
                    return Ok(());
                };
                let focused =
                    windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == hwnd;
                if let Ok(mut registry) = app.state::<crate::window_state::AppState>().lock() {
                    if let Some(content) = registry.content.get_mut(&id) {
                        if content.nonce == nonce {
                            content.composing = active && focused;
                        }
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        Ok(())
    })
}

pub fn mute(view: &Webview, value: Option<bool>) -> Result<bool, String> {
    platform_call(view, move |platform| unsafe {
        let core = platform
            .controller()
            .CoreWebView2()?
            .cast::<ICoreWebView2_8>()?;
        let mut previous = windows::core::BOOL::default();
        core.IsMuted(&mut previous)?;
        if let Some(value) = value {
            core.SetIsMuted(value)?;
        }
        Ok(previous.as_bool())
    })
}

fn platform_call<T: Send + 'static>(
    view: &Webview,
    work: impl FnOnce(tauri::webview::PlatformWebview) -> windows::core::Result<T> + Send + 'static,
) -> Result<T, String> {
    let (send, receive) = mpsc::sync_channel(1);
    view.with_webview(move |platform| {
        let _ = send.send(work(platform).map_err(|e| e.to_string()));
    })
    .map_err(|e| e.to_string())?;
    receive
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| format!("WebView UI operation failed: {e}"))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub muted: bool,
    pub rasterization_scale: f64,
    pub zoom_factor: f64,
    pub content_scale: f64,
    pub bounds: [i32; 4],
}

pub fn diagnostics(view: &Webview) -> Result<Diagnostics, String> {
    platform_call(view, |platform| unsafe {
        let controller = platform.controller();
        let mut scale = 0.0;
        let mut bounds = RECT::default();
        let mut zoom = 0.0;
        let mut muted = windows::core::BOOL::default();
        controller
            .cast::<ICoreWebView2Controller3>()?
            .RasterizationScale(&mut scale)?;
        controller.Bounds(&mut bounds)?;
        controller.ZoomFactor(&mut zoom)?;
        controller
            .CoreWebView2()?
            .cast::<ICoreWebView2_8>()?
            .IsMuted(&mut muted)?;
        Ok(Diagnostics {
            muted: muted.as_bool(),
            rasterization_scale: scale,
            zoom_factor: zoom,
            content_scale: scale * zoom,
            bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
        })
    })
}
