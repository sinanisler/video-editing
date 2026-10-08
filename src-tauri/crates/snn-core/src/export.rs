//! Turns a project into FFmpeg commands that render it to a video file.
//!
//! The timeline is rendered in chunks of a few seconds, each by one FFmpeg run that reads only the
//! pieces of footage it needs, and the chunks are then joined without re-encoding the picture. An
//! edit with hundreds of cuts therefore never opens hundreds of files at once.
//!
//! Pictures are drawn the way `src/look.ts` draws them in the preview: the same transform, crop,
//! effects and transitions, restated as FFmpeg filters. Text (cards and captions) is not restated:
//! the window renders it with its own CSS and hands over images, see [`TextTrack`].
//!
//! Nothing here runs FFmpeg. [`plan`] is a pure function, so it can be tested without it.

use crate::library::{library, Library, PropKind};
use crate::model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};

pub const SAMPLE_RATE: i64 = 48_000;
/// Transparent border kept around a layer while it is transformed, so its edges stay clean.
const MARGIN: f64 = 16.0;
const CHUNK_SECONDS: f64 = 20.0;
const MAX_INPUTS: usize = 28;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    High,
    #[default]
    Standard,
    Small,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportRange {
    #[default]
    Whole,
    /// Between the in and out points; a missing point means the start or the end.
    InOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioStreams {
    /// The first audio stream of each file, which is what the preview plays.
    #[default]
    First,
    /// Every audio stream of each file mixed together, for recordings with the microphone on its own stream.
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportSettings {
    /// Output file. The extension picks the container: mp4, mov or mkv.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    #[serde(default)]
    pub quality: Quality,
    /// An encoder from [`ENCODERS`], or empty for the best one the machine has.
    #[serde(default)]
    pub encoder: String,
    #[serde(default)]
    pub range: ExportRange,
    #[serde(default)]
    pub audio: AudioStreams,
}

#[derive(Clone, Debug, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub quality: Quality,
}

/// Ready-made settings. The first two follow the project so they always fit it.
pub fn presets(p: &Project) -> Vec<Preset> {
    let s = &p.settings;
    let fps = s.fps_num as f64 / s.fps_den as f64;
    let tall = |h: u32| (((s.width as f64 / s.height as f64 * h as f64) / 2.0).round() as u32 * 2, h);
    let preset = |id, label, hint, (width, height): (u32, u32), fps, quality| Preset { id, label, hint, width, height, fps, quality };
    vec![
        preset("project", "Same as project", "The project's size and frame rate", (s.width, s.height), fps, Quality::Standard),
        preset("youtube_1080p60", "YouTube 1080p60", "Smooth motion for gameplay recorded at 60 fps", tall(1080), 60.0, Quality::High),
        preset("youtube_1080p30", "YouTube 1080p30", "Smaller file, fine for slow footage", tall(1080), 30.0, Quality::High),
        preset("youtube_1440p60", "YouTube 1440p60", "Upscaled; YouTube gives 1440p uploads a better codec", tall(1440), 60.0, Quality::High),
        preset("draft_720p", "Draft 720p", "Fast and small, for checking an edit", tall(720), 30.0, Quality::Small),
    ]
}

/// H.264 encoders in order of preference: graphics-card encoders first, then software.
pub const ENCODERS: [&str; 4] = ["h264_nvenc", "h264_qsv", "h264_amf", "libx264"];

fn encoder_args(encoder: &str, quality: Quality, fps: f64) -> Result<Vec<String>, String> {
    let level = |high: u32, standard: u32, small: u32| match quality {
        Quality::High => high,
        Quality::Standard => standard,
        Quality::Small => small,
    };
    let q = level(19, 23, 28).to_string();
    let args: Vec<&str> = match encoder {
        // Each chunk carries its own headers, so the joined file plays whatever the chunks' settings were.
        "libx264" => vec!["-preset", "medium", "-crf", ["17", "20", "25"][level(0, 1, 2) as usize], "-pix_fmt", "yuv420p", "-x264-params", "repeat-headers=1"],
        "h264_nvenc" => vec!["-preset", "p5", "-rc", "vbr", "-cq", &q, "-b:v", "0", "-pix_fmt", "yuv420p"],
        "h264_qsv" => vec!["-global_quality", &q, "-pix_fmt", "nv12"],
        "h264_amf" => vec!["-rc", "cqp", "-qp_i", &q, "-qp_p", &q, "-qp_b", &q, "-quality", "quality", "-pix_fmt", "yuv420p"],
        other => return Err(format!("unknown encoder {other}; available: {}", ENCODERS.join(", "))),
    };
    let mut out = vec!["-c:v".to_string(), encoder.to_string()];
    out.extend(args.into_iter().map(String::from));
    // A keyframe every two seconds, which is what YouTube asks for.
    out.extend(["-g".into(), ((fps * 2.0).round() as i64).to_string()]);
    out.extend(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"].map(String::from));
    Ok(out)
}

/// What the window drew for one track: which image shows over which timeline frames.
/// Frames no span covers have no text.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TextTrack {
    pub track: String,
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextSpan {
    pub from: Frame,
    pub to: Frame,
    /// File name inside the job folder.
    pub image: String,
    /// The part of the picture that has anything in it: x, y, width, height. Without it the whole frame is laid over.
    #[serde(default)]
    pub rect: Option<[i64; 4]>,
}

/// What FFmpeg found in a media file.
#[derive(Clone, Copy, Debug, Default)]
pub struct MediaInfo {
    pub audio_streams: usize,
}

pub struct Chunk {
    pub args: Vec<String>,
    pub frames: i64,
    pub file: PathBuf,
}

pub struct Plan {
    /// Text files (filter graphs and image lists) to write before running anything.
    pub files: Vec<(PathBuf, String)>,
    pub chunks: Vec<Chunk>,
    /// Joins the chunks into the output file.
    pub finish: Vec<String>,
    pub total_frames: i64,
    /// Things the export cannot draw the way the preview does.
    pub warnings: Vec<String>,
}

/// A number as FFmpeg wants it: plain decimals, no exponent, no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.5}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

fn rate(fps: f64) -> String {
    let ntsc = (fps * 1001.0 / 1000.0).round();
    if (fps - ntsc * 1000.0 / 1001.0).abs() < 0.005 && (fps - fps.round()).abs() > 0.005 {
        format!("{}/1001", ntsc as i64 * 1000)
    } else {
        num(fps)
    }
}

fn path_arg(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A clip's properties with the library's defaults, as `reader` in `look.ts` gives them.
struct Props<'a> {
    c: &'a Clip,
    lib: &'a Library,
}

impl Props<'_> {
    fn n(&self, key: &str) -> f64 {
        if let Some(v) = self.c.props.get(key).and_then(Value::as_f64) {
            return v;
        }
        match self.lib.properties.iter().find(|p| p.key == key).map(|p| &p.kind) {
            Some(PropKind::Number { default, .. }) => *default,
            _ => 0.0,
        }
    }

    fn b(&self, key: &str) -> bool {
        if let Some(v) = self.c.props.get(key).and_then(Value::as_bool) {
            return v;
        }
        matches!(self.lib.properties.iter().find(|p| p.key == key).map(|p| &p.kind), Some(PropKind::Bool { default: true }))
    }

    fn blend(&self) -> &str {
        self.c.props.get("blend").and_then(Value::as_str).unwrap_or("Normal")
    }

    /// Strength of an effect, 0 to 100, or `None` when the clip does not have it.
    fn fx(&self, name: &str) -> Option<f64> {
        if !self.c.effects.iter().any(|e| e == name) {
            return None;
        }
        let stored = self.c.props.get(&format!("fx.{name}")).and_then(Value::as_f64);
        stored.or_else(|| self.lib.effects.iter().find(|e| e.name == name).map(|e| e.amount)).or(Some(50.0))
    }
}

/// `colorchannelmixer` for a 3×3 colour matrix, rows are output red, green, blue.
fn matrix(m: [f64; 9]) -> String {
    let keys = ["rr", "rg", "rb", "gr", "gg", "gb", "br", "bg", "bb"];
    let parts: Vec<String> = keys.iter().zip(m).map(|(k, v)| format!("{k}={}", num(v))).collect();
    format!("colorchannelmixer={}", parts.join(":"))
}

/// The CSS filter effects, with the matrices the CSS specification gives for them.
fn colour_filter(name: &str, amount: f64, height: f64) -> Option<String> {
    let a = amount / 100.0;
    let lut = |expr: String| format!("lutrgb=r='{expr}':g='{expr}':b='{expr}'");
    Some(match name {
        "Greyscale" => {
            let g = 1.0 - a;
            matrix([
                0.2126 + 0.7874 * g, 0.7152 - 0.7152 * g, 0.0722 - 0.0722 * g,
                0.2126 - 0.2126 * g, 0.7152 + 0.2848 * g, 0.0722 - 0.0722 * g,
                0.2126 - 0.2126 * g, 0.7152 - 0.7152 * g, 0.0722 + 0.9278 * g,
            ])
        }
        "Sepia" => {
            let g = 1.0 - a;
            matrix([
                0.393 + 0.607 * g, 0.769 - 0.769 * g, 0.189 - 0.189 * g,
                0.349 - 0.349 * g, 0.686 + 0.314 * g, 0.168 - 0.168 * g,
                0.272 - 0.272 * g, 0.534 - 0.534 * g, 0.131 + 0.869 * g,
            ])
        }
        "Saturation" => {
            let s = amount / 50.0;
            matrix([
                0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s,
                0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s,
                0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s,
            ])
        }
        "Hue shift" => {
            let (s, c) = (amount * 3.6 * PI / 180.0).sin_cos();
            matrix([
                0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928,
                0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.140, 0.072 - c * 0.072 - s * 0.283,
                0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072,
            ])
        }
        "Invert" => lut(format!("val*{}+{}", num(1.0 - 2.0 * a), num(255.0 * a))),
        "Brightness" => lut(format!("clip(val*{},0,255)", num(amount / 50.0))),
        "Contrast" => lut(format!("clip((val-127.5)*{}+127.5,0,255)", num(amount / 50.0))),
        "Blur" => format!("gblur=sigma={}", num((amount * 0.04 * height / 100.0).max(0.01))),
        _ => return None,
    })
}

/// FFmpeg has these blend modes; the four that work on hue and saturation it does not.
fn blend_mode(name: &str) -> Option<&'static str> {
    Some(match name {
        "Add" => "addition",
        "Multiply" => "multiply",
        "Screen" => "screen",
        "Overlay" => "overlay",
        "Darken" => "darken",
        "Lighten" => "lighten",
        "Soft Light" => "softlight",
        "Hard Light" => "hardlight",
        "Difference" => "difference",
        "Exclusion" => "exclusion",
        "Color Dodge" => "dodge",
        "Color Burn" => "burn",
        _ => return None,
    })
}

/// A transition running over part of a clip.
#[derive(Clone, Copy)]
struct Edge<'a> {
    name: &'a str,
    /// Timeline frame the clip starts at (for the way in) or ends at (for the way out).
    at: Frame,
    frames: Frame,
    out: bool,
    /// The clip follows another on its track, which a dip then fades from.
    has_previous: bool,
}

/// How a transition changes a layer while it runs. Expressions use `ld(0)` for progress 0..1,
/// `ld(1)` for eased progress and `ld(2)` for the whole timeline frame, as `TRANSITIONS` in
/// `look.ts` uses `t`, `e` and `frame`.
#[derive(Default)]
struct Motion {
    /// Multiplies the layer's opacity; may use the pixel position `X`, `Y`.
    alpha: Option<String>,
    tx: Option<String>,
    ty: Option<String>,
    scale: Option<String>,
    /// Radians, clockwise.
    rotate: Option<String>,
    /// Blur at progress 0, in pixels, fading to none.
    blur: Option<f64>,
    glitch: bool,
    /// A flat colour over the layer and its opacity.
    flat: Option<(&'static str, String)>,
}

const NOISE: &str = "(sin((ld(2)+{k})*127.1)*43758.5453-floor(sin((ld(2)+{k})*127.1)*43758.5453))";

fn noise(k: i32) -> String {
    NOISE.replace("{k}", &k.to_string())
}

fn motion(edge: &Edge, w: f64, h: f64) -> Motion {
    let mut m = Motion::default();
    let leaves = format!("(1-ld(1))*{}", num(w));
    let rises = format!("(1-ld(1))*{}", num(h));
    let some = |s: &str| Some(s.to_string());
    match edge.name {
        "Cross dissolve" => m.alpha = some("ld(1)"),
        "Blur dissolve" => {
            m.alpha = some("ld(1)");
            m.blur = Some(3.0 * h / 100.0);
        }
        "Dip to black" | "Dip to white" => {
            let colour = if edge.name == "Dip to black" { "black" } else { "white" };
            if edge.has_previous && !edge.out {
                m.alpha = some("gte(ld(0),0.5)");
                m.flat = Some((colour, "1-abs(2*ld(0)-1)".into()));
            } else {
                m.flat = Some((colour, "1-ld(0)".into()));
            }
        }
        "Slide left" | "Push left" => m.tx = Some(leaves),
        "Slide right" | "Push right" => m.tx = Some(format!("(0-{leaves})")),
        "Slide up" => m.ty = Some(rises),
        "Slide down" => m.ty = Some(format!("(0-{rises})")),
        "Wipe left" => m.alpha = some("gte(X,W*(1-ld(1)))"),
        "Wipe right" => m.alpha = some("lte(X,W*ld(1))"),
        "Wipe up" => m.alpha = some("gte(Y,H*(1-ld(1)))"),
        "Wipe down" => m.alpha = some("lte(Y,H*ld(1))"),
        "Iris" => m.alpha = some("lte(hypot(X-W/2,Y-H/2),ld(1)*0.75*hypot(W,H)/sqrt(2))"),
        "Zoom in" => {
            m.scale = some("(0.2+0.8*ld(1))");
            m.alpha = some("min(1,2*ld(1))");
        }
        "Zoom out" => {
            m.scale = some("(1+0.8*(1-ld(1)))");
            m.alpha = some("min(1,2*ld(1))");
        }
        "Whoosh zoom" => {
            m.scale = some("(1+1.5*(1-ld(1)))");
            m.alpha = some("min(1,3*ld(1))");
            m.blur = Some(4.0 * h / 100.0);
        }
        "Spin" => {
            m.rotate = some("((ld(1)-1)*PI)");
            m.scale = some("(0.3+0.7*ld(1))");
            m.alpha = some("min(1,2*ld(1))");
        }
        "Glitch" => {
            m.tx = Some(format!("({}-0.5)*{}*(1-ld(0))", noise(0), num(0.12 * w)));
            m.ty = Some(format!("({}-0.5)*{}*(1-ld(0))", noise(9), num(0.04 * h)));
            m.alpha = Some(format!("if(lt({},(1-ld(0))*0.5),0.25,1)", noise(5)));
            m.glitch = true;
        }
        _ => {}
    }
    m
}

/// One stretch of one clip inside a chunk, drawn with one set of filters.
struct Piece<'a> {
    c: &'a Clip,
    m: &'a MediaItem,
    /// Output frames, counted from the start of the export.
    from: i64,
    to: i64,
    edge: Option<Edge<'a>>,
    /// The clip is held on its last frame under the next clip's transition, which may push it aside.
    leaving: Option<Edge<'a>>,
}

struct Builder<'a> {
    p: &'a Project,
    lib: Library,
    s: &'a ExportSettings,
    w: f64,
    h: f64,
    /// Export pixels per project pixel.
    kx: f64,
    ky: f64,
    fps: f64,
    project_fps: f64,
    /// Timeline frames per output frame.
    step: f64,
    /// Timeline frame the export starts at.
    start: Frame,
    info: &'a dyn Fn(&MediaItem) -> MediaInfo,
    warnings: Vec<String>,
}

/// The inputs and filter graph of one chunk as they are put together.
#[derive(Default)]
struct Graph {
    inputs: Vec<Vec<String>>,
    chains: Vec<String>,
    video: String,
    /// Plain full-frame pieces of the lowest track, as chunk frames and a label. They are joined end
    /// to end into the picture everything else is drawn on, which is far cheaper than laying them over black.
    base: Vec<(i64, i64, String)>,
    sounds: Vec<String>,
    labels: usize,
}

impl Graph {
    fn input(&mut self, args: Vec<String>) -> usize {
        self.inputs.push(args);
        self.inputs.len() - 1
    }

    fn label(&mut self, prefix: &str) -> String {
        self.labels += 1;
        format!("{prefix}{}", self.labels)
    }
}

impl<'a> Builder<'a> {
    /// Output frame for a timeline frame.
    fn out_frame(&self, f: Frame) -> i64 {
        ((f - self.start) as f64 / self.step).round() as i64
    }

    /// Timeline frame, possibly between two whole ones, that an output frame shows.
    fn timeline(&self, k: i64) -> f64 {
        self.start as f64 + k as f64 * self.step
    }

    fn warn(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    /// Sets `ld(0)`, `ld(1)` and `ld(2)` for a transition; `n` is the filter's name for the frame number.
    fn prelude(&self, piece: &Piece, edge: &Edge, n: &str) -> String {
        let frame = format!("({}+{n}*{})", num(self.timeline(piece.from)), num(self.step));
        let t = match edge.out {
            false => format!("({frame}-{}+0.5)/{}", edge.at, edge.frames),
            true => format!("({}-{frame}-0.5)/{}", edge.at, edge.frames),
        };
        format!("st(0,clip({t},0,1));st(1,if(lt(ld(0),0.5),2*ld(0)*ld(0),1-pow(2-2*ld(0),2)/2));st(2,floor({frame}));")
    }

    /// Where a point of the layer lands on the stage, following `clipLook`: scale, rotate, tilt
    /// and move, around the anchor point.
    fn place(&self, r: &Props, x: f64, y: f64) -> (f64, f64) {
        let punch = 1.0 + r.fx("Zoom punch").unwrap_or(0.0) / 100.0 * 0.4;
        let sx = r.n("zoom_x") * punch * if r.b("flip_h") { -1.0 } else { 1.0 };
        let sy = r.n("zoom_y") * punch * if r.b("flip_v") { -1.0 } else { 1.0 };
        let origin = (self.w / 2.0 + r.n("anchor_x") * self.kx, self.h / 2.0 - r.n("anchor_y") * self.ky);
        let (x, y) = ((x - origin.0) * sx, (y - origin.1) * sy);
        let (sin, cos) = (r.n("rotation") * PI / 180.0).sin_cos();
        let (mut x, mut y) = (x * cos - y * sin, x * sin + y * cos);
        let (pitch, yaw) = (r.n("pitch") * PI / 180.0, r.n("yaw") * PI / 180.0);
        if pitch != 0.0 || yaw != 0.0 {
            // CSS `perspective(160cqh) rotateX(pitch) rotateY(yaw)`, applied right to left.
            let distance = 1.6 * self.h;
            let (x1, z1) = (x * yaw.cos(), -x * yaw.sin());
            let (y2, z2) = (y * pitch.cos() - z1 * pitch.sin(), y * pitch.sin() + z1 * pitch.cos());
            let k = distance / (distance - z2).max(distance * 0.05);
            (x, y) = (x1 * k, y2 * k);
        }
        (x + origin.0 + r.n("pos_x") * self.kx, y + origin.1 - r.n("pos_y") * self.ky)
    }

    /// Extends `chain` so one decoded picture becomes a stage-sized layer, without anything that
    /// moves. Returns whether the layer may have see-through parts.
    fn still_look(&mut self, g: &mut Graph, piece: &Piece, r: &Props, chain: &mut String) -> bool {
        let (w, h, fps) = (self.w, self.h, rate(self.fps));
        let colours: Vec<String> = piece.c.effects.iter().filter_map(|e| colour_filter(e, r.fx(e).unwrap_or(0.0), h)).collect();
        let crop = [r.n("crop_left") * self.kx, r.n("crop_right") * self.kx, r.n("crop_top") * self.ky, r.n("crop_bottom") * self.ky].map(f64::round);
        let (round, border, vignette) = (r.fx("Rounded corners"), r.fx("Border"), r.fx("Vignette"));
        let fills = piece.m.width > 0 && (piece.m.width as f64 * h - piece.m.height as f64 * w).abs() < 2.0 * piece.m.height as f64;
        let boxed = crop.iter().any(|v| *v > 0.0) || round.is_some() || border.is_some() || vignette.is_some();
        if colours.is_empty() && !boxed && fills {
            *chain += &format!(",scale={w}:{h}:flags=bicubic,setsar=1,format=yuv420p");
            return false;
        }
        *chain += &format!(",scale={w}:{h}:force_original_aspect_ratio=decrease:flags=bicubic,setsar=1,format=rgba");
        for c in colours {
            *chain += &format!(",{c}");
        }
        *chain += &format!(",pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0");
        if !boxed {
            return true;
        }
        let (bw, bh) = ((w - crop[0] - crop[1]).max(2.0), (h - crop[2] - crop[3]).max(2.0));
        *chain += &format!(",crop={bw}:{bh}:{}:{}", crop[0], crop[2]);
        // A mask is the same for every frame, so it is drawn once and repeated.
        let once = |kind: &str, paint: String| format!("color=c={kind}:s={bw}x{bh}:r={fps},trim=end_frame=1,{paint},loop=loop=-1:size=1:start=0");
        if let Some(v) = vignette {
            // `radial-gradient(ellipse at center, transparent 45%, #000 110%)` over the crop window.
            let (shade, under) = (g.label("m"), g.label("m"));
            let d = "hypot((X-W/2)/(W*0.70711),(Y-H/2)/(H*0.70711))";
            g.chains.push(format!("{}[{shade}]", once("black", format!("format=rgba,geq=r=0:g=0:b=0:a='255*{}*clip(({d}-0.45)/0.65,0,1)'", num(v / 100.0)))));
            *chain += &format!("[{under}];[{under}][{shade}]overlay=format=auto:shortest=1");
        }
        if let Some(b) = border {
            *chain += &format!(",drawbox=x=0:y=0:w=iw:h=ih:color=white:t={}", ((0.2 + b * 0.02) * h / 100.0).round().max(1.0));
        }
        if let Some(v) = round {
            let (mask, a, b) = (g.label("m"), g.label("m"), g.label("m"));
            let radius = num((v * 0.25 * h / 100.0).min(bw / 2.0).min(bh / 2.0));
            let inside = format!("clip({radius}+0.5-hypot(max(max({radius}-X,X-(W-1-{radius})),0),max(max({radius}-Y,Y-(H-1-{radius})),0)),0,1)");
            g.chains.push(format!("{}[{mask}]", once("white", format!("format=gray,geq=lum='255*{inside}'"))));
            *chain += &format!(",split[{a}][{b}];[{b}]alphaextract[{b}x];[{b}x][{mask}]blend=all_mode=multiply:shortest=1[{b}y];[{a}][{b}y]alphamerge");
        }
        *chain += &format!(",pad={w}:{h}:{}:{}:color=black@0", crop[0], crop[2]);
        true
    }

    /// Filters that move a stage-sized layer: its own transform, shake, and a transition's motion,
    /// folded into one perspective mapping of the layer's four corners.
    fn moved(&self, piece: &Piece, r: &Props, m: &Motion, edge: Option<&Edge>) -> Vec<String> {
        let (w, h) = (self.w, self.h);
        let shake = r.fx("Shake").unwrap_or(0.0) / 100.0;
        let moving = shake > 0.0 || m.tx.is_some() || m.ty.is_some() || m.scale.is_some() || m.rotate.is_some();
        let stays = (0..4).all(|i| {
            let (x, y) = (if i % 2 == 0 { 0.0 } else { w }, if i < 2 { 0.0 } else { h });
            let (px, py) = self.place(r, x, y);
            (px - x).abs() < 0.01 && (py - y).abs() < 0.01
        });
        if stays && !moving {
            return vec![];
        }
        // Shrinking is done by the scaler, which filters properly; the perspective step then only moves.
        let tilted = r.n("pitch") != 0.0 || r.n("yaw") != 0.0;
        let zoom = r.n("zoom_x").abs().min(r.n("zoom_y").abs()) * (1.0 + r.fx("Zoom punch").unwrap_or(0.0) / 100.0 * 0.4);
        let pre = if !tilted && zoom < 0.95 { zoom.max(0.02) } else { 1.0 };
        let (iw, ih) = (w + 2.0 * MARGIN, h + 2.0 * MARGIN);
        let mut f = vec![];
        if pre < 1.0 {
            f.push(format!("scale={}:{}:flags=bicubic", (w * pre).round().max(2.0), (h * pre).round().max(2.0)));
        }
        f.push(format!("pad={iw}:{ih}:{MARGIN}:{MARGIN}:color=black@0"));

        let frame_at = |n: &str| format!("({}+{n}*{})", num(self.timeline(piece.from)), num(self.step));
        let prelude = match edge {
            Some(e) if moving => self.prelude(piece, e, "on"),
            _ if shake > 0.0 => format!("st(2,floor({}));", frame_at("on")),
            _ => String::new(),
        };
        let mut corners = vec![];
        for i in 0..4 {
            let (ix, iy) = (if i % 2 == 0 { 0.0 } else { iw }, if i < 2 { 0.0 } else { ih });
            let (qx, qy) = self.place(r, (ix - MARGIN) / pre, (iy - MARGIN) / pre);
            if !moving {
                corners.push(format!("x{i}={}:y{i}={}", num(qx + MARGIN), num(qy + MARGIN)));
                continue;
            }
            // Shake rides on the layer; a transition then moves the whole layer around the stage centre.
            let (mut x, mut y) = (num(qx - w / 2.0), num(qy - h / 2.0));
            if shake > 0.0 {
                x = format!("({x}+sin(ld(2)*1.7)*{})", num(shake * 0.02 * w));
                y = format!("({y}+cos(ld(2)*2.3)*{})", num(shake * 0.012 * h));
            }
            let (mut ex, mut ey) = (x.clone(), y.clone());
            if let Some(a) = &m.rotate {
                ex = format!("({x}*cos({a})-{y}*sin({a}))");
                ey = format!("({x}*sin({a})+{y}*cos({a}))");
            }
            if let Some(s) = &m.scale {
                ex = format!("{s}*{ex}");
                ey = format!("{s}*{ey}");
            }
            let tx = m.tx.as_deref().unwrap_or("0");
            let ty = m.ty.as_deref().unwrap_or("0");
            corners.push(format!("x{i}='{prelude}{}+({ex})+({tx})':y{i}='{prelude}{}+({ey})+({ty})'", num(w / 2.0 + MARGIN), num(h / 2.0 + MARGIN)));
        }
        let eval = if moving { ":eval=frame" } else { "" };
        f.push(format!("perspective={}:sense=destination:interpolation=cubic{eval}", corners.join(":")));
        f.push(format!("crop={w}:{h}:{MARGIN}:{MARGIN}"));
        f
    }

    /// For a layer that is only scaled and moved and still fills the stage (a zoom punch, a reframing):
    /// the part of the picture that ends up on screen, cut out and scaled, with no see-through step.
    fn reframed(&self, r: &Props) -> Option<String> {
        let (w, h) = (self.w, self.h);
        if r.n("rotation") != 0.0 || r.n("pitch") != 0.0 || r.n("yaw") != 0.0 || r.fx("Shake").is_some() {
            return None;
        }
        let ((x0, y0), (x1, y1)) = (self.place(r, 0.0, 0.0), self.place(r, w, h));
        if x0.min(x1) > 0.5 || x0.max(x1) < w - 0.5 || y0.min(y1) > 0.5 || y0.max(y1) < h - 0.5 {
            return None;
        }
        // The stage's corners, in the picture's own coordinates.
        let (sx, sy) = (w / (x1 - x0), h / (y1 - y0));
        let (ax, bx) = (-x0 * sx, (w - x0) * sx);
        let (ay, by) = (-y0 * sy, (h - y0) * sy);
        // Whole, even numbers: the picture is still in a format that stores colour at half size.
        let even = |v: f64| ((v / 2.0).round() * 2.0).max(0.0);
        let mut f = format!("crop={}:{}:{}:{}", even((bx - ax).abs()).clamp(2.0, w), even((by - ay).abs()).clamp(2.0, h), even(ax.min(bx)), even(ay.min(by)));
        if x1 < x0 {
            f += ",hflip";
        }
        if y1 < y0 {
            f += ",vflip";
        }
        Some(format!("{f},scale={w}:{h}:flags=bicubic,setsar=1"))
    }

    /// Multiplies a layer's opacity by `expr`, which may change every frame. The mask is drawn small
    /// and stretched: evaluating an expression for every pixel of every frame is very slow.
    fn fade(&self, g: &mut Graph, chain: &mut String, pre: &str, expr: &str, frames: i64) {
        let (w, h, fps) = (self.w, self.h, rate(self.fps));
        let spatial = expr.contains('X') || expr.contains('Y');
        let (mw, mh, stretch) = if spatial { ((w / 8.0).round() * 2.0, (h / 8.0).round() * 2.0, "bilinear") } else { (16.0, 16.0, "neighbor") };
        let (mask, a, b) = (g.label("k"), g.label("k"), g.label("k"));
        g.chains.push(format!("color=c=white:s={mw}x{mh}:r={fps},format=gray,trim=end_frame={frames},geq=lum='{pre}255*clip({expr},0,1)',scale={w}:{h}:flags={stretch}[{mask}]"));
        *chain += &format!(",split[{a}][{b}];[{b}]alphaextract[{b}x];[{b}x][{mask}]blend=all_mode=multiply[{b}y];[{a}][{b}y]alphamerge");
    }

    /// Adds one picture layer to the chunk and draws it over what is there. With `base`, a plain
    /// full-frame piece is kept aside to become part of the picture under everything.
    fn layer(&mut self, g: &mut Graph, piece: &Piece, chunk_from: i64, base: bool) {
        let lib = self.lib.clone();
        let r = Props { c: piece.c, lib: &lib };
        let (w, h, fps) = (self.w, self.h, rate(self.fps));
        let frames = piece.to - piece.from;
        let speed = piece.c.speed();
        let clip_frame = if piece.leaving.is_some() { (piece.c.end() - 1) as f64 } else { self.timeline(piece.from) };
        let source = (piece.c.source_in as f64 + (clip_frame - piece.c.start as f64) * speed).max(0.0) / self.project_fps;
        let still = piece.m.kind == MediaKind::Image || piece.leaving.is_some();

        let input = match piece.m.kind {
            MediaKind::Image => g.input(vec!["-i".into(), piece.m.path.clone()]),
            _ if still => g.input(vec!["-ss".into(), num(source), "-t".into(), "0.5".into(), "-i".into(), piece.m.path.clone()]),
            _ => {
                let seconds = frames as f64 * self.step * speed / self.project_fps + 0.5;
                g.input(vec!["-ss".into(), num(source), "-t".into(), num(seconds), "-i".into(), piece.m.path.clone()])
            }
        };
        let mut chain = match still {
            true => format!("[{input}:v]trim=end_frame=1"),
            false => format!("[{input}:v]setpts=(PTS-STARTPTS)/{},fps={fps}", num(speed)),
        };
        let mut see_through = self.still_look(g, piece, &r, &mut chain);
        if still {
            // One picture is filtered once and then repeated.
            chain += &format!(",loop=loop=-1:size=1:start=0,setpts=N/({fps})/TB");
        }
        let mut f: Vec<String> = vec![];

        let edge = piece.edge.or(piece.leaving);
        let mut m = edge.map(|e| motion(&e, w, h)).unwrap_or_default();
        if let Some(e) = piece.leaving {
            // Under an incoming clip only a push moves the old picture; everything else leaves it be.
            m = Motion::default();
            match e.name {
                "Push left" => m.tx = Some(format!("(0-ld(1)*{})", num(w))),
                "Push right" => m.tx = Some(format!("ld(1)*{}", num(w))),
                _ => {}
            }
        }
        let steady = m.tx.is_none() && m.ty.is_none() && m.scale.is_none() && m.rotate.is_none();
        let reframed = self.reframed(&r).filter(|_| steady && !see_through);
        let moved = if reframed.is_some() { vec![] } else { self.moved(piece, &r, &m, edge.as_ref()) };
        if let Some(cut) = reframed.filter(|_| !self.moved(piece, &r, &m, edge.as_ref()).is_empty()) {
            f.push(cut);
        }
        if !moved.is_empty() {
            if !see_through {
                f.push("format=rgba".into());
            }
            see_through = true;
            f.extend(moved);
        }
        let dynamic = m.alpha.is_some() || m.blur.is_some() || m.glitch;
        let shadow = r.fx("Drop shadow");
        let opacity = r.n("opacity") / 100.0;
        let blend = blend_mode(r.blend());
        if !["Normal", ""].contains(&r.blend()) && blend.is_none() {
            self.warn(format!("Blend mode {} is drawn as Normal in the export.", r.blend()));
        }
        let plain = !see_through && !dynamic && shadow.is_none() && opacity >= 1.0 && blend.is_none() && m.flat.is_none();
        if !plain && !see_through {
            f.push("format=rgba".into());
        }

        for filter in f {
            chain += &format!(",{filter}");
        }
        if let Some(amount) = shadow {
            // `drop-shadow(0 1cqh blur rgba(0,0,0,a))`: the layer's own shape, blurred, a little lower.
            let (a, b, s) = (g.label("s"), g.label("s"), g.label("s"));
            let sigma = (0.5 + amount * 0.04) * h / 100.0 / 2.0;
            let drop = (h / 100.0).round();
            chain += &format!(",split[{a}][{b}];[{a}]colorchannelmixer=rr=0:gg=0:bb=0:aa={},gblur=sigma={},pad={w}:{}:0:{drop}:color=black@0,crop={w}:{h}:0:0[{s}];[{s}][{b}]overlay=format=auto", num(0.3 + amount / 150.0), num(sigma), h + drop);
        }
        if let Some(e) = edge.filter(|_| dynamic) {
            let pre = self.prelude(piece, &e, "N");
            if let Some(sigma) = m.blur {
                // A blur that clears: the sharp picture fades in over a blurred copy of itself.
                let (a, b, c) = (g.label("s"), g.label("s"), g.label("s"));
                chain += &format!(",split[{a}][{b}];[{a}]gblur=sigma={}[{c}];[{b}]null", num(sigma));
                self.fade(g, &mut chain, &pre, "ld(1)", frames);
                chain += &format!("[{b}x];[{c}][{b}x]overlay=format=auto");
            }
            if let Some(alpha) = &m.alpha {
                self.fade(g, &mut chain, &pre, alpha, frames);
            }
            if m.glitch {
                let pre = self.prelude(piece, &e, "n");
                chain += &format!(",hue=h='{pre}{}*180*(1-ld(0))':s='{pre}1+2*(1-ld(0))'", noise(3));
            }
        }
        if opacity < 1.0 {
            chain += &format!(",colorchannelmixer=aa={}", num(opacity.max(0.0)));
        }

        let (n0, n1) = (piece.from - chunk_from, piece.to - chunk_from);
        let offset = format!("{n0}/({fps})");
        let shown = format!("enable='between(n,{n0},{})'", n1 - 1);
        let (out, top) = (g.label("v"), g.label("l"));
        if base && plain {
            chain += &format!(",tpad=stop_mode=clone:stop_duration=1,trim=end_frame={frames}[{top}]");
            g.chains.push(chain);
            g.base.push((n0, n1, top));
            return;
        }
        match blend {
            None => {
                // The last picture is held in case the file runs out a frame early.
                chain += &format!(",tpad=stop_mode=clone:stop_duration=1,trim=end_frame={frames},setpts=PTS+{offset}/TB[{top}]");
                g.chains.push(chain);
                g.chains.push(format!("[{}][{top}]overlay=0:0:format=auto:eof_action=pass:{shown}[{out}]", g.video));
            }
            Some(mode) => {
                // Blending needs the layer for the whole chunk, so it is padded with nothing before and after.
                let (base, under, colour, alpha, mixed) = (g.label("b"), g.label("b"), g.label("b"), g.label("b"), g.label("b"));
                chain += &format!(",tpad=stop_mode=clone:stop_duration=1,trim=end_frame={frames},tpad=start={n0}:start_mode=add:color=black@0,split[{colour}][{alpha}]");
                g.chains.push(chain);
                g.chains.push(format!("[{}]split[{base}][{under}]", g.video));
                g.chains.push(format!("[{under}]format=gbrp[{under}x];[{colour}]format=gbrp[{colour}x];[{under}x][{colour}x]blend=all_mode={mode}:eof_action=pass[{mixed}]"));
                g.chains.push(format!("[{alpha}]alphaextract[{alpha}x];[{mixed}][{alpha}x]alphamerge[{top}]"));
                g.chains.push(format!("[{base}][{top}]overlay=0:0:format=auto:eof_action=pass:{shown}[{out}]"));
            }
        }
        g.video = out;

        if let Some((colour, alpha)) = edge.and_then(|e| Some((e, m.flat.as_ref()?))).map(|(e, (c, a))| (*c, format!("{}255*{}*({a})", self.prelude(piece, &e, "N"), num(opacity)))) {
            // A dip: a flat colour over the layer. Drawn tiny and stretched, since every pixel is the same.
            let (flat, out) = (g.label("f"), g.label("v"));
            g.chains.push(format!("color=c={colour}:s=16x16:r={fps},format=rgba,trim=end_frame={frames},geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='{alpha}',scale={w}:{h}:flags=neighbor,setpts=PTS+{offset}/TB[{flat}]"));
            g.chains.push(format!("[{}][{flat}]overlay=0:0:format=auto:eof_action=pass:{shown}[{out}]", g.video));
            g.video = out;
        }
    }

    /// The text of one track over a chunk, as a list of images for FFmpeg's concat reader.
    fn text(&self, g: &mut Graph, track: &TextTrack, chunk: (i64, i64), dir: &Path, list: PathBuf, files: &mut Vec<(PathBuf, String)>) {
        let (from, to) = (self.timeline(chunk.0), self.timeline(chunk.1));
        let spans: Vec<&TextSpan> = track.spans.iter().filter(|s| (s.to as f64) > from && (s.from as f64) < to).collect();
        if spans.is_empty() {
            return;
        }
        let mut text = String::from("ffconcat version 1.0\n");
        let mut entry = |image: &str, frames: f64| {
            if frames > 0.0 {
                text += &format!("file '{}'\nduration {}\n", path_arg(&dir.join(image)).replace('\'', "'\\''"), num(frames / self.project_fps));
            }
        };
        let mut at = from;
        for s in &spans {
            entry("blank.png", s.from as f64 - at);
            let end = (s.to as f64).min(to);
            entry(&s.image, end - (s.from as f64).max(at));
            at = end;
        }
        entry("blank.png", to - at);
        // The reader drops the length of the last entry unless the entry is named once more.
        text += &format!("file '{}'\n", path_arg(&dir.join("blank.png")).replace('\'', "'\\''"));
        let input = g.input(vec!["-f".into(), "concat".into(), "-safe".into(), "0".into(), "-i".into(), path_arg(&list)]);
        files.push((list, text));
        // Only the part of the frame the text ever touches in this chunk is laid over the video.
        let (w, h) = (self.w as i64, self.h as i64);
        let mut area = [w, h, 0, 0];
        for rect in spans.iter().map(|s| s.rect.unwrap_or([0, 0, w, h])) {
            area = [area[0].min(rect[0]), area[1].min(rect[1]), area[2].max(rect[0] + rect[2]), area[3].max(rect[1] + rect[3])];
        }
        let (x, y) = ((area[0].clamp(0, w - 2) / 2) * 2, (area[1].clamp(0, h - 2) / 2) * 2);
        let (cw, ch) = ((((area[2].min(w) - x).max(2) + 1) / 2) * 2, (((area[3].min(h) - y).max(2) + 1) / 2) * 2);
        let (top, out) = (g.label("t"), g.label("v"));
        // Converted before the frames are repeated, so each picture is converted once.
        g.chains.push(format!("[{input}:v]crop={}:{}:{x}:{y},format=yuva420p,fps={},trim=end_frame={}[{top}]", cw.min(w - x), ch.min(h - y), rate(self.fps), chunk.1 - chunk.0));
        g.chains.push(format!("[{}][{top}]overlay={x}:{y}:format=auto:eof_action=pass[{out}]", g.video));
        g.video = out;
    }

    /// The sound of one clip over a chunk, with its volume, fades and speed.
    fn sound(&mut self, g: &mut Graph, c: &Clip, m: &MediaItem, chunk: (i64, i64)) {
        let streams = (self.info)(m).audio_streams;
        if streams == 0 {
            return;
        }
        let lib = self.lib.clone();
        let r = Props { c, lib: &lib };
        let (from, to) = (self.out_frame(c.start).max(chunk.0), self.out_frame(c.end()).min(chunk.1));
        let speed = c.speed();
        let into = self.timeline(from) - c.start as f64;
        let source = (c.source_in as f64 + into * speed) / self.project_fps;
        let seconds = (to - from) as f64 * self.step * speed / self.project_fps;
        let input = g.input(vec!["-ss".into(), num(source), "-t".into(), num(seconds + 0.1), "-vn".into(), "-i".into(), m.path.clone()]);
        let sample = |k: i64| (k as f64 * SAMPLE_RATE as f64 / self.fps).round() as i64;

        let mut chain = match (self.s.audio, streams) {
            (AudioStreams::All, n) if n > 1 => {
                let all: String = (0..n).map(|i| format!("[{input}:a:{i}]")).collect();
                format!("{all}amix=inputs={n}:normalize=0,")
            }
            _ => format!("[{input}:a:0]"),
        };
        chain += "asetpts=PTS-STARTPTS";
        // `atempo` keeps the pitch, as the preview does, and takes factors from 0.5 up.
        let mut tempo = speed;
        while tempo < 0.5 {
            chain += ",atempo=0.5";
            tempo /= 0.5;
        }
        if (tempo - 1.0).abs() > 1e-6 {
            chain += &format!(",atempo={}", num(tempo));
        }
        chain += &format!(",aresample={SAMPLE_RATE},aformat=sample_fmts=fltp:channel_layouts=stereo");

        // `clipGain` in look.ts, with `t` counted in seconds from the start of this piece.
        let at = format!("({}+t*{})", num(into), num(self.project_fps));
        let mut gain = vec![num(10f64.powf(r.n("volume") / 20.0))];
        if r.n("fade_in") > 0.0 {
            gain.push(format!("min(1,{at}/{})", num(r.n("fade_in") * self.project_fps)));
        }
        if r.n("fade_out") > 0.0 {
            gain.push(format!("min(1,({}-{at})/{})", c.duration, num(r.n("fade_out") * self.project_fps)));
        }
        if let Some(t) = &c.transition_in {
            gain.push(format!("if(lt({at},{0}),min(1,({at}+0.5)/{0}),1)", t.duration));
        }
        if let Some(t) = &c.transition_out {
            gain.push(format!("if(gte({at},{}),min(1,({}-{at}-0.5)/{}),1)", c.duration - t.duration, c.duration, t.duration));
        }
        if gain.len() > 1 || gain[0] != "1" {
            chain += &format!(",volume='clip({},0,1)':eval=frame", gain.join("*"));
        }
        let label = g.label("a");
        chain += &format!(",atrim=end_sample={},adelay=delays={}S:all=1[{label}]", sample(to) - sample(from), sample(from) - sample(chunk.0));
        g.chains.push(chain);
        g.sounds.push(label);
    }

    /// Everything one chunk needs: its inputs and its filter graph.
    fn chunk(&mut self, chunk: (i64, i64), text: &[TextTrack], dir: &Path, index: usize, files: &mut Vec<(PathBuf, String)>) -> Graph {
        let p = self.p;
        let (from, to) = (self.timeline(chunk.0), self.timeline(chunk.1));
        let visible = |c: &&Clip| (c.end() as f64) > from && (c.start as f64) < to;
        let frames = chunk.1 - chunk.0;
        let mut g = Graph { video: "v0".into(), ..Default::default() };
        let lowest = p.sequence.tracks.iter().rev().find(|t| t.kind != TrackKind::Audio && !t.hidden).map(|t| t.id.clone());

        // Bottom track first, so higher tracks draw on top, as in the preview.
        for t in p.sequence.tracks.iter().rev().filter(|t| t.kind != TrackKind::Audio && !t.hidden) {
            let mut clips: Vec<&Clip> = p.sequence.clips.iter().filter(|c| c.track == t.id).filter(visible).collect();
            clips.sort_by_key(|c| c.start);
            let picture = |c: &Clip| c.kind == ClipKind::Video && c.media.as_ref().and_then(|id| p.media(id)).filter(|m| m.kind != MediaKind::Audio).is_some();
            let previous = |c: &Clip| p.sequence.clips.iter().find(|x| x.track == c.track && x.end() == c.start);

            // Old pictures held under an incoming transition go underneath everything else on the track.
            for c in &clips {
                let (Some(tr), Some(old)) = (&c.transition_in, previous(c)) else { continue };
                let window = (self.out_frame(c.start).max(chunk.0), self.out_frame(c.start + tr.duration).min(self.out_frame(c.end())).min(chunk.1));
                if window.1 > window.0 && picture(old) {
                    let edge = Edge { name: &tr.name, at: c.start, frames: tr.duration, out: false, has_previous: true };
                    let m = p.media(old.media.as_ref().unwrap()).unwrap();
                    self.layer(&mut g, &Piece { c: old, m, from: window.0, to: window.1, edge: None, leaving: Some(edge) }, chunk.0, false);
                }
            }
            if let Some(track) = text.iter().find(|x| x.track == t.id) {
                self.text(&mut g, track, chunk, dir, dir.join(format!("text-{index}-{}.txt", t.id)), files);
            }
            for c in clips.iter().filter(|c| picture(c)) {
                let m = p.media(c.media.as_ref().unwrap()).unwrap();
                // A clip is drawn in up to three parts: coming in, plain, going out.
                let into = c.transition_in.as_ref().map(|t| c.start + t.duration.min(c.duration)).unwrap_or(c.start);
                let outof = c.transition_out.as_ref().map(|t| (c.end() - t.duration).max(into)).unwrap_or(c.end());
                let coming = c.transition_in.as_ref().map(|t| Edge { name: &t.name, at: c.start, frames: t.duration, out: false, has_previous: previous(c).is_some() });
                let going = c.transition_out.as_ref().map(|t| Edge { name: &t.name, at: c.end(), frames: t.duration, out: true, has_previous: false });
                for (a, b, edge) in [(c.start, into, coming), (into, outof, None), (outof, c.end(), going)] {
                    let (a, b) = (self.out_frame(a).max(chunk.0), self.out_frame(b).min(chunk.1));
                    if b > a {
                        self.layer(&mut g, &Piece { c, m, from: a, to: b, edge, leaving: None }, chunk.0, lowest.as_deref() == Some(t.id.as_str()));
                    }
                }
            }
        }

        // Sound: clips on audio tracks, and video clips, which carry their own.
        for t in p.sequence.tracks.iter().filter(|t| !t.muted && !t.hidden && t.kind != TrackKind::Caption) {
            for c in p.sequence.clips.iter().filter(|c| c.track == t.id).filter(visible) {
                let media = c.media.as_ref().and_then(|id| p.media(id)).filter(|m| m.kind != MediaKind::Image);
                if let Some(m) = media.filter(|_| self.out_frame(c.end()).min(chunk.1) > self.out_frame(c.start).max(chunk.0)) {
                    self.sound(&mut g, c, m, chunk);
                }
            }
        }
        // The picture under everything: the lowest track's plain pieces end to end, black where it has none.
        let black = |frames: i64| format!("color=c=black:s={}x{}:r={},trim=end_frame={frames},format=yuv420p,setsar=1", self.w, self.h, rate(self.fps));
        let mut parts = std::mem::take(&mut g.base);
        parts.sort_by_key(|(from, _, _)| *from);
        let mut row = String::new();
        let (mut at, mut count) = (0, 0);
        for (from, to, label) in parts.iter().chain([&(frames, frames, String::new())]) {
            if *from > at {
                let gap = g.label("g");
                g.chains.push(format!("{}[{gap}]", black(from - at)));
                row += &format!("[{gap}]");
                count += 1;
            }
            if !label.is_empty() {
                row += &format!("[{label}]");
                count += 1;
            }
            at = *to;
        }
        g.chains.push(if count == 1 { format!("{row}null[v0]") } else { format!("{row}concat=n={count}:v=1:a=0[v0]") });
        let samples = |k: i64| (k as f64 * SAMPLE_RATE as f64 / self.fps).round() as i64;
        g.chains.push(format!("anullsrc=r={SAMPLE_RATE}:cl=stereo,atrim=end_sample={}[a0]", samples(chunk.1) - samples(chunk.0)));
        if !g.sounds.is_empty() {
            let all: String = g.sounds.iter().map(|l| format!("[{l}]")).collect();
            g.chains.push(format!("[a0]{all}amix=inputs={}:duration=first:normalize=0[amix]", g.sounds.len() + 1));
        }
        g
    }
}

/// Checks the settings against the project and returns the timeline frames to export.
pub fn check(p: &Project, s: &ExportSettings) -> Result<(Frame, Frame), String> {
    if !Path::new(&s.path).is_absolute() {
        return Err("the output file needs a full path, such as C:/Videos/episode.mp4".into());
    }
    let extension = Path::new(&s.path).extension().map(|e| e.to_string_lossy().to_lowercase());
    if !matches!(extension.as_deref(), Some("mp4" | "mov" | "mkv")) {
        return Err("the output file must end in .mp4, .mov or .mkv".into());
    }
    if s.width < 16 || s.height < 16 || s.width > 7680 || s.height > 4320 || s.width % 2 != 0 || s.height % 2 != 0 {
        return Err("width and height must be even numbers between 16 and 7680×4320".into());
    }
    if !(1.0..=240.0).contains(&s.fps) {
        return Err("the frame rate must be between 1 and 240".into());
    }
    let (pw, ph) = (p.settings.width as f64, p.settings.height as f64);
    if (s.width as f64 / s.height as f64 - pw / ph).abs() > 0.01 {
        return Err(format!("{}×{} is a different shape from the project ({pw}×{ph}); change the project size first", s.width, s.height));
    }
    if !s.encoder.is_empty() && !ENCODERS.contains(&s.encoder.as_str()) {
        return Err(format!("unknown encoder {}; available: {}", s.encoder, ENCODERS.join(", ")));
    }
    let end = p.duration();
    let (from, to) = match s.range {
        ExportRange::Whole => (0, end),
        ExportRange::InOut => (p.sequence.in_point.unwrap_or(0), p.sequence.out_point.unwrap_or(end).min(end)),
    };
    if to <= from {
        return Err("there is nothing on the timeline to export".into());
    }
    for c in p.sequence.clips.iter().filter(|c| c.end() > from && c.start < to && matches!(c.kind, ClipKind::Video | ClipKind::Audio)) {
        let m = c.media.as_ref().and_then(|id| p.media(id)).ok_or_else(|| format!("clip {} ({}) has no media", c.id, c.name))?;
        if !Path::new(&m.path).is_file() {
            return Err(format!("{} is missing: {}", m.name, m.path));
        }
    }
    Ok((from, to))
}

/// Works out every FFmpeg run of an export. `encoder` must be one the machine has; `dir` is a
/// folder for the job's working files, where the window's text images already are.
pub fn plan(p: &Project, s: &ExportSettings, encoder: &str, text: &[TextTrack], info: &dyn Fn(&MediaItem) -> MediaInfo, dir: &Path) -> Result<Plan, String> {
    let (start, end) = check(p, s)?;
    let project_fps = p.settings.fps_num as f64 / p.settings.fps_den as f64;
    let step = project_fps / s.fps;
    let total = ((end - start) as f64 / step).round() as i64;
    let mut b = Builder {
        p,
        lib: library(),
        s,
        w: s.width as f64,
        h: s.height as f64,
        kx: s.width as f64 / p.settings.width as f64,
        ky: s.height as f64 / p.settings.height as f64,
        fps: s.fps,
        project_fps,
        step,
        start,
        info,
        warnings: vec![],
    };
    if p.sequence.clips.iter().any(|c| matches!(c.kind, ClipKind::Graphic | ClipKind::Caption) && c.props.get("blend").and_then(Value::as_str).is_some_and(|m| m != "Normal")) {
        b.warn("Blend modes on text are drawn as Normal in the export.".into());
    }
    let video = encoder_args(encoder, s.quality, s.fps)?;
    let mut files = vec![];
    let mut chunks = vec![];
    let mut at = 0;
    while at < total {
        // Shorten the chunk until FFmpeg has a sensible number of files to open.
        let mut length = (CHUNK_SECONDS * s.fps).round() as i64;
        let (to, graph, extra) = loop {
            let to = (at + length).min(total);
            let mut extra = vec![];
            let graph = b.chunk((at, to), text, dir, chunks.len(), &mut extra);
            if graph.inputs.len() <= MAX_INPUTS || length <= (s.fps.round() as i64).max(2) {
                break (to, graph, extra);
            }
            length /= 2;
        };
        files.extend(extra);
        let script = dir.join(format!("graph-{}.txt", chunks.len()));
        let file = dir.join(format!("chunk-{:05}.mkv", chunks.len()));
        let sound = if graph.sounds.is_empty() { "[a0]" } else { "[amix]" };
        let mut args: Vec<String> = graph.inputs.iter().flatten().cloned().collect();
        args.extend(["-/filter_complex".into(), path_arg(&script), "-map".into(), format!("[{}]", graph.video), "-map".into(), sound.into()]);
        args.extend(video.iter().cloned());
        args.extend(["-c:a", "pcm_f32le", "-f", "matroska", "-y"].map(String::from));
        args.push(path_arg(&file));
        files.push((script, graph.chains.join(";\n")));
        chunks.push(Chunk { args, frames: to - at, file });
        at = to;
    }
    let list = dir.join("chunks.txt");
    let names: String = chunks.iter().map(|c| format!("file '{}'\n", path_arg(&c.file).replace('\'', "'\\''"))).collect();
    files.push((list.clone(), format!("ffconcat version 1.0\n{names}")));
    // The chunks are copied packet for packet. Left to itself FFmpeg converts the packets on the way
    // through, and the MP4 it then writes does not always play.
    let mut finish: Vec<String> = ["-f", "concat", "-safe", "0", "-auto_convert", "0", "-i"].map(String::from).into();
    finish.push(path_arg(&list));
    // The picture is copied as it is; the mixed sound is kept from clipping and encoded once.
    finish.extend(["-c:v", "copy", "-af", "alimiter=level=0:limit=0.97", "-c:a", "aac", "-b:a", "256k", "-ar"].map(String::from));
    finish.push(SAMPLE_RATE.to_string());
    if !s.path.to_lowercase().ends_with(".mkv") {
        finish.extend(["-movflags".into(), "+faststart".into()]);
    }
    finish.extend(["-y".into(), s.path.clone()]);
    Ok(Plan { files, chunks, finish, total_frames: total, warnings: b.warnings })
}

/// Changes the frame rate a project counts in, keeping everything at the same moment in time.
pub(crate) fn rescale_frames(p: &mut Project, fps_num: u32, fps_den: u32) {
    let ratio = (fps_num as f64 / fps_den as f64) / (p.settings.fps_num as f64 / p.settings.fps_den as f64);
    let scale = |f: Frame| (f as f64 * ratio).round() as Frame;
    for c in &mut p.sequence.clips {
        // Start and end are rounded on their own, so clips that touched still touch.
        let (start, end) = (scale(c.start), scale(c.end()));
        c.start = start;
        c.duration = (end - start).max(1);
        c.source_in = scale(c.source_in);
        for t in [&mut c.transition_in, &mut c.transition_out].into_iter().flatten() {
            t.duration = scale(t.duration).clamp(1, c.duration);
        }
    }
    for m in &mut p.media {
        m.duration = scale(m.duration);
    }
    for m in &mut p.sequence.markers {
        m.at = scale(m.at);
    }
    p.sequence.in_point = p.sequence.in_point.map(scale);
    p.sequence.out_point = p.sequence.out_point.map(scale);
    p.settings.fps_num = fps_num;
    p.settings.fps_den = fps_den;
}
