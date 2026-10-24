use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WebviewId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowOptions {
    pub privacy_enabled: bool,
    pub manual_hidden: bool,
    pub manual_passthrough: bool,
    pub hidden_residual_alpha: f64,
    pub text_alpha: f64,
    pub background_alpha: f64,
    pub topmost: bool,
    pub hidden_preview_allowed: bool,
    pub hidden_audio_allowed: bool,
}

impl WindowOptions {
    pub fn for_mode(mode: WindowMode) -> Self {
        Self {
            privacy_enabled: mode == WindowMode::Privacy,
            manual_hidden: false,
            manual_passthrough: false,
            hidden_residual_alpha: 0.0,
            text_alpha: 1.0,
            background_alpha: 1.0,
            topmost: false,
            hidden_preview_allowed: false,
            hidden_audio_allowed: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for (key, value) in [
            ("hiddenResidualAlpha", self.hidden_residual_alpha),
            ("textAlpha", self.text_alpha),
            ("backgroundAlpha", self.background_alpha),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!("{key} must be finite and between 0 and 1"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowMode {
    Privacy,
    AlwaysVisible,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    pub id: String,
    pub url: String,
    pub layout_viewport: Viewport,
    pub crop: Crop,
    pub scroll: Point,
    pub zoom: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayState {
    Visible,
    PrivacyHidden,
    ManualHidden,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractionState {
    Interactive,
    Passthrough,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protection {
    Move,
    Resize,
    Pointer,
    Menu,
    Ime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostWindow {
    pub id: WindowId,
    pub control_webview_id: WebviewId,
    pub options: WindowOptions,
    pub revision: u64,
    pub slices: Vec<Slice>,
    pub display: DisplayState,
    pub interaction: InteractionState,
    pub protections: Vec<Protection>,
}

#[derive(Default)]
pub struct Registry {
    pub windows: BTreeMap<WindowId, HostWindow>,
    pub managed_targets: BTreeMap<WebviewIdKey, BTreeSet<WindowId>>,
    pub content: BTreeMap<WindowId, ContentState>,
    next_id: usize,
}

#[derive(Default)]
pub struct ContentState {
    pub slot: crate::geometry::Slot,
    pub selecting: bool,
    pub previous_muted: Option<bool>,
    pub hidden: bool,
    pub nonce: String,
    pub composing: bool,
}

pub type WebviewIdKey = String;
pub type AppState = Mutex<Registry>;

impl Registry {
    pub fn reserve(
        &mut self,
        mode: WindowMode,
        creator: Option<&str>,
    ) -> Result<HostWindow, String> {
        if self.windows.len() >= 16 {
            return Err("Prototype supports at most 16 simultaneous windows".into());
        }
        self.next_id += 1;
        let id = WindowId(format!("window-{:03}", self.next_id));
        let control = format!("control-{}", id.0);
        let host = HostWindow {
            id: id.clone(),
            control_webview_id: WebviewId(control.clone()),
            options: WindowOptions::for_mode(mode),
            revision: 0,
            slices: vec![],
            display: DisplayState::Visible,
            interaction: InteractionState::Interactive,
            protections: vec![],
        };
        self.managed_targets
            .insert(control, BTreeSet::from([id.clone()]));
        if let Some(creator) = creator {
            self.managed_targets
                .entry(creator.into())
                .or_default()
                .insert(id.clone());
        }
        self.windows.insert(id, host.clone());
        self.content
            .insert(host.id.clone(), ContentState::default());
        Ok(host)
    }

    pub fn remove(&mut self, id: &WindowId) {
        self.content.remove(id);
        if let Some(host) = self.windows.remove(id) {
            self.managed_targets.remove(&host.control_webview_id.0);
        }
        for targets in self.managed_targets.values_mut() {
            targets.remove(id);
        }
    }
}
