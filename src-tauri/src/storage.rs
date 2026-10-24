use crate::{
    commands::validate_http_url,
    geometry::validate_crop,
    window_state::{AppState, Slice, WindowOptions},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Manager};

static DIRTY: AtomicBool = AtomicBool::new(false);
static WRITABLE: AtomicBool = AtomicBool::new(true);
static WRITER: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedWindow {
    pub options: WindowOptions,
    pub slices: Vec<Slice>,
    pub geometry: Geometry,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub version: u32,
    pub windows: Vec<SavedWindow>,
}

pub fn validate(workspace: &Workspace) -> Result<(), String> {
    if workspace.version != 1 {
        return Err("Unsupported workspace version".into());
    }
    if workspace.windows.len() > 16 {
        return Err("Workspace exceeds window limit".into());
    }
    for window in &workspace.windows {
        window.options.validate()?;
        if window.geometry.width < 1
            || window.geometry.height < 1
            || window.geometry.width > 16384
            || window.geometry.height > 16384
        {
            return Err("Invalid saved window size".into());
        }
        if window.slices.len() > 1 {
            return Err("Prototype supports one slice per window".into());
        }
        for slice in &window.slices {
            validate_http_url(&slice.url)?;
            validate_crop(&slice.crop, &slice.layout_viewport)?;
            if !slice.scroll.x.is_finite()
                || !slice.scroll.y.is_finite()
                || slice.scroll.x.abs() > 10000000.0
                || slice.scroll.y.abs() > 10000000.0
                || slice.zoom != 1.0
            {
                return Err("Invalid saved slice state".into());
            }
        }
    }
    Ok(())
}

pub fn clamp(mut window: Geometry, monitors: &[Geometry]) -> Geometry {
    let Some(monitor) = monitors.iter().max_by_key(|monitor| {
        let width = (i64::from(window.x) + i64::from(window.width))
            .min(i64::from(monitor.x) + i64::from(monitor.width))
            - i64::from(window.x).max(i64::from(monitor.x));
        let height = (i64::from(window.y) + i64::from(window.height))
            .min(i64::from(monitor.y) + i64::from(monitor.height))
            - i64::from(window.y).max(i64::from(monitor.y));
        width.max(0) * height.max(0)
    }) else {
        return window;
    };
    window.width = window.width.min(monitor.width).max(1);
    window.height = window.height.min(monitor.height).max(1);
    window.x = window.x.clamp(
        monitor.x,
        monitor
            .x
            .saturating_add((monitor.width - window.width) as i32),
    );
    window.y = window.y.clamp(
        monitor.y,
        monitor
            .y
            .saturating_add((monitor.height - window.height) as i32),
    );
    window
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = if let Some(directory) = std::env::var_os("OMNISLICE_CONFIG_DIR") {
        PathBuf::from(directory)
    } else {
        app.path().app_config_dir().map_err(|e| e.to_string())?
    };
    Ok(directory.join("workspace.json"))
}

pub fn load(app: &AppHandle) -> Result<Option<Workspace>, String> {
    let path = path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let result = (|| {
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
            return Err("Workspace exceeds size limit".into());
        }
        let workspace = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        validate(&workspace)?;
        Ok(Some(workspace))
    })();
    if result.is_err() {
        WRITABLE.store(false, Ordering::SeqCst);
    }
    result
}

pub fn dirty() {
    DIRTY.store(true, Ordering::SeqCst);
}

pub fn flush_if_dirty(app: &AppHandle) -> Result<(), String> {
    if !DIRTY.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    let result = save(app);
    if result.is_err() {
        DIRTY.store(true, Ordering::SeqCst);
    }
    result
}

fn save(app: &AppHandle) -> Result<(), String> {
    if !WRITABLE.load(Ordering::SeqCst) {
        return Err("Invalid existing workspace preserved; saving disabled for this run".into());
    }
    let _writer = WRITER.lock().map_err(|_| "Workspace writer unavailable")?;
    let hosts = app
        .state::<AppState>()
        .lock()
        .map_err(|_| "Window registry unavailable")?
        .windows
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut windows = vec![];
    for host in hosts {
        if let Some(window) = app.get_window(&host.id.0) {
            let position = window.outer_position().map_err(|e| e.to_string())?;
            let size = window.inner_size().map_err(|e| e.to_string())?;
            windows.push(SavedWindow {
                options: host.options,
                slices: host.slices,
                geometry: Geometry {
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                },
            });
        }
    }
    let workspace = Workspace {
        version: 1,
        windows,
    };
    validate(&workspace)?;
    let target = path(app)?;
    fs::create_dir_all(target.parent().ok_or("Invalid workspace path")?)
        .map_err(|e| e.to_string())?;
    let temporary = target.with_extension("json.tmp");
    let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&workspace).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };
    let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| e.to_string())
}
