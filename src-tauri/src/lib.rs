pub mod commands;
pub mod content;
pub mod geometry;
pub mod policy;
pub mod storage;
pub mod window_state;
pub mod windows;

use tauri::Manager;
use window_state::{AppState, Registry, WindowMode};

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new(Registry::default()))
        .invoke_handler(tauri::generate_handler![
            commands::list_windows,
            commands::create_window,
            commands::update_window_options,
            commands::raise_window,
            content::open_slice,
            content::begin_crop,
            content::set_content_bounds,
            content::window_diagnostics
        ])
        .setup(|app| {
            let restore =
                tauri::menu::MenuItem::with_id(app, "restore", "恢复所有窗口", true, None::<&str>)?;
            let quit =
                tauri::menu::MenuItem::with_id(app, "quit", "退出 OmniSlice", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&restore, &quit])?;
            let pixels = [138, 207, 194, 255].repeat(32 * 32);
            tauri::tray::TrayIconBuilder::with_id("omnislice")
                .icon(tauri::image::Image::new_owned(pixels, 32, 32))
                .tooltip("OmniSlice · 恢复窗口")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "restore" => windows::privacy::request_recovery(),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            let handle = app.handle().clone();
            // Windows 的 Webview 创建必须离开同步命令与主线程回调，避免事件循环互等。
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = initialize(&handle) {
                    eprintln!("Failed to create initial native host: {error}");
                    handle.exit(1);
                    return;
                }
                windows::privacy::start(handle);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Unable to run OmniSlice");
}

fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    // 快捷键属于常驻隐藏宿主，不随任一切片窗口关闭而失效。
    let recovery = tauri::WindowBuilder::new(app, "recovery-host")
        .title("OmniSlice recovery")
        .visible(false)
        .skip_taskbar(true)
        .build()
        .map_err(|e| format!("Create recovery host: {e}"))?;
    windows::privacy::install(&recovery).map_err(|e| format!("Install recovery hooks: {e}"))?;
    let saved = match storage::load(app) {
        Ok(saved) => saved,
        Err(error) => {
            eprintln!("Workspace restore failed; original file preserved: {error}");
            None
        }
    };
    if let Some(workspace) = saved.filter(|workspace| !workspace.windows.is_empty()) {
        let monitors = app
            .available_monitors()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|monitor| {
                let area = monitor.work_area();
                storage::Geometry {
                    x: area.position.x,
                    y: area.position.y,
                    width: area.size.width,
                    height: area.size.height,
                }
            })
            .collect::<Vec<_>>();
        for saved in workspace.windows {
            let mut host = app
                .state::<AppState>()
                .lock()
                .map_err(|_| "Window registry unavailable")?
                .reserve(WindowMode::AlwaysVisible, None)?;
            host.options = saved.options;
            // 恢复后先解除手动隐藏/穿透，隐私仍由鼠标位置决定。
            host.options.manual_hidden = false;
            host.options.manual_passthrough = false;
            app.state::<AppState>()
                .lock()
                .map_err(|_| "Window registry unavailable")?
                .windows
                .insert(host.id.clone(), host.clone());
            windows::create_host(app, &host)?;
            let window = app
                .get_window(&host.id.0)
                .ok_or("Restored window unavailable")?;
            let inner = window.inner_size().map_err(|e| e.to_string())?;
            let outer = window.outer_size().map_err(|e| e.to_string())?;
            let geometry = storage::clamp_inner(
                saved.geometry,
                &monitors,
                outer.width.saturating_sub(inner.width),
                outer.height.saturating_sub(inner.height),
            );
            window
                .set_size(tauri::PhysicalSize::new(geometry.width, geometry.height))
                .map_err(|e| e.to_string())?;
            window
                .set_position(tauri::PhysicalPosition::new(geometry.x, geometry.y))
                .map_err(|e| e.to_string())?;
            if host.options.topmost {
                windows::privacy::raise(&window, true)?;
            }
            if let Some(slice) = saved.slices.into_iter().next() {
                let url = commands::validate_http_url(&slice.url)?;
                content::open_internal(app, &host.id, url, Some(slice))?;
            }
        }
    } else {
        for mode in [WindowMode::Privacy, WindowMode::AlwaysVisible] {
            let host = app
                .state::<AppState>()
                .lock()
                .map_err(|_| "Window registry unavailable")?
                .reserve(mode, None)?;
            windows::create_host(app, &host)?;
        }
    }
    Ok(())
}
