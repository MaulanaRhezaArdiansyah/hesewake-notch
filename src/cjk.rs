//! Glyph di luar font bawaan (Jepang / Mandarin / Korea, simbol, tanda baca khusus) lewat GDI.
//!
//! Font bawaan (Instrument Sans) tidak punya kana, kanji, hangul, dan banyak simbol. Memuat font Windows ke `fontdue`
//! ternyata mahal: semua outline di-parse sekaligus (Yu Gothic +420 MB, Segoe UI +38 MB dan macet ±160 ms saat notifikasi
//! pertama yang berisi emoji/tanda kutip melengkung muncul). Karena itu karakter yang kurang dirender satu per satu oleh
//! GDI dari font sistem (di-map dari berkas, tanpa salinan besar) dan hasilnya masuk ke cache glyph yang sama.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// Urutan font untuk kana/kanji/hanzi; font pertama yang punya glyph-nya dipakai.
const FACES_CJK: [&str; 5] = ["Yu Gothic UI", "Meiryo UI", "MS Gothic", "Microsoft YaHei UI", "Malgun Gothic"];
const FACES_HANGUL: [&str; 2] = ["Malgun Gothic", "Yu Gothic UI"];
/// Simbol, tanda baca khusus, dan huruf non-Latin lain.
const FACES_SYMBOL: [&str; 4] = ["Segoe UI", "Segoe UI Symbol", "Segoe UI Emoji", "Arial Unicode MS"];

struct Ctx {
    dc: HDC,
    fonts: HashMap<(u32, bool, &'static str), HFONT>,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

pub(crate) fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF)
}

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x2FFF | 0x3000..=0x30FF | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// Karakter yang dilayani jalur GDI ini: semua karakter BMP yang bukan kontrol (karakter di luar BMP, mis. emoji
/// berwarna, tetap jatuh ke kotak "tidak ada").
pub(crate) fn wants(c: char) -> bool {
    (c as u32) <= 0xFFFF && !c.is_control()
}

fn fixed(v: i16) -> FIXED {
    FIXED { fract: 0, value: v }
}

/// Rasterisasi satu karakter (BMP) menjadi `(Metrics, coverage 0..255)` dengan konvensi yang sama seperti `fontdue`.
pub(crate) fn rasterize(ch: char, size: f32, bold: bool) -> Option<(fontdue::Metrics, Vec<u8>)> {
    if !wants(ch) {
        return None;
    }
    CTX.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.is_invalid() {
                return None;
            }
            *c = Some(Ctx { dc, fonts: HashMap::new() });
        }
        let ctx = c.as_mut()?;
        let faces: &[&'static str] = if is_hangul(ch) {
            &FACES_HANGUL
        } else if is_cjk(ch) {
            &FACES_CJK
        } else {
            &FACES_SYMBOL
        };
        let px = size.round().max(6.0) as u32;
        for &name in faces {
            let font = *ctx.fonts.entry((px, bold, name)).or_insert_with(|| unsafe {
                let wname: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                CreateFontW(-(px as i32), 0, 0, 0, if bold { 700 } else { 400 }, 0, 0, 0, 1, 0, 0, 4, 0, PCWSTR(wname.as_ptr()))
            });
            if font.is_invalid() {
                continue;
            }
            unsafe {
                let old = SelectObject(ctx.dc, font);
                let r = glyph(ctx.dc, ch);
                SelectObject(ctx.dc, old);
                if r.is_some() {
                    return r;
                }
            }
        }
        None
    })
}

/// Glyph dari font yang sedang terpilih di `dc`; None bila font itu tidak punya karakternya.
unsafe fn glyph(dc: HDC, ch: char) -> Option<(fontdue::Metrics, Vec<u8>)> {
    let u = [ch as u16];
    let mut idx = [0u16; 1];
    if GetGlyphIndicesW(dc, PCWSTR(u.as_ptr()), 1, idx.as_mut_ptr(), GGI_MARK_NONEXISTING_GLYPHS) == u32::MAX || idx[0] == 0xFFFF {
        return None;
    }
    let mat = MAT2 { eM11: fixed(1), eM12: fixed(0), eM21: fixed(0), eM22: fixed(1) };
    let mut gm = GLYPHMETRICS::default();
    let n = GetGlyphOutlineW(dc, ch as u32, GGO_GRAY8_BITMAP, &mut gm, 0, None, &mat);
    if n == u32::MAX {
        return None;
    }
    let mut m = fontdue::Metrics::default();
    m.advance_width = gm.gmCellIncX as f32;
    let (w, h) = (gm.gmBlackBoxX as usize, gm.gmBlackBoxY as usize);
    if n == 0 || w == 0 || h == 0 {
        return Some((m, Vec::new()));
    }
    let mut buf = vec![0u8; n as usize];
    if GetGlyphOutlineW(dc, ch as u32, GGO_GRAY8_BITMAP, &mut gm, n, Some(buf.as_mut_ptr() as *mut c_void), &mat) == u32::MAX {
        return None;
    }
    let pitch = (w + 3) & !3;
    if pitch * h > buf.len() {
        return None;
    }
    // GGO_GRAY8: 65 tingkat (0..=64) -> 0..=255, baris dipadatkan ke kelipatan DWORD.
    let mut bm = Vec::with_capacity(w * h);
    for row in 0..h {
        for col in 0..w {
            bm.push(((buf[row * pitch + col].min(64) as u32 * 255 + 32) / 64) as u8);
        }
    }
    m.xmin = gm.gmptGlyphOrigin.x;
    m.ymin = gm.gmptGlyphOrigin.y - h as i32;
    m.width = w;
    m.height = h;
    Some((m, bm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_ranges() {
        assert!(wants('日') && wants('の') && wants('ト') && wants('한') && wants('，') && wants('“') && wants('✓'));
        // di luar BMP dan karakter kontrol tidak dilayani
        assert!(!wants('😀') && !wants('\n'));
        assert!(is_hangul('가') && !is_hangul('日'));
    }

    #[test]
    fn rasterizes_japanese_glyphs() {
        // memakai font sistem; Windows tanpa font Jepang melewati tes ini
        let Some((m, bm)) = rasterize('日', 16.0, false) else { return };
        assert!(m.width > 6 && m.height > 6, "{} x {}", m.width, m.height);
        assert_eq!(bm.len(), m.width * m.height);
        assert!(bm.iter().any(|&v| v > 200), "glyph harus punya piksel tebal");
        assert!(m.advance_width >= 14.0, "advance {}", m.advance_width);
        // spasi ideografis: tanpa bitmap tapi punya lebar
        if let Some((m, bm)) = rasterize('\u{3000}', 16.0, false) {
            assert!(bm.is_empty() && m.advance_width > 8.0);
        }
    }

    #[test]
    fn symbols_render_without_loading_big_fonts() {
        // tanda kutip melengkung, em dash, centang, dan panah: dulu memicu pemuatan Segoe UI (+38 MB, macet 160 ms)
        for ch in ['\u{201c}', '\u{2014}', '\u{2713}', '\u{2192}', '\u{20ac}'] {
            let Some((m, _)) = rasterize(ch, 14.0, false) else { continue };
            assert!(m.advance_width > 2.0, "{ch:?} advance {}", m.advance_width);
        }
    }
}
