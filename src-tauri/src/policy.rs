use crate::window_state::WindowOptions;

pub fn should_hide(options: &WindowOptions, inside: bool, protected: bool) -> bool {
    options.manual_hidden || (options.privacy_enabled && !inside && !protected)
}
