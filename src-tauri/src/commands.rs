use crate::window_state::{AppState, HostWindow, Registry, WindowId, WindowMode, WindowOptions};
use tauri::{AppHandle, Manager, State, Webview};
use url::Url;

pub fn is_local_control_url(url: &Url) -> bool {
    let origin_is_local = if cfg!(debug_assertions) {
        (url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
            && url.port() == Some(1420))
            || (url.scheme() == "http"
                && url.host_str() == Some("tauri.localhost")
                && url.port().is_none())
    } else {
        url.scheme() == "http" && url.host_str() == Some("tauri.localhost") && url.port().is_none()
    };
    origin_is_local
        && matches!(url.path(), "/" | "/index.html")
        && url.username().is_empty()
        && url.password().is_none()
}

pub fn trusted_local_sender(
    registry: &Registry,
    label: &str,
    url: &Url,
) -> Result<WindowId, String> {
    if !is_local_control_url(url) {
        return Err("Sender URL is not a trusted local control page".into());
    }
    let host = registry
        .windows
        .values()
        .find(|host| host.control_webview_id.0 == label)
        .ok_or("Sender is not a registered control Webview")?;
    if label != format!("control-{}", host.id.0) {
        return Err("Control identity mismatch".into());
    }
    Ok(host.id.clone())
}

pub fn validate_target(registry: &Registry, sender: &str, target: &WindowId) -> Result<(), String> {
    if !registry.windows.contains_key(target) {
        return Err("Unknown target window".into());
    }
    if !registry
        .managed_targets
        .get(sender)
        .is_some_and(|targets| targets.contains(target))
    {
        return Err("Sender is not authorized to manage target window".into());
    }
    Ok(())
}

pub fn validate_update(
    registry: &Registry,
    label: &str,
    url: &Url,
    target: &WindowId,
    options: &WindowOptions,
) -> Result<(), String> {
    trusted_local_sender(registry, label, url)?;
    validate_target(registry, label, target)?;
    options.validate()
}

pub fn validate_http_url(input: &str) -> Result<Url, String> {
    let url = Url::parse(input).map_err(|_| "Invalid URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only HTTP/HTTPS URLs without embedded credentials are allowed".into());
    }
    Ok(url)
}

#[tauri::command]
pub fn list_windows(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<Vec<HostWindow>, String> {
    let url = webview.url().map_err(|error| error.to_string())?;
    let registry = state.lock().map_err(|_| "Window registry unavailable")?;
    trusted_local_sender(&registry, webview.label(), &url)?;
    Ok(registry
        .windows
        .values()
        .filter(|host| {
            registry
                .managed_targets
                .get(webview.label())
                .is_some_and(|targets| targets.contains(&host.id))
        })
        .cloned()
        .collect())
}

#[tauri::command]
pub async fn create_window(
    app: AppHandle,
    webview: Webview,
    mode: WindowMode,
) -> Result<HostWindow, String> {
    let url = webview.url().map_err(|error| error.to_string())?;
    let host = {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        trusted_local_sender(&registry, webview.label(), &url)?;
        registry.reserve(mode, Some(webview.label()))?
    };
    let result_host = host.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || crate::windows::create_host(&app, &host))
            .await
            .map_err(|error| error.to_string())?;
    result?;
    Ok(result_host)
}

#[tauri::command]
pub async fn update_window_options(
    app: AppHandle,
    webview: Webview,
    window_id: WindowId,
    options: WindowOptions,
) -> Result<HostWindow, String> {
    let url = webview.url().map_err(|error| error.to_string())?;
    let pin_changed = {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        validate_update(&registry, webview.label(), &url, &window_id, &options)?;
        let host = registry
            .windows
            .get_mut(&window_id)
            .ok_or("Unknown target window")?;
        let changed = host.options.topmost != options.topmost;
        host.options = options;
        host.revision = host
            .revision
            .checked_add(1)
            .ok_or("Window revision exhausted")?;
        changed
    };
    tauri::async_runtime::spawn_blocking(move || {
        let host = {
            let state = app.state::<AppState>();
            let registry = state.lock().map_err(|_| "Window registry unavailable")?;
            registry
                .windows
                .get(&window_id)
                .cloned()
                .ok_or("Unknown target window")?
        };
        let native_window = app
            .get_window(&window_id.0)
            .ok_or("Native target window unavailable")?;
        if pin_changed {
            crate::windows::privacy::raise(&native_window, host.options.topmost)?;
        }
        crate::content::apply_alpha(&app, &window_id)?;
        crate::windows::privacy::tick(&app, &window_id, false)?;
        crate::storage::dirty();
        Ok(host)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn raise_window(
    app: AppHandle,
    webview: Webview,
    window_id: WindowId,
) -> Result<(), String> {
    let url = webview.url().map_err(|e| e.to_string())?;
    {
        let state = app.state::<AppState>();
        let registry = state.lock().map_err(|_| "Window registry unavailable")?;
        trusted_local_sender(&registry, webview.label(), &url)?;
        validate_target(&registry, webview.label(), &window_id)?;
        if !registry
            .windows
            .get(&window_id)
            .ok_or("Unknown target")?
            .options
            .topmost
        {
            return Err("Pin the window before raising it".into());
        }
    }
    let window = app
        .get_window(&window_id.0)
        .ok_or("Native window unavailable")?;
    crate::windows::privacy::raise(&window, true)
}
