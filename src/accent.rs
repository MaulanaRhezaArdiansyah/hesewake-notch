//! Warna aksen yang bisa dipilih di Settings > Extras. Merah peringatan, hijau "selesai", biru air/Bluetooth,
//! dan tomat Pomodoro tidak ikut berubah karena warnanya punya arti sendiri.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Accent {
    pub(crate) name: &'static str,
    /// Warna utama: judul slide, isi bar, tombol, titik navigasi.
    pub(crate) main: [u8; 3],
    /// Sedikit lebih terang untuk teks kecil dan garis tipis (angka stepper, grafik, timer).
    pub(crate) bright: [u8; 3],
    /// Glow bawah slide (lebih redup dari `main`).
    pub(crate) glow: [u8; 3],
    /// Track toggle yang menyala dan ikon bel di knob-nya.
    pub(crate) deep: [u8; 3],
    /// Teks chip yang terpilih (pucat).
    pub(crate) pale: [u8; 3],
}

/// Indeks 0 = biru bawaan (sama dengan warna sebelum ada pilihan aksen).
pub(crate) const ACCENTS: [Accent; 6] = [
    Accent { name: "Blue", main: [90, 150, 255], bright: [90, 170, 255], glow: [90, 120, 200], deep: [40, 132, 245], pale: [190, 225, 255] },
    Accent { name: "Purple", main: [165, 120, 255], bright: [180, 145, 255], glow: [125, 95, 205], deep: [130, 80, 240], pale: [225, 210, 255] },
    Accent { name: "Pink", main: [255, 105, 180], bright: [255, 135, 200], glow: [195, 85, 150], deep: [230, 65, 150], pale: [255, 210, 232] },
    Accent { name: "Orange", main: [255, 150, 60], bright: [255, 172, 90], glow: [200, 120, 60], deep: [235, 120, 30], pale: [255, 225, 192] },
    Accent { name: "Green", main: [70, 200, 120], bright: [95, 215, 140], glow: [55, 155, 100], deep: [35, 170, 95], pale: [195, 240, 210] },
    Accent { name: "Teal", main: [50, 200, 210], bright: [85, 215, 225], glow: [40, 145, 170], deep: [20, 165, 180], pale: [190, 240, 245] },
];

/// Aksen ke-`i`; indeks di luar daftar jatuh ke biru bawaan.
pub(crate) fn get(i: usize) -> Accent {
    ACCENTS.get(i).copied().unwrap_or(ACCENTS[0])
}

/// Indeks berikutnya/sebelumnya, berputar.
pub(crate) fn step(i: usize, forward: bool) -> usize {
    let n = ACCENTS.len();
    if forward { (i + 1) % n } else { (i % n + n - 1) % n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_is_the_previous_hardcoded_palette() {
        let b = get(0);
        assert_eq!(b.main, [90, 150, 255]);
        assert_eq!(b.bright, [90, 170, 255]);
        assert_eq!(b.glow, [90, 120, 200]);
        assert_eq!(b.deep, [40, 132, 245]);
        assert_eq!(b.pale, [190, 225, 255]);
    }

    #[test]
    fn get_falls_back_and_step_wraps() {
        assert_eq!(get(99), ACCENTS[0]);
        assert_eq!(step(0, false), ACCENTS.len() - 1);
        assert_eq!(step(ACCENTS.len() - 1, true), 0);
        assert_eq!(step(2, true), 3);
    }

    #[test]
    fn accent_text_stays_readable_on_black() {
        // Kontras WCAG terhadap latar notch (hitam) minimal 4.5:1 untuk warna yang dipakai sebagai teks.
        fn lum(c: [u8; 3]) -> f64 {
            let ch = |v: u8| {
                let s = v as f64 / 255.0;
                if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
        }
        for a in ACCENTS {
            for c in [a.main, a.bright, a.pale] {
                let ratio = (lum(c) + 0.05) / 0.05;
                assert!(ratio >= 4.5, "{} {:?} kontras {:.2}", a.name, c, ratio);
            }
        }
        let names: std::collections::HashSet<_> = ACCENTS.iter().map(|a| a.name).collect();
        assert_eq!(names.len(), ACCENTS.len());
    }
}
