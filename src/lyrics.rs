//! Lirik sinkron dari LRCLIB (https://lrclib.net): fetch di thread terpisah, parser LRC, pencarian baris aktif.

use super::*;
use serde::Deserialize;

const UA: &str = concat!("HesewakeNotch/", env!("CARGO_PKG_VERSION"), " (Windows; based on Poch)");
/// Baris ditampilkan sedikit lebih awal supaya terasa tepat dengan suara.
const LEAD_MS: u32 = 150;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LyricLine {
    pub(crate) at_ms: u32,
    pub(crate) text: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) enum Lyrics {
    #[default]
    None,
    Loading,
    Synced(Vec<LyricLine>),
    /// Hanya ada lirik tanpa timestamp: tidak ditampilkan per baris.
    Plain,
    Unavailable,
}

pub(crate) type LyricsSlot = Arc<Mutex<Option<(String, Lyrics)>>>;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Entry {
    duration: Option<f64>,
    instrumental: Option<bool>,
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

fn pct(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}

fn get_string(url: &str) -> Option<String> {
    let client = HttpClient::new().ok()?;
    if let Ok(h) = client.DefaultRequestHeaders() {
        if let Ok(ua) = h.UserAgent() {
            let _ = ua.TryParseAdd(&HSTRING::from(UA));
        }
    }
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let body = client.GetStringAsync(&uri).ok()?.get().ok()?;
    let s = body.to_string();
    // Batasi ukuran respons (lirik normal < 100 KB).
    if s.is_empty() || s.len() > 2_000_000 {
        None
    } else {
        Some(s)
    }
}

/// Cari lirik untuk satu lagu. Blocking: panggil dari thread terpisah.
pub(crate) fn fetch(title: &str, artist: &str, dur_secs: u32) -> Lyrics {
    let base = format!("track_name={}&artist_name={}", pct(title), pct(artist));
    // 1) Pencocokan tepat (pakai durasi bila ada); 404 -> get_string mengembalikan None.
    let exact = if dur_secs > 0 {
        format!("https://lrclib.net/api/get?{}&duration={}", base, dur_secs)
    } else {
        format!("https://lrclib.net/api/get?{}", base)
    };
    if let Some(body) = get_string(&exact) {
        if let Ok(e) = serde_json::from_str::<Entry>(&body) {
            if let Some(l) = from_entry(&e) {
                return l;
            }
        }
    }
    // 2) Pencarian: ambil yang punya lirik sinkron dengan durasi paling mendekati.
    let Some(body) = get_string(&format!("https://lrclib.net/api/search?{}", base)) else {
        return Lyrics::Unavailable;
    };
    let entries: Vec<Entry> = serde_json::from_str(&body).unwrap_or_default();
    let dist = |e: &Entry| e.duration.map_or(f64::MAX, |d| (d - dur_secs as f64).abs());
    let best = entries
        .iter()
        .filter(|e| from_entry(e).is_some())
        .min_by(|a, b| {
            let (sa, sb) = (a.synced_lyrics.as_deref().map_or(true, str::is_empty), b.synced_lyrics.as_deref().map_or(true, str::is_empty));
            sa.cmp(&sb).then(dist(a).total_cmp(&dist(b)))
        });
    best.and_then(from_entry).unwrap_or(Lyrics::Unavailable)
}

fn from_entry(e: &Entry) -> Option<Lyrics> {
    if let Some(s) = e.synced_lyrics.as_deref().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Some(Lyrics::Synced(lines));
        }
    }
    if e.instrumental == Some(true) {
        return Some(Lyrics::Unavailable);
    }
    if e.plain_lyrics.as_deref().map_or(false, |s| !s.trim().is_empty()) {
        return Some(Lyrics::Plain);
    }
    None
}

/// `[mm:ss.xx]teks`, termasuk beberapa timestamp per baris. Tag metadata (`[ar:...]`) diabaikan.
pub(crate) fn parse_lrc(body: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let mut rest = raw.trim_start();
        let mut stamps: Vec<u32> = Vec::new();
        while let Some(r) = rest.strip_prefix('[') {
            let Some((ts, after)) = r.split_once(']') else { break };
            match parse_ts(ts) {
                Some(ms) => stamps.push(ms),
                None => break,
            }
            rest = after;
        }
        let text = rest.trim();
        for ms in stamps {
            out.push(LyricLine { at_ms: ms, text: text.to_string() });
        }
    }
    out.sort_by_key(|l| l.at_ms);
    out
}

fn parse_ts(ts: &str) -> Option<u32> {
    let (m, rest) = ts.split_once(':')?;
    let (s, frac) = rest.split_once(['.', ':']).unwrap_or((rest, "0"));
    let m: u32 = m.trim().parse().ok()?;
    let s: u32 = s.trim().parse().ok()?;
    if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // ".5" = 500 ms, ".50" = 500 ms, ".500" = 500 ms
    let ms: u32 = match frac.len() {
        1 => frac.parse::<u32>().ok()? * 100,
        2 => frac.parse::<u32>().ok()? * 10,
        _ => frac[..3].parse().ok()?,
    };
    Some((m * 60 + s) * 1000 + ms)
}

/// Indeks baris aktif pada posisi `pos_ms`, atau None sebelum baris pertama.
pub(crate) fn current_line(lines: &[LyricLine], pos_ms: u32) -> Option<usize> {
    let p = pos_ms.saturating_add(LEAD_MS);
    lines.partition_point(|l| l.at_ms <= p).checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_and_multi_stamps() {
        let l = parse_lrc("[ar:Someone]\n[00:12.34]Hello\n[00:10.00][00:50.5]Again\n[01:00.123]\n");
        assert_eq!(l.len(), 4);
        assert_eq!(l[0], LyricLine { at_ms: 10_000, text: "Again".into() });
        assert_eq!(l[1].at_ms, 12_340);
        assert_eq!(l[2].at_ms, 50_500);
        assert_eq!(l[3], LyricLine { at_ms: 60_123, text: String::new() });
    }

    #[test]
    fn current_line_edges() {
        let l = parse_lrc("[00:10.00]a\n[00:20.00]b\n");
        assert_eq!(current_line(&l, 0), None);
        assert_eq!(current_line(&l, 10_000 - LEAD_MS), Some(0));
        assert_eq!(current_line(&l, 19_000), Some(0));
        assert_eq!(current_line(&l, 20_000 - LEAD_MS), Some(1));
        assert_eq!(current_line(&l, 999_999), Some(1));
    }

    #[test]
    fn rejects_garbage_stamps() {
        assert!(parse_lrc("[xx:yy]nope\nplain text").is_empty());
    }
}

// ---------------------------------------------------------------- state & helper di App
const FADE_SECS: f32 = 0.2;

impl App {
    /// Posisi lagu sekarang (ms): sampel terakhir dari media thread + waktu berjalan sejak sampel itu.
    pub(crate) fn lyric_pos_ms(&self) -> u32 {
        let extra = if self.media.playing { self.pos_at.elapsed().as_millis().min(5_000) as u32 } else { 0 };
        let p = self.media.pos_ms.saturating_add(extra);
        if self.media.dur > 0 {
            p.min(self.media.dur.saturating_mul(1000))
        } else {
            p
        }
    }

    /// Hitung ulang baris aktif. True bila berubah (perlu redraw).
    pub(crate) fn lyric_update(&mut self) -> bool {
        let idx = match &self.lyrics {
            Lyrics::Synced(lines) => current_line(lines, self.lyric_pos_ms()),
            _ => None,
        };
        if idx == self.lyric_idx {
            return false;
        }
        self.lyric_idx = idx;
        self.lyric_t = Instant::now();
        // Lebar teks baris baru (tebal) menentukan lebar notch kecil.
        let text = self.lyric_text().map(str::to_string);
        self.lyric_w = match (text, self.gfx.as_mut()) {
            (Some(t), Some(g)) => g.width_ex(&t, LYRIC_SIZE, true),
            _ => 0.0,
        };
        true
    }

    /// Lirik sinkron tersedia untuk lagu yang sedang main (menentukan lebar notch kecil).
    pub(crate) fn lyric_synced(&self) -> bool {
        self.media.playing && matches!(self.lyrics, Lyrics::Synced(_))
    }

    /// Teks baris aktif (None sebelum baris pertama atau saat jeda instrumental/baris kosong).
    pub(crate) fn lyric_text(&self) -> Option<&str> {
        match (&self.lyrics, self.lyric_idx) {
            (Lyrics::Synced(lines), Some(i)) => lines.get(i).map(|l| l.text.as_str()).filter(|t| !t.is_empty()),
            _ => None,
        }
    }

    /// 0..1: fade-in baris yang baru ganti.
    pub(crate) fn lyric_alpha(&self) -> f32 {
        (self.lyric_t.elapsed().as_secs_f32() / FADE_SECS).clamp(0.0, 1.0)
    }

    pub(crate) fn lyric_fading(&self) -> bool {
        self.lyric_synced() && self.lyric_text().is_some() && self.lyric_t.elapsed().as_secs_f32() < FADE_SECS
    }
}
