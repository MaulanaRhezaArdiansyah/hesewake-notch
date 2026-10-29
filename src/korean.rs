//! Lirik Korea -> romanisasi (Revised Romanization, sistem resmi Korea Selatan). Hangul tersusun aritmetis di Unicode,
//! jadi tidak perlu kamus: tiap suku kata dipecah jadi konsonan awal, vokal, dan konsonan akhir, aturan bunyi antar
//! suku kata (liaison, nasalisasi, lateralisasi, aspirasi, palatalisasi) diterapkan dalam satu kata, lalu dilatinkan.
//! Sesuai RR, pengerasan bunyi (경음화) tidak ditulis: 학교 = "hakgyo".

use crate::lyrics::LyricLine;

/// Konsonan awal, urutan Unicode.
const INITIALS: [char; 19] = ['ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'];
/// Konsonan akhir (batchim), urutan Unicode; indeks 0 = tanpa akhir. Gugus konsonan berisi dua huruf.
const FINALS: [&[char]; 28] = [
    &[],
    &['ㄱ'],
    &['ㄲ'],
    &['ㄱ', 'ㅅ'],
    &['ㄴ'],
    &['ㄴ', 'ㅈ'],
    &['ㄴ', 'ㅎ'],
    &['ㄷ'],
    &['ㄹ'],
    &['ㄹ', 'ㄱ'],
    &['ㄹ', 'ㅁ'],
    &['ㄹ', 'ㅂ'],
    &['ㄹ', 'ㅅ'],
    &['ㄹ', 'ㅌ'],
    &['ㄹ', 'ㅍ'],
    &['ㄹ', 'ㅎ'],
    &['ㅁ'],
    &['ㅂ'],
    &['ㅂ', 'ㅅ'],
    &['ㅅ'],
    &['ㅆ'],
    &['ㅇ'],
    &['ㅈ'],
    &['ㅊ'],
    &['ㅋ'],
    &['ㅌ'],
    &['ㅍ'],
    &['ㅎ'],
];
/// Vokal, urutan Unicode.
const VOWELS: [&str; 21] = ["a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i"];
const VOWEL_I: usize = 20;

/// Satu suku kata: konsonan awal, indeks vokal, konsonan akhir (0-2 huruf).
struct Syl {
    init: char,
    vowel: usize,
    fin: Vec<char>,
}

fn is_syllable(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

/// Ada suku kata Hangul. (Jamo tunggal seperti ㅋㅋ tidak dihitung.)
pub(crate) fn has_hangul(s: &str) -> bool {
    s.chars().any(is_syllable)
}

fn decompose(c: char) -> Syl {
    let n = c as u32 - 0xAC00;
    Syl { init: INITIALS[(n / 588) as usize], vowel: ((n % 588) / 28) as usize, fin: FINALS[(n % 28) as usize].to_vec() }
}

/// Konsonan akhir yang diucapkan (penetralan): ㅅㅆㅈㅊㅌ -> ㄷ, ㅋㄲ -> ㄱ, ㅍ -> ㅂ, dan gugus jadi satu bunyi.
fn neutral(f: &[char]) -> Option<char> {
    let c = match f {
        [] => return None,
        [a] => *a,
        // gugus: ㄳ ㄵ ㄶ ㄺ ㄻ ㄼ ㄽ ㄾ ㄿ ㅀ ㅄ
        [a, b] => match (a, b) {
            ('ㄹ', 'ㄱ') => 'ㄱ',
            ('ㄹ', 'ㅁ') => 'ㅁ',
            ('ㄹ', 'ㅍ') => 'ㅂ',
            ('ㅂ', _) => 'ㅂ',
            _ => *a,
        },
        _ => return None,
    };
    Some(match c {
        'ㄱ' | 'ㅋ' | 'ㄲ' => 'ㄱ',
        'ㄷ' | 'ㅅ' | 'ㅆ' | 'ㅈ' | 'ㅊ' | 'ㅌ' | 'ㅎ' => 'ㄷ',
        'ㅂ' | 'ㅍ' => 'ㅂ',
        other => other,
    })
}

fn aspirated(c: char) -> Option<char> {
    match c {
        'ㄱ' => Some('ㅋ'),
        'ㄷ' => Some('ㅌ'),
        'ㅂ' => Some('ㅍ'),
        'ㅈ' => Some('ㅊ'),
        _ => None,
    }
}

fn nasal(f: char) -> char {
    match f {
        'ㄱ' => 'ㅇ',
        'ㄷ' => 'ㄴ',
        _ => 'ㅁ',
    }
}

/// Terapkan aturan bunyi antar suku kata dalam satu kata.
fn assimilate(syls: &mut [Syl]) {
    for i in 0..syls.len().saturating_sub(1) {
        if syls[i].fin.is_empty() {
            continue;
        }
        let (head, tail) = syls.split_at_mut(i + 1);
        let (cur, next) = (&mut head[i], &mut tail[0]);
        if next.init == 'ㅇ' {
            // liaison: konsonan akhir pindah ke suku kata berikutnya (ㅇ tetap; ㅎ lenyap)
            if cur.fin == ['ㅇ'] {
                continue;
            }
            let mut moved = cur.fin.pop().unwrap_or('ㅇ');
            // ㄶ ㅀ: ㅎ lenyap dan konsonan di depannya yang pindah (잃은 = "ireun", 많아 = "mana")
            if moved == 'ㅎ' {
                if let Some(c) = cur.fin.pop() {
                    moved = c;
                }
            }
            next.init = match moved {
                'ㅎ' => 'ㅇ',
                // palatalisasi: 같이 = "gachi", 굳이 = "guji"
                'ㄷ' if next.vowel == VOWEL_I => 'ㅈ',
                'ㅌ' if next.vowel == VOWEL_I => 'ㅊ',
                m => m,
            };
            continue;
        }
        // ㅎ di akhir (ㅎ ㄶ ㅀ): lenyap, tetapi mengaspirasi konsonan berikutnya (놓다 = "nota")
        if cur.fin.last() == Some(&'ㅎ') {
            cur.fin.pop();
            if let Some(asp) = aspirated(next.init) {
                next.init = asp;
            }
            if cur.fin.is_empty() {
                continue;
            }
        }
        // ㅎ di awal setelah ㄱ ㄷ ㅂ ㅈ: konsonan akhir melebur ke konsonan awal beraspirasi (먹히다 = "meokida")
        if next.init == 'ㅎ' && cur.fin.len() == 1 {
            let orig = cur.fin[0];
            if let Some(asp) = neutral(&[orig]).and_then(aspirated) {
                next.init = if matches!(orig, 'ㅈ' | 'ㅊ') { 'ㅊ' } else { asp };
                cur.fin.clear();
                continue;
            }
        }
        // ㄺ + ㄱ: ㄹ yang diucapkan (읽고 = "ilgo")
        if cur.fin == ['ㄹ', 'ㄱ'] && next.init == 'ㄱ' {
            cur.fin = vec!['ㄹ'];
            continue;
        }
        let Some(f) = neutral(&cur.fin) else { continue };
        cur.fin = vec![f];
        match (f, next.init) {
            // nasalisasi
            ('ㄱ' | 'ㄷ' | 'ㅂ', 'ㄴ' | 'ㅁ') => cur.fin = vec![nasal(f)],
            ('ㄱ' | 'ㄷ' | 'ㅂ', 'ㄹ') => {
                cur.fin = vec![nasal(f)];
                next.init = 'ㄴ';
            }
            ('ㅁ' | 'ㅇ', 'ㄹ') => next.init = 'ㄴ',
            // lateralisasi
            ('ㄴ', 'ㄹ') => cur.fin = vec!['ㄹ'],
            ('ㄹ', 'ㄴ') => next.init = 'ㄹ',
            _ => {}
        }
    }
}

fn initial_roman(c: char, after_l: bool) -> &'static str {
    match c {
        'ㄱ' => "g",
        'ㄲ' => "kk",
        'ㄴ' => "n",
        'ㄷ' => "d",
        'ㄸ' => "tt",
        'ㄹ' => {
            if after_l {
                "l"
            } else {
                "r"
            }
        }
        'ㅁ' => "m",
        'ㅂ' => "b",
        'ㅃ' => "pp",
        'ㅅ' => "s",
        'ㅆ' => "ss",
        'ㅈ' => "j",
        'ㅉ' => "jj",
        'ㅊ' => "ch",
        'ㅋ' => "k",
        'ㅌ' => "t",
        'ㅍ' => "p",
        'ㅎ' => "h",
        _ => "",
    }
}

fn final_roman(f: &[char]) -> &'static str {
    match neutral(f) {
        Some('ㄱ') => "k",
        Some('ㄴ') => "n",
        Some('ㄷ') => "t",
        Some('ㄹ') => "l",
        Some('ㅁ') => "m",
        Some('ㅂ') => "p",
        Some('ㅇ') => "ng",
        _ => "",
    }
}

/// Satu kata (deretan suku kata Hangul) ke huruf Latin.
fn word(chars: &[char]) -> String {
    let mut syls: Vec<Syl> = chars.iter().map(|&c| decompose(c)).collect();
    assimilate(&mut syls);
    let mut out = String::new();
    let mut prev_l = false;
    for s in &syls {
        out.push_str(initial_roman(s.init, prev_l));
        out.push_str(VOWELS[s.vowel]);
        out.push_str(final_roman(&s.fin));
        prev_l = s.fin.last() == Some(&'ㄹ');
    }
    out
}

/// Romanisasi satu baris; karakter selain suku kata Hangul (spasi, tanda baca, huruf Latin) tidak diubah.
pub(crate) fn romanize(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + line.len() / 2);
    let mut run: Vec<char> = Vec::new();
    for c in line.chars() {
        if is_syllable(c) {
            run.push(c);
            continue;
        }
        if !run.is_empty() {
            out.push_str(&word(&run));
            run.clear();
        }
        out.push(c);
    }
    if !run.is_empty() {
        out.push_str(&word(&run));
    }
    out
}

/// Isi `roma` untuk tiap baris yang berisi Hangul. Cepat dan tanpa kamus; aman dipanggil dari thread mana pun.
pub(crate) fn annotate(lines: &mut [LyricLine]) {
    for l in lines.iter_mut().filter(|l| has_hangul(&l.text)) {
        l.roma = romanize(&l.text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_syllables() {
        assert_eq!(romanize("사랑해"), "saranghae");
        assert_eq!(romanize("안녕하세요"), "annyeonghaseyo");
        assert_eq!(romanize("감사합니다"), "gamsahamnida");
        assert_eq!(romanize("대한민국"), "daehanminguk");
        assert_eq!(romanize("노래"), "norae");
        assert_eq!(romanize("라면"), "ramyeon");
        assert_eq!(romanize("의사"), "uisa");
        assert_eq!(romanize("괜찮아"), "gwaenchana");
    }

    #[test]
    fn liaison_moves_the_final_to_the_next_syllable() {
        assert_eq!(romanize("한국어"), "hangugeo");
        assert_eq!(romanize("있어"), "isseo");
        assert_eq!(romanize("꽃이"), "kkochi");
        assert_eq!(romanize("앉아"), "anja");
        assert_eq!(romanize("달이"), "dari");
        assert_eq!(romanize("좋아"), "joa");
        assert_eq!(romanize("많이"), "mani");
        assert_eq!(romanize("종이"), "jongi", "ㅇ never moves");
        assert_eq!(romanize("잃은"), "ireun", "ㅎ is silent, the ㄹ before it moves");
        assert_eq!(romanize("많아"), "mana");
        assert_eq!(romanize("읽어"), "ilgeo");
        assert_eq!(romanize("앉은"), "anjeun");
        assert_eq!(romanize("싫어"), "sireo");
    }

    #[test]
    fn palatalization_before_i() {
        assert_eq!(romanize("같이"), "gachi");
        assert_eq!(romanize("굳이"), "guji");
        assert_eq!(romanize("해돋이"), "haedoji");
    }

    #[test]
    fn nasalization_and_lateralization() {
        assert_eq!(romanize("국물"), "gungmul");
        assert_eq!(romanize("독립"), "dongnip");
        assert_eq!(romanize("입니다"), "imnida");
        assert_eq!(romanize("종로"), "jongno");
        assert_eq!(romanize("신라"), "silla");
        assert_eq!(romanize("칼날"), "kallal");
        assert_eq!(romanize("꽃물"), "kkonmul");
    }

    #[test]
    fn aspiration_with_h() {
        assert_eq!(romanize("놓다"), "nota");
        assert_eq!(romanize("않다"), "anta");
        assert_eq!(romanize("먹히다"), "meokida");
        assert_eq!(romanize("좋고"), "joko");
    }

    #[test]
    fn finals_at_the_end_of_a_word() {
        assert_eq!(romanize("밥"), "bap");
        assert_eq!(romanize("앞"), "ap");
        assert_eq!(romanize("부엌"), "bueok");
        assert_eq!(romanize("값"), "gap");
        assert_eq!(romanize("삶"), "sam");
        assert_eq!(romanize("옷"), "ot");
        assert_eq!(romanize("닭"), "dak");
        assert_eq!(romanize("읽다"), "ikda");
        assert_eq!(romanize("읽고"), "ilgo");
    }

    #[test]
    fn tensification_is_not_written_in_rr() {
        assert_eq!(romanize("학교"), "hakgyo");
        assert_eq!(romanize("맛있다"), "masitda");
        assert_eq!(romanize("합격"), "hapgyeok");
    }

    #[test]
    fn other_characters_pass_through_and_words_stay_apart() {
        assert_eq!(romanize("I love 너, 정말!"), "I love neo, jeongmal!");
        assert_eq!(romanize("먹어 어때"), "meogeo eottae", "no liaison across a space");
        assert_eq!(romanize("ㅋㅋ"), "ㅋㅋ", "lone jamo stay as they are");
        assert_eq!(romanize(""), "");
    }

    #[test]
    fn annotate_only_touches_hangul_lines() {
        let mut v = vec![
            LyricLine { at_ms: 0, text: "안녕".into(), roma: String::new() },
            LyricLine { at_ms: 1, text: "hello".into(), roma: String::new() },
            LyricLine { at_ms: 2, text: "你好".into(), roma: String::new() },
        ];
        annotate(&mut v);
        assert_eq!(v[0].roma, "annyeong");
        assert!(v[1].roma.is_empty() && v[2].roma.is_empty());
        assert!(has_hangul("노래 song") && !has_hangul("ㅋㅋㅋ") && !has_hangul("歌"));
    }
}
