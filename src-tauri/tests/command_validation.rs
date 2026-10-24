use omnislice_lib::{
    commands::{is_local_control_url, trusted_local_sender, validate_http_url, validate_update},
    geometry::{contain, validate_crop, Slot},
    policy::should_hide,
    window_state::{Crop, Registry, Viewport, WindowMode, WindowOptions},
};

#[test]
fn remote_or_unregistered_senders_cannot_change_windows() {
    let mut registry = Registry::default();
    let first = registry.reserve(WindowMode::Privacy, None).unwrap();
    let second = registry.reserve(WindowMode::AlwaysVisible, None).unwrap();
    let local = url::Url::parse("http://tauri.localhost/index.html").unwrap();
    let remote = url::Url::parse("https://example.org/").unwrap();
    assert!(trusted_local_sender(&registry, &first.control_webview_id.0, &remote).is_err());
    assert!(trusted_local_sender(&registry, "content-window-001", &local).is_err());
    assert!(validate_update(
        &registry,
        &first.control_webview_id.0,
        &local,
        &second.id,
        &first.options
    )
    .is_err());
    assert!(validate_update(
        &registry,
        &first.control_webview_id.0,
        &local,
        &first.id,
        &first.options
    )
    .is_ok());
}

#[test]
fn local_origin_validation_rejects_origin_confusion() {
    for value in [
        "https://tauri.localhost/",
        "http://tauri.localhost.evil/",
        "http://tauri.localhost:4000/",
        "http://tauri.localhost/other.html",
        "http://user@tauri.localhost/",
    ] {
        assert!(!is_local_control_url(&value.parse().unwrap()), "{value}");
    }
}

#[test]
fn only_plain_http_urls_are_accepted() {
    for value in [
        "file:///C:/secret",
        "javascript:alert(1)",
        "https://user:secret@example.org",
        "data:text/html,hi",
        "http://",
    ] {
        assert!(validate_http_url(value).is_err(), "{value}");
    }
    assert!(validate_http_url("https://example.org/chart?a=1#x").is_ok());
}

#[test]
fn privacy_waits_for_drag_release_but_manual_hide_takes_priority() {
    let mut options = WindowOptions::for_mode(WindowMode::Privacy);
    assert!(should_hide(&options, false, false));
    assert!(!should_hide(&options, true, false));
    assert!(!should_hide(&options, false, true));
    assert!(should_hide(&options, false, false));
    options.manual_hidden = true;
    assert!(should_hide(&options, true, true));
    let visible = WindowOptions::for_mode(WindowMode::AlwaysVisible);
    assert!(!should_hide(&visible, false, false));
}

#[test]
fn crop_contain_preserves_ratio_and_source_coordinate_mapping() {
    let crop = Crop {
        x: 160.0,
        y: 150.0,
        width: 720.0,
        height: 480.0,
    };
    let viewport = Viewport {
        width: 1280.0,
        height: 900.0,
    };
    let placement = contain(
        Slot {
            x: 12.0,
            y: 200.0,
            width: 360.0,
            height: 360.0,
        },
        &crop,
        &viewport,
        1.5,
    )
    .unwrap();
    assert_eq!(placement.scale, 0.75);
    assert_eq!(placement.width, 960);
    assert_eq!(placement.height, 675);
    assert_eq!(placement.left, -120);
    assert_eq!(placement.top, -23);
    assert!(validate_crop(
        &Crop {
            width: f64::NAN,
            ..crop.clone()
        },
        &viewport
    )
    .is_err());
    assert!(validate_crop(&Crop { x: 900.0, ..crop }, &viewport).is_err());
}

#[test]
fn closed_windows_release_grants_and_do_not_exhaust_lifetime_ids() {
    let mut registry = Registry::default();
    for _ in 0..30 {
        let host = registry.reserve(WindowMode::Privacy, None).unwrap();
        registry.remove(&host.id);
    }
    assert!(registry.windows.is_empty());
    assert!(registry.managed_targets.is_empty());
    assert!(registry.content.is_empty());
}

#[test]
fn monitor_restore_supports_negative_coordinates_and_missing_screens() {
    use omnislice_lib::storage::{clamp, Geometry};
    let left = Geometry {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1040,
    };
    let primary = Geometry {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    let saved = Geometry {
        x: -1800,
        y: 50,
        width: 540,
        height: 780,
    };
    assert_eq!(clamp(saved, &[primary, left]), saved);
    assert_eq!(clamp(saved, &[primary]).x, 0);
    let oversized = Geometry {
        x: 4000,
        y: 5000,
        width: 3000,
        height: 2000,
    };
    assert_eq!(clamp(oversized, &[primary]), primary);
}

#[test]
fn workspace_rejects_unknown_versions_and_dangerous_urls() {
    use omnislice_lib::storage::{validate, Geometry, SavedWindow, Workspace};
    let window = SavedWindow {
        options: WindowOptions::for_mode(WindowMode::Privacy),
        slices: vec![],
        geometry: Geometry {
            x: 0,
            y: 0,
            width: 540,
            height: 780,
        },
    };
    assert!(validate(&Workspace {
        version: 2,
        windows: vec![window.clone()]
    })
    .is_err());
    let valid = Workspace {
        version: 1,
        windows: vec![window],
    };
    assert!(validate(&valid).is_ok());
    let serialized = serde_json::to_string(&valid).unwrap();
    let restored: Workspace = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored.windows[0].geometry.width, 540);
}
