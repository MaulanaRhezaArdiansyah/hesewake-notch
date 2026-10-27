//! Glyph di luar font bawaan (Jepang / Mandarin / Korea, simbol, tanda baca khusus) lewat GDI.
//!
//! Font bawaan (Instrument Sans) tidak punya kana, kanji, hangul, dan banyak simbol. Memuat font Windows ke `fontdue`
//! ternyata mahal: semua outline di-parse sekaligus (Yu Gothic +420 MB, Segoe UI +38 MB dan macet ±160 ms saat notifikasi
//! pertama yang berisi emoji/tanda kutip melengkung muncul). Karena itu karakter yang kurang dirender satu per satu oleh
//! GDI dari font sistem (di-map dari berkas, tanpa salinan besar) dan hasilnya masuk ke cache glyph yang sama.
//!
//! Emoji di luar BMP (mis. U+1F4CB) dirender monokrom dari Segoe UI Emoji: `GetGlyphIndicesW` tidak mengerti surrogate
//! pair, jadi indeks glyph dicari sendiri lewat tabel `cmap` (format 12) font itu, lalu dirasterisasi dengan
//! `GGO_GLYPH_INDEX`. Penanda variasi, ZWJ, dan pengubah warna kulit tidak punya lebar (emoji dasarnya tetap tampil).

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// Urutan font untuk kana/kanji/hanzi; font pertama yang punya glyph-nya dipakai.
const FACES_CJK: [&str; 5] = ["Yu Gothic UI", "Meiryo UI", "MS Gothic", "Microsoft YaHei UI", "Malgun Gothic"];
const FACES_HANGUL: [&str; 2] = ["Malgun Gothic", "Yu Gothic UI"];
/// Simbol, tanda baca khusus, dan huruf non-Latin lain.
const FACES_SYMBOL: [&str; 4] = ["Segoe UI", "Segoe UI Symbol", "Segoe UI Emoji", "Arial Unicode MS"];
/// Emoji dan simbol di luar BMP.
const FACES_ASTRAL: [&str; 2] = ["Segoe UI Emoji", "Segoe UI Symbol"];

struct Ctx {
    dc: HDC,
    fonts: HashMap<(u32, bool, &'static str), HFONT>,
    /// Tabel `cmap` per nama font (sama untuk semua ukuran); None = font tidak ada / tanpa cmap.
    cmaps: HashMap<&'static str, Option<Vec<u8>>>,
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

/// Karakter tanpa lebar: penanda variasi (termasuk VS16 emoji), ZWJ/ZWNJ, spasi lebar nol, tag, dan pengubah warna kulit.
pub(crate) fn zero_width(c: char) -> bool {
    matches!(c as u32, 0x200B..=0x200F | 0x2060..=0x2064 | 0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0xE0100..=0xE01EF)
}

/// Karakter yang dilayani jalur GDI ini: semua karakter yang bukan kontrol, termasuk emoji di luar BMP.
pub(crate) fn wants(c: char) -> bool {
    !c.is_control()
}

/// Indeks glyph untuk `cp` dari tabel `cmap` OpenType: subtabel format 12 (Unicode penuh), pencarian biner pada grup.
pub(crate) fn cmap12_lookup(cmap: &[u8], cp: u32) -> Option<u16> {
    let u16_at = |o: usize| cmap.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let u32_at = |o: usize| cmap.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let n = u16_at(2)? as usize;
    for i in 0..n {
        let rec = 4 + i * 8;
        let (plat, enc, off) = (u16_at(rec)?, u16_at(rec + 2)?, u32_at(rec + 4)? as usize);
        if !(plat == 3 && enc == 10 || plat == 0 && enc == 4 || plat == 0 && enc == 6) || u16_at(off)? != 12 {
            continue;
        }
        let groups = u32_at(off + 12)? as usize;
        let (mut lo, mut hi) = (0usize, groups);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let g = off + 16 + mid * 12;
            let (start, end, first) = (u32_at(g)?, u32_at(g + 4)?, u32_at(g + 8)?);
            if cp < start {
                hi = mid;
            } else if cp > end {
                lo = mid + 1;
            } else {
                let id = first.checked_add(cp - start)?;
                return u16::try_from(id).ok().filter(|&v| v != 0);
            }
        }
        return None;
    }
    None
}

fn fixed(v: i16) -> FIXED {
    FIXED { fract: 0, value: v }
}

/// Rasterisasi satu karakter (BMP) menjadi `(Metrics, coverage 0..255)` dengan konvensi yang sama seperti `fontdue`.
pub(crate) fn rasterize(ch: char, size: f32, bold: bool) -> Option<(fontdue::Metrics, Vec<u8>)> {
    if !wants(ch) {
        return None;
    }
    if zero_width(ch) {
        return Some((fontdue::Metrics::default(), Vec::new()));
    }
    CTX.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.is_invalid() {
                return None;
            }
            *c = Some(Ctx { dc, fonts: HashMap::new(), cmaps: HashMap::new() });
        }
        let ctx = c.as_mut()?;
        let faces: &[&'static str] = if ch as u32 > 0xFFFF {
            &FACES_ASTRAL
        } else if is_hangul(ch) {
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
                let r = if ch as u32 > 0xFFFF {
                    let cmap = ctx.cmaps.entry(name).or_insert_with(|| read_cmap(ctx.dc));
                    cmap.as_deref().and_then(|t| cmap12_lookup(t, ch as u32)).and_then(|gi| glyph_index(ctx.dc, gi))
                } else {
                    glyph(ctx.dc, ch)
                };
                SelectObject(ctx.dc, old);
                if r.is_some() {
                    return r;
                }
            }
        }
        None
    })
}

/// Tabel `cmap` mentah dari font yang sedang terpilih di `dc`.
unsafe fn read_cmap(dc: HDC) -> Option<Vec<u8>> {
    const TAG: u32 = u32::from_le_bytes(*b"cmap");
    let n = GetFontData(dc, TAG, 0, None, 0);
    if n == u32::MAX || n < 4 || n > 4 << 20 {
        return None;
    }
    let mut buf = vec![0u8; n as usize];
    (GetFontData(dc, TAG, 0, Some(buf.as_mut_ptr() as *mut c_void), n) == n).then_some(buf)
}

/// Glyph dari font yang sedang terpilih di `dc`; None bila font itu tidak punya karakternya.
unsafe fn glyph(dc: HDC, ch: char) -> Option<(fontdue::Metrics, Vec<u8>)> {
    let u = [ch as u16];
    let mut idx = [0u16; 1];
    if GetGlyphIndicesW(dc, PCWSTR(u.as_ptr()), 1, idx.as_mut_ptr(), GGI_MARK_NONEXISTING_GLYPHS) == u32::MAX || idx[0] == 0xFFFF {
        return None;
    }
    outline(dc, ch as u32, GGO_GRAY8_BITMAP)
}

/// Glyph berdasarkan indeks (karakter di luar BMP).
unsafe fn glyph_index(dc: HDC, gi: u16) -> Option<(fontdue::Metrics, Vec<u8>)> {
    outline(dc, gi as u32, GET_GLYPH_OUTLINE_FORMAT(GGO_GRAY8_BITMAP.0 | GGO_GLYPH_INDEX.0))
}

/// `code` = karakter (BMP) atau indeks glyph bila `fmt` memuat `GGO_GLYPH_INDEX`.
unsafe fn outline(dc: HDC, code: u32, fmt: GET_GLYPH_OUTLINE_FORMAT) -> Option<(fontdue::Metrics, Vec<u8>)> {
    let mat = MAT2 { eM11: fixed(1), eM12: fixed(0), eM21: fixed(0), eM22: fixed(1) };
    let mut gm = GLYPHMETRICS::default();
    let n = GetGlyphOutlineW(dc, code, fmt, &mut gm, 0, None, &mat);
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
    if GetGlyphOutlineW(dc, code, fmt, &mut gm, n, Some(buf.as_mut_ptr() as *mut c_void), &mat) == u32::MAX {
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
        // emoji di luar BMP dilayani; karakter kontrol tidak
        assert!(wants('😀') && !wants('\n'));
        assert!(zero_width('\u{fe0f}') && zero_width('\u{200d}') && zero_width('\u{1f3fd}') && !zero_width('😀'));
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

    /// cmap sintetis: header + satu catatan (3,10) -> subtabel format 12 dengan dua grup terurut.
    fn fake_cmap() -> Vec<u8> {
        let mut t = vec![0, 0, 0, 1, 0, 3, 0, 10, 0, 0, 0, 12];
        let gs: [(u32, u32, u32); 2] = [(0x1F300, 0x1F3FF, 100), (0x1F4CB, 0x1F4CB, 7)];
        t.extend([0, 12, 0, 0]);
        t.extend((16 + gs.len() as u32 * 12).to_be_bytes());
        t.extend([0, 0, 0, 0]);
        t.extend((gs.len() as u32).to_be_bytes());
        for (a, b, c) in gs {
            t.extend(a.to_be_bytes());
            t.extend(b.to_be_bytes());
            t.extend(c.to_be_bytes());
        }
        t
    }

    #[test]
    fn cmap12_binary_search() {
        let t = fake_cmap();
        assert_eq!(cmap12_lookup(&t, 0x1F300), Some(100));
        assert_eq!(cmap12_lookup(&t, 0x1F310), Some(116));
        assert_eq!(cmap12_lookup(&t, 0x1F2FF), None);
        assert_eq!(cmap12_lookup(&[0, 0, 0], 0x1F300), None);
    }

    #[test]
    fn emoji_outside_bmp_renders_monochrome() {
        // memakai Segoe UI Emoji bila ada; Windows tanpa font itu melewati tes ini
        let Some((m, bm)) = rasterize('\u{1F4CB}', 14.0, false) else { return };
        assert!(m.advance_width > 6.0, "advance {}", m.advance_width);
        assert!(m.width > 4 && m.height > 4 && bm.iter().any(|&v| v > 100), "glyph kosong: {}x{}", m.width, m.height);
        // penanda variasi tidak memakan lebar
        let (z, zb) = rasterize('\u{fe0f}', 14.0, false).unwrap();
        assert!(z.advance_width == 0.0 && zb.is_empty());
    }
}
