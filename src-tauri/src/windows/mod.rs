use crate::commands::is_local_control_url;
use crate::window_state::{AppState, HostWindow};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, WindowBuilder,
};
pub mod privacy;
pub mod webview;

pub fn automation_builder(builder: WebviewBuilder<tauri::Wry>) -> WebviewBuilder<tauri::Wry> {
    if !cfg!(debug_assertions) {
        return builder;
    }
    let port = std::env::var("OMNISLICE_AUTOMATION_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port >= 1024);
    if let (Some(port), Some(directory)) = (port, std::env::var_os("OMNISLICE_CONFIG_DIR")) {
        return builder
            .additional_browser_args(&format!("--remote-debugging-port={port}"))
            .data_directory(std::path::PathBuf::from(directory).join("WebView2-test"));
    }
    builder
}

pub fn create_host(app: &AppHandle, host: &HostWindow) -> Result<(), String> {
    let result = create_local_host(app, host);
    if result.is_err() {
        if let Some(window) = app.get_window(&host.id.0) {
            if let Err(error) = window.destroy() {
                eprintln!("Failed to roll back host: {error}");
            }
        }
        app.state::<AppState>()
            .lock()
            .map_err(|_| "Window registry unavailable")?
            .remove(&host.id);
    }
    result
}

fn create_local_host(app: &AppHandle, host: &HostWindow) -> Result<(), String> {
    let index = host
        .id
        .0
        .trim_start_matches("window-")
        .parse::<u32>()
        .map_err(|error| error.to_string())?;
    let window = WindowBuilder::new(app, &host.id.0)
        .title(format!("OmniSlice — {}", host.id.0))
        .inner_size(540.0, 780.0)
        .min_inner_size(380.0, 520.0)
        .position(80.0 + f64::from((index - 1) % 3) * 565.0, 70.0)
        .decorations(false)
        .transparent(true)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?;
    let id = host.id.clone();
    let app_for_close = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Moved(_)) {
            if let Some(view) = app_for_close.get_webview(&crate::content::content_label(&id)) {
                if let Err(error) = webview::notify_moved(&view) {
                    eprintln!("WebView move synchronization failed: {error}");
                }
            }
        }
        if matches!(
            event,
            tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::ScaleFactorChanged { .. }
        ) {
            crate::storage::dirty();
        }
        if matches!(event, tauri::WindowEvent::Destroyed) {
            match app_for_close.state::<AppState>().lock() {
                Ok(mut registry) => registry.remove(&id),
                Err(error) => eprintln!("Failed to unregister window: {error}"),
            }
            crate::storage::dirty();
        }
    });
    let url = format!("index.html?windowId={}", host.id.0);
    window
        .add_child(
            automation_builder(
                WebviewBuilder::new(&host.control_webview_id.0, WebviewUrl::App(url.into()))
                    .on_navigation(is_local_control_url)
                    .auto_resize(),
            ),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(540.0, 780.0),
        )
        .map_err(|error| error.to_string())?;
    privacy::install(&window)?;
    privacy::tick(app, &host.id, false)?;
    window.show().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    eprintln!(
        "Native host {} HWND={:?} control={}",
        host.id.0,
        window.hwnd().map_err(|error| error.to_string())?,
        host.control_webview_id.0
    );
    Ok(())
}
