//! The single definition of what a clip can carry: its properties, the effects and the transitions.
//! The inspector is drawn from it, agents read it through `library_list`, and commands are checked
//! against it, so a name that is not in here cannot end up in a project.

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct EffectDef {
    pub name: &'static str,
    pub hint: &'static str,
    /// Strength a newly added effect starts with, 0 to 100. Stored per clip under `fx.<name>`.
    pub amount: f64,
}

#[derive(Clone, Serialize)]
pub struct TransitionDef {
    pub name: &'static str,
    pub hint: &'static str,
}

/// Which clips a property makes sense on.
#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applies {
    /// Anything with a picture: video, images, graphics, captions.
    Visual,
    /// Clips that play a video or audio file.
    Media,
    /// Clips that make sound.
    Sound,
    /// Graphics and captions.
    Text,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PropKind {
    Number { min: f64, max: f64, step: f64, default: f64, unit: &'static str },
    Bool { default: bool },
    Choice { options: &'static [&'static str], default: &'static str },
    Color { default: &'static str },
}

#[derive(Clone, Serialize)]
pub struct PropDef {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub applies: Applies,
    #[serde(flatten)]
    pub kind: PropKind,
}

#[derive(Clone, Serialize)]
pub struct Library {
    pub properties: Vec<PropDef>,
    pub effects: Vec<EffectDef>,
    pub transitions: Vec<TransitionDef>,
}

pub const BLEND_MODES: &[&str] = &[
    "Normal", "Add", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "Soft Light", "Hard Light", "Difference",
    "Exclusion", "Color Dodge", "Color Burn", "Hue", "Saturation", "Color", "Luminosity",
];
pub const FONTS: &[&str] =
    &["Segoe UI", "Arial", "Impact", "Georgia", "Times New Roman", "Verdana", "Trebuchet MS", "Consolas", "Comic Sans MS"];

fn num(key: &'static str, label: &'static str, group: &'static str, applies: Applies, range: (f64, f64, f64), default: f64, unit: &'static str) -> PropDef {
    let (min, max, step) = range;
    PropDef { key, label, group, applies, kind: PropKind::Number { min, max, step, default, unit } }
}

fn flag(key: &'static str, label: &'static str, group: &'static str, applies: Applies, default: bool) -> PropDef {
    PropDef { key, label, group, applies, kind: PropKind::Bool { default } }
}

pub fn library() -> Library {
    use Applies::*;
    let px = (-4000.0, 4000.0, 1.0);
    let crop = (0.0, 4000.0, 1.0);
    let properties = vec![
        num("zoom_x", "Zoom X", "Transform", Visual, (0.0, 10.0, 0.01), 1.0, "×"),
        num("zoom_y", "Zoom Y", "Transform", Visual, (0.0, 10.0, 0.01), 1.0, "×"),
        flag("zoom_link", "Link zoom", "Transform", Visual, true),
        num("pos_x", "Position X", "Transform", Visual, px, 0.0, "px"),
        num("pos_y", "Position Y", "Transform", Visual, px, 0.0, "px"),
        num("rotation", "Rotation angle", "Transform", Visual, (-360.0, 360.0, 0.1), 0.0, "°"),
        num("anchor_x", "Anchor point X", "Transform", Visual, px, 0.0, "px"),
        num("anchor_y", "Anchor point Y", "Transform", Visual, px, 0.0, "px"),
        num("pitch", "Pitch", "Transform", Visual, (-85.0, 85.0, 0.1), 0.0, "°"),
        num("yaw", "Yaw", "Transform", Visual, (-85.0, 85.0, 0.1), 0.0, "°"),
        flag("flip_h", "Flip horizontal", "Transform", Visual, false),
        flag("flip_v", "Flip vertical", "Transform", Visual, false),
        num("crop_left", "Crop left", "Cropping", Visual, crop, 0.0, "px"),
        num("crop_right", "Crop right", "Cropping", Visual, crop, 0.0, "px"),
        num("crop_top", "Crop top", "Cropping", Visual, crop, 0.0, "px"),
        num("crop_bottom", "Crop bottom", "Cropping", Visual, crop, 0.0, "px"),
        PropDef { key: "blend", label: "Composite mode", group: "Composite", applies: Visual, kind: PropKind::Choice { options: BLEND_MODES, default: "Normal" } },
        num("opacity", "Opacity", "Composite", Visual, (0.0, 100.0, 0.1), 100.0, "%"),
        num("speed", "Speed", "Speed change", Media, (10.0, 1000.0, 1.0), 100.0, "%"),
        num("volume", "Volume", "Audio", Sound, (-60.0, 0.0, 0.1), 0.0, "dB"),
        num("fade_in", "Fade in", "Audio", Sound, (0.0, 10.0, 0.05), 0.0, "s"),
        num("fade_out", "Fade out", "Audio", Sound, (0.0, 10.0, 0.05), 0.0, "s"),
        PropDef { key: "font", label: "Font", group: "Text", applies: Text, kind: PropKind::Choice { options: FONTS, default: "Segoe UI" } },
        num("font_size", "Size", "Text", Text, (10.0, 400.0, 1.0), 100.0, "%"),
        flag("bold", "Bold", "Text", Text, true),
        PropDef { key: "color", label: "Colour", group: "Text", applies: Text, kind: PropKind::Color { default: "#ffffff" } },
        num("tracking", "Tracking", "Text", Text, (-10.0, 50.0, 0.5), 0.0, ""),
        PropDef { key: "stroke_color", label: "Stroke colour", group: "Text", applies: Text, kind: PropKind::Color { default: "#000000" } },
        num("stroke_size", "Stroke size", "Text", Text, (0.0, 30.0, 0.5), 5.0, ""),
        flag("shadow", "Drop shadow", "Text", Text, true),
    ];

    let fx = |name, hint, amount| EffectDef { name, hint, amount };
    let effects = vec![
        fx("Greyscale", "Black and white", 100.0),
        fx("Sepia", "Warm brown tone", 100.0),
        fx("Invert", "Negative colours", 100.0),
        fx("Blur", "Soft focus", 30.0),
        fx("Brightness", "Darker below 50, brighter above", 65.0),
        fx("Contrast", "Flatter below 50, punchier above", 65.0),
        fx("Saturation", "Muted below 50, vivid above", 75.0),
        fx("Hue shift", "Rotate all colours", 25.0),
        fx("Vignette", "Darkened edges", 60.0),
        fx("Zoom punch", "Punch in on the picture", 45.0),
        fx("Shake", "Camera shake", 40.0),
        fx("Rounded corners", "Round the frame, for insets", 30.0),
        fx("Border", "White outline, for insets", 30.0),
        fx("Drop shadow", "Shadow behind the frame, for insets", 50.0),
    ];

    let tr = |name, hint| TransitionDef { name, hint };
    let transitions = vec![
        tr("Cross dissolve", "Blend from the previous clip"),
        tr("Blur dissolve", "Blend through a blur"),
        tr("Dip to black", "Fade through black"),
        tr("Dip to white", "Fade through white"),
        tr("Slide left", "Slides in over the previous clip, moving left"),
        tr("Slide right", "Slides in over the previous clip, moving right"),
        tr("Slide up", "Slides in moving up"),
        tr("Slide down", "Slides in moving down"),
        tr("Push left", "Pushes the previous clip out to the left"),
        tr("Push right", "Pushes the previous clip out to the right"),
        tr("Wipe left", "Reveals along an edge moving left"),
        tr("Wipe right", "Reveals along an edge moving right"),
        tr("Wipe up", "Reveals along an edge moving up"),
        tr("Wipe down", "Reveals along an edge moving down"),
        tr("Iris", "Opens from a circle in the centre"),
        tr("Zoom in", "Grows in from small"),
        tr("Zoom out", "Settles in from oversized"),
        tr("Whoosh zoom", "Fast zoom with blur"),
        tr("Spin", "Rotates in"),
        tr("Glitch", "Short digital glitch"),
    ];
    Library { properties, effects, transitions }
}

pub fn is_effect(name: &str) -> bool {
    library().effects.iter().any(|e| e.name == name)
}

pub fn is_transition(name: &str) -> bool {
    library().transitions.iter().any(|t| t.name == name)
}

pub fn effect_names() -> String {
    library().effects.iter().map(|e| e.name).collect::<Vec<_>>().join(", ")
}

pub fn transition_names() -> String {
    library().transitions.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
}

/// Checks a property value against its definition. Null is always allowed: it resets the property.
pub fn validate_property(key: &str, value: &Value) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    let in_range = |min: f64, max: f64| match value.as_f64() {
        Some(v) if v >= min && v <= max => Ok(()),
        Some(v) => Err(format!("{key} must be between {min} and {max}, got {v}")),
        None => Err(format!("{key} must be a number")),
    };
    if let Some(effect) = key.strip_prefix("fx.") {
        if !is_effect(effect) {
            return Err(format!("unknown effect {effect}; available: {}", effect_names()));
        }
        return in_range(0.0, 100.0);
    }
    let lib = library();
    let Some(def) = lib.properties.iter().find(|p| p.key == key) else {
        let keys: Vec<_> = lib.properties.iter().map(|p| p.key).collect();
        return Err(format!("unknown property {key}; available: name, text, {}, and fx.<effect name>", keys.join(", ")));
    };
    match &def.kind {
        PropKind::Number { min, max, .. } => in_range(*min, *max),
        PropKind::Bool { .. } => value.as_bool().map(|_| ()).ok_or_else(|| format!("{key} must be true or false")),
        PropKind::Choice { options, .. } => match value.as_str() {
            Some(v) if options.contains(&v) => Ok(()),
            _ => Err(format!("{key} must be one of: {}", options.join(", "))),
        },
        PropKind::Color { .. } => match value.as_str() {
            Some(v) if v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()) => Ok(()),
            _ => Err(format!("{key} must be a colour like #ffcc00")),
        },
    }
}
