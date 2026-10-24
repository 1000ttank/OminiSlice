use crate::{
    commands::{trusted_local_sender, validate_http_url, validate_target},
    geometry::{validate_crop, Slot},
    window_state::{AppState, Crop, Point, Slice, Viewport, WindowId},
};
use serde::Deserialize;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
};

pub fn content_label(id: &WindowId) -> String {
    format!("content-{}", id.0)
}

fn authorize(app: &AppHandle, sender: &Webview, target: &WindowId) -> Result<(), String> {
    let url = sender.url().map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let registry = state.lock().map_err(|_| "Window registry unavailable")?;
    trusted_local_sender(&registry, sender.label(), &url)?;
    validate_target(&registry, sender.label(), target)
}

#[tauri::command]
pub async fn window_diagnostics(
    app: AppHandle,
    webview: Webview,
    window_id: WindowId,
) -> Result<crate::windows::webview::Diagnostics, String> {
    authorize(&app, &webview, &window_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let view = app
            .get_webview(&content_label(&window_id))
            .ok_or("No content WebView")?;
        crate::windows::webview::diagnostics(&view)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_slice(
    app: AppHandle,
    webview: Webview,
    window_id: WindowId,
    url: String,
) -> Result<(), String> {
    authorize(&app, &webview, &window_id)?;
    let url = validate_http_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || open_internal(&app, &window_id, url, None))
        .await
        .map_err(|e| e.to_string())?
}

pub fn open_internal(
    app: &AppHandle,
    window_id: &WindowId,
    url: url::Url,
    restored: Option<Slice>,
) -> Result<(), String> {
    if let Some(old) = app.get_webview(&content_label(window_id)) {
        old.close().map_err(|e| e.to_string())?;
    }
    let viewport = Viewport {
        width: 1280.0,
        height: 900.0,
    };
    let slice = restored.unwrap_or(Slice {
        id: format!("slice-{}", window_id.0),
        url: url.to_string(),
        crop: Crop {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        },
        layout_viewport: viewport,
        scroll: Point { x: 0.0, y: 0.0 },
        zoom: 1.0,
    });
    let nonce = uuid::Uuid::new_v4().to_string();
    let slot = {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry
            .windows
            .get_mut(window_id)
            .ok_or("Unknown window")?
            .slices = vec![slice];
        let content = registry
            .content
            .get_mut(window_id)
            .ok_or("Missing content state")?;
        content.previous_muted = None;
        content.selecting = false;
        content.composing = false;
        content.nonce = nonce.clone();
        content.slot
    };
    let window = app
        .get_window(&window_id.0)
        .ok_or("Native window unavailable")?;
    let callback_app = app.clone();
    let callback_id = window_id.clone();
    let load_app = app.clone();
    let load_id = window_id.clone();
    window
        .add_child(
            crate::windows::automation_builder(WebviewBuilder::new(
                content_label(window_id),
                WebviewUrl::External(url),
            ))
            .initialization_script(
                include_str!("../../scripts/page-adapter.js")
                    .replace("__OMNISLICE_NONCE__", &nonce),
            )
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .on_navigation(move |url| {
                if url.host_str() == Some("omnislice.invalid") {
                    if url.path() == "/crop" {
                        let payload = url
                            .query_pairs()
                            .find(|(k, _)| k == "data")
                            .map(|(_, v)| v.into_owned());
                        if let Some(payload) = payload.filter(|value| value.len() < 2048) {
                            let app = callback_app.clone();
                            let id = callback_id.clone();
                            tauri::async_runtime::spawn_blocking(move || {
                                if let Err(error) = receive_crop(&app, &id, &payload) {
                                    eprintln!("Crop rejected: {error}");
                                }
                            });
                        }
                    }
                    return false;
                }
                if let Ok(mut registry) = callback_app.state::<AppState>().lock() {
                    if let Some(content) = registry.content.get_mut(&callback_id) {
                        content.selecting = false;
                    }
                }
                validate_http_url(url.as_str()).is_ok()
            })
            .on_page_load(move |_, payload| {
                if payload.event() == tauri::webview::PageLoadEvent::Finished {
                    let app = load_app.clone();
                    let id = load_id.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(error) = restore_scroll(&app, &id)
                            .and_then(|_| layout(&app, &id))
                            .and_then(|_| apply_alpha(&app, &id))
                        {
                            eprintln!("Content initialization failed: {error}");
                        }
                    });
                }
            }),
            LogicalPosition::new(slot.x, slot.y),
            LogicalSize::new(slot.width, slot.height),
        )
        .map_err(|e| e.to_string())?;
    let view = app
        .get_webview(&content_label(window_id))
        .ok_or("Content unavailable")?;
    crate::windows::webview::install_interactions(&view, app.clone(), window_id.clone())?;
    layout(app, window_id)?;
    crate::storage::dirty();
    Ok(())
}

fn restore_scroll(app: &AppHandle, id: &WindowId) -> Result<(), String> {
    if let Some(window) = app.get_window(&id.0) {
        crate::windows::privacy::watch_children(&window)?;
    }
    let scroll = {
        let state = app.state::<AppState>();
        let registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry
            .windows
            .get(id)
            .and_then(|host| host.slices.first())
            .map(|slice| slice.scroll.clone())
    };
    if let (Some(view), Some(scroll)) = (app.get_webview(&content_label(id)), scroll) {
        view.eval(format!("scrollTo({}, {})", scroll.x, scroll.y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_content_bounds(
    app: AppHandle,
    webview: Webview,
    window_id: WindowId,
    slot: Slot,
) -> Result<(), String> {
    authorize(&app, &webview, &window_id)?;
    slot.validate()?;
    {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry
            .content
            .get_mut(&window_id)
            .ok_or("Missing content state")?
            .slot = slot;
    }
    tauri::async_runtime::spawn_blocking(move || layout(&app, &window_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn begin_crop(
    app: AppHandle,
    webview: Webview,
    window_id: WindowId,
) -> Result<(), String> {
    authorize(&app, &webview, &window_id)?;
    if app.get_webview(&content_label(&window_id)).is_none() {
        return Err("Open a webpage before cropping".into());
    }
    {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        let active = registry
            .content
            .get_mut(&window_id)
            .ok_or("Missing content state")?;
        active.selecting = !active.selecting;
    }
    tauri::async_runtime::spawn_blocking(move || {
        layout(&app, &window_id)?;
        let selecting = {
            let state = app.state::<AppState>();
            let registry = state.lock().map_err(|_| "Window registry unavailable")?;
            registry
                .content
                .get(&window_id)
                .ok_or("No content")?
                .selecting
        };
        app.get_webview(&content_label(&window_id))
            .ok_or("Content unavailable")?
            .eval(if selecting {
                "window.__omni?.select()"
            } else {
                "window.__omni?.cancel()"
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    crop: Option<Crop>,
    scroll: Point,
}

fn receive_crop(app: &AppHandle, id: &WindowId, payload: &str) -> Result<(), String> {
    let selection: Selection = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    if !selection.scroll.x.is_finite()
        || !selection.scroll.y.is_finite()
        || selection.scroll.x.abs() > 10000000.0
        || selection.scroll.y.abs() > 10000000.0
    {
        return Err("Invalid scroll position".into());
    }
    {
        let state = app.state::<AppState>();
        let mut registry = state.lock().map_err(|_| "Window registry unavailable")?;
        if !registry
            .content
            .get(id)
            .is_some_and(|content| content.selecting)
        {
            return Err("No crop operation is active".into());
        }
        let slice = registry
            .windows
            .get_mut(id)
            .and_then(|host| host.slices.first_mut())
            .ok_or("No slice")?;
        if let Some(crop) = selection.crop {
            validate_crop(&crop, &slice.layout_viewport)?;
            slice.crop = crop;
            slice.scroll = selection.scroll;
        }
        registry.content.get_mut(id).ok_or("No content")?.selecting = false;
    }
    crate::storage::dirty();
    layout(app, id)
}

pub fn layout(app: &AppHandle, id: &WindowId) -> Result<(), String> {
    let Some(view) = app.get_webview(&content_label(id)) else {
        return Ok(());
    };
    let (slot, slice, selecting) = {
        let state = app.state::<AppState>();
        let registry = state.lock().map_err(|_| "Window registry unavailable")?;
        let content = registry.content.get(id).ok_or("Missing content state")?;
        (
            content.slot,
            registry
                .windows
                .get(id)
                .and_then(|host| host.slices.first())
                .cloned()
                .ok_or("Missing slice")?,
            content.selecting,
        )
    };
    let crop = if selecting {
        Crop {
            x: 0.0,
            y: 0.0,
            width: slice.layout_viewport.width,
            height: slice.layout_viewport.height,
        }
    } else {
        slice.crop
    };
    let dpi = app
        .get_window(&id.0)
        .ok_or("Window unavailable")?
        .scale_factor()
        .map_err(|e| e.to_string())?;
    crate::windows::webview::configure(&view, slot, &crop, &slice.layout_viewport, dpi)
}

pub fn apply_alpha(app: &AppHandle, id: &WindowId) -> Result<(), String> {
    let options = {
        let state = app.state::<AppState>();
        let registry = state.lock().map_err(|_| "Window registry unavailable")?;
        registry
            .windows
            .get(id)
            .ok_or("Unknown window")?
            .options
            .clone()
    };
    if let Some(view) = app.get_webview(&content_label(id)) {
        view.eval(format!(
            "window.__omni?.alpha({}, {})",
            options.text_alpha, options.background_alpha
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
