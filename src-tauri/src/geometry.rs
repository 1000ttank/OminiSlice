use crate::window_state::{Crop, Viewport};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slot {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            x: 12.0,
            y: 260.0,
            width: 516.0,
            height: 480.0,
        }
    }
}

impl Slot {
    pub fn validate(&self) -> Result<(), String> {
        if ![self.x, self.y, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            || self.x < 0.0
            || self.y < 0.0
            || self.width < 1.0
            || self.height < 1.0
            || self.x + self.width > 16384.0
            || self.y + self.height > 16384.0
        {
            return Err("Invalid content bounds".into());
        }
        Ok(())
    }
}

pub fn validate_crop(crop: &Crop, viewport: &Viewport) -> Result<(), String> {
    if ![
        crop.x,
        crop.y,
        crop.width,
        crop.height,
        viewport.width,
        viewport.height,
    ]
    .iter()
    .all(|n| n.is_finite())
        || viewport.width < 1.0
        || viewport.height < 1.0
        || viewport.width > 8192.0
        || viewport.height > 8192.0
        || crop.x < 0.0
        || crop.y < 0.0
        || crop.width < 1.0
        || crop.height < 1.0
        || crop.x + crop.width > viewport.width + 0.01
        || crop.y + crop.height > viewport.height + 0.01
    {
        return Err("Crop must be finite and inside the layout viewport".into());
    }
    Ok(())
}

#[derive(Debug)]
pub struct Placement {
    pub scale: f64,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

pub fn contain(
    slot: Slot,
    crop: &Crop,
    viewport: &Viewport,
    dpi: f64,
) -> Result<Placement, String> {
    slot.validate()?;
    validate_crop(crop, viewport)?;
    if !dpi.is_finite() || dpi <= 0.0 {
        return Err("Invalid DPI".into());
    }
    let scale = (slot.width / crop.width).min(slot.height / crop.height);
    Ok(Placement {
        scale: dpi * scale,
        left: (((slot.width - crop.width * scale) / 2.0 - crop.x * scale) * dpi).round() as i32,
        top: (((slot.height - crop.height * scale) / 2.0 - crop.y * scale) * dpi).round() as i32,
        width: (viewport.width * scale * dpi).round() as i32,
        height: (viewport.height * scale * dpi).round() as i32,
    })
}
