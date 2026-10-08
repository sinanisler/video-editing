//! Reads length and picture size from MP4-family files (MP4, MOV, M4V) without decoding them.
//! Other formats need FFmpeg, which arrives with the media milestone; the app window measures
//! those through the webview instead.

use crate::model::MediaKind;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Probe {
    pub kind: MediaKind,
    pub seconds: f64,
    pub width: u32,
    pub height: u32,
    /// True when the file has a sound track (an `soun` handler in some track).
    pub has_audio: bool,
}

const MAX_MOOV: u64 = 256 * 1024 * 1024;

fn be32(b: &[u8]) -> u64 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64
}

fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// The child boxes directly inside `data`, as (type, payload).
fn boxes(mut data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = vec![];
    while data.len() >= 8 {
        let (mut size, mut header) = (be32(data) as usize, 8);
        if size == 1 && data.len() >= 16 {
            size = be64(&data[8..]) as usize;
            header = 16;
        } else if size == 0 {
            size = data.len();
        }
        if size < header || size > data.len() {
            break;
        }
        out.push((&data[4..8], &data[header..size]));
        data = &data[size..];
    }
    out
}

fn find<'a>(data: &'a [u8], kind: &[u8]) -> Option<&'a [u8]> {
    boxes(data).into_iter().find(|(k, _)| *k == kind).map(|(_, payload)| payload)
}

/// The payload of the top-level `moov` box, which may sit at either end of the file.
fn read_moov(file: &mut File) -> Result<Vec<u8>, String> {
    let err = |e: std::io::Error| e.to_string();
    let len = file.metadata().map_err(err)?.len();
    let mut pos = 0u64;
    while pos + 8 <= len {
        file.seek(SeekFrom::Start(pos)).map_err(err)?;
        let mut head = [0u8; 16];
        file.read_exact(&mut head[..8]).map_err(err)?;
        let (mut size, mut header) = (be32(&head), 8u64);
        if size == 1 {
            file.read_exact(&mut head[8..]).map_err(err)?;
            size = be64(&head[8..]);
            header = 16;
        } else if size == 0 {
            size = len - pos;
        }
        if size < header || pos + size > len {
            break;
        }
        if &head[4..8] == b"moov" {
            if size > MAX_MOOV {
                return Err("its index is unreasonably large".into());
            }
            let mut payload = vec![0u8; (size - header) as usize];
            file.read_exact(&mut payload).map_err(err)?;
            return Ok(payload);
        }
        pos += size;
    }
    Err("it is not an MP4-family file".into())
}

pub fn probe_mp4(path: &Path) -> Result<Probe, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let moov = read_moov(&mut file)?;
    let mvhd = find(&moov, b"mvhd").filter(|b| b.len() >= 32).ok_or("it has no movie header")?;
    let (timescale, duration) = if mvhd[0] == 1 {
        (be32(&mvhd[20..]), be64(&mvhd[24..]))
    } else {
        (be32(&mvhd[12..]), be32(&mvhd[16..]))
    };
    if timescale == 0 || duration == 0 {
        return Err("it reports no length".into());
    }
    // A track with a picture size is a video track; the size is 16.16 fixed point at the end of tkhd.
    let (width, height) = boxes(&moov)
        .into_iter()
        .filter(|(kind, _)| *kind == b"trak")
        .filter_map(|(_, trak)| find(trak, b"tkhd"))
        .filter(|tkhd| tkhd.len() >= 8)
        .map(|tkhd| {
            let tail = &tkhd[tkhd.len() - 8..];
            ((be32(tail) >> 16) as u32, (be32(&tail[4..]) >> 16) as u32)
        })
        .find(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((0, 0));
    let has_audio = boxes(&moov)
        .into_iter()
        .filter(|(kind, _)| *kind == b"trak")
        .filter_map(|(_, trak)| find(trak, b"mdia"))
        .filter_map(|mdia| find(mdia, b"hdlr"))
        // A handler box is version and flags (4 bytes), a pre-defined field (4 bytes), then the handler type.
        .any(|hdlr| hdlr.len() >= 12 && &hdlr[8..12] == b"soun");
    Ok(Probe {
        kind: if width > 0 { MediaKind::Video } else { MediaKind::Audio },
        seconds: duration as f64 / timescale as f64,
        width,
        height,
        has_audio,
    })
}
