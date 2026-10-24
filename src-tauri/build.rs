fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_windows",
            "create_window",
            "update_window_options",
            "raise_window",
            "open_slice",
            "begin_crop",
            "set_content_bounds",
            "window_diagnostics",
        ]),
    ))
    .expect("Tauri build failed");
}
