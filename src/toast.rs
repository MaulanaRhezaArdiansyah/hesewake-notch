//! Toast di notch: charger tersambung/dicabut, baterai rendah, perangkat Bluetooth tersambung.
//! Notch melebar dengan pegas (lihat `target`), isi masuk bertahap, lalu menutup sendiri.

use super::*;

pub(crate) const TOAST_CHARGE: (f32, f32) = (336.0, 70.0);
pub(crate) const TOAST_BT: (f32, f32) = (372.0, 70.0);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ToastKind {
    Charging { pct: u8 },
    Unplugged { pct: u8 },
    LowBattery { pct: u8 },
    Bluetooth { name: String, pct: Option<u8>, connected: bool },
    /// Notifikasi Windows; isinya dicari di `App.notifs` berdasarkan id.
    Notification { id: u32 },
}

pub(crate) struct Toast {
    pub(crate) kind: ToastKind,
    pub(crate) t0: Instant,
    pub(crate) dur: f32,
}

const GREEN: [u8; 3] = [60, 200, 110];
const BLUE: [u8; 3] = [90, 190, 255];
const GREY: [u8; 3] = [150, 150, 158];

fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Pegas teredam 0 -> 1 dengan sedikit overshoot (dipakai untuk ikon yang "muncul").
fn pop(t: f32) -> f32 {
    1.0 - (-8.0 * t).exp() * (14.0 * t).cos()
}

impl App {
    pub(crate) fn push_toast(&mut self, kind: ToastKind) {
        if self.display_off {
            return;
        }
        if self.toast.is_none() && !self.expanded() {
            self.start_toast(kind);
        } else if self.toast_q.len() < 3 {
            self.toast_q.push(kind);
        }
    }

    fn start_toast(&mut self, kind: ToastKind) {
        let dur = match kind {
            ToastKind::LowBattery { .. } => 4.0,
            ToastKind::Bluetooth { .. } => 3.8,
            ToastKind::Notification { .. } => 4.5,
            _ => 3.4,
        };
        perf::note(&format!("toast start {:?} mem={:.1}MB", kind, perf::mem_mb()));
        self.toast = Some(Toast { kind, t0: Instant::now(), dur });
        self.drop_pulse = Some(Instant::now()); // riak tepi bawah, sama dengan saat drop file di Ledge
        self.last = Instant::now();
    }

    /// Selesaikan toast yang habis waktu / dibatalkan hover, lalu mulai berikutnya. True bila ada perubahan.
    pub(crate) fn toast_update(&mut self) -> bool {
        let mut changed = false;
        if let Some(t) = &self.toast {
            if self.expanded() || t.t0.elapsed().as_secs_f32() >= t.dur {
                let to_notif = self.expanded() && matches!(t.kind, ToastKind::Notification { .. });
                self.toast = None;
                if to_notif {
                    self.show_id(SlideId::Notifications);
                }
                self.toast_end = Instant::now();
                changed = true;
            }
        }
        if self.toast.is_none() && !self.expanded() && !self.toast_q.is_empty() {
            let k = self.toast_q.remove(0);
            self.start_toast(k);
            changed = true;
        }
        changed
    }

    /// Notch sedang (atau baru saja) berukuran toast: jangan hitung progres expand dari tinggi notch.
    pub(crate) fn toast_hold(&self) -> bool {
        !self.expanded() && (self.toast.is_some() || self.toast_end.elapsed().as_secs_f32() < 0.8)
    }

    pub(crate) fn toast_size(&self) -> (f32, f32) {
        match self.toast.as_ref().map(|t| &t.kind) {
            Some(ToastKind::Bluetooth { .. }) | Some(ToastKind::Notification { .. }) => TOAST_BT,
            _ => TOAST_CHARGE,
        }
    }

    /// Dipanggil saat status daya berubah dan berkala: toast saat charger dicolok/dicabut, baterai <= 20%.
    pub(crate) fn power_check(&mut self) {
        let Some((ac, pct)) = sys::power_status() else { return };
        if let Some(prev) = self.ac_prev {
            if prev != ac {
                self.push_toast(if ac { ToastKind::Charging { pct } } else { ToastKind::Unplugged { pct } });
            }
        }
        self.ac_prev = Some(ac);
        if ac || pct > 25 {
            self.low_armed = true;
        } else if pct <= 20 && self.low_armed {
            self.low_armed = false;
            self.push_toast(ToastKind::LowBattery { pct });
        }
    }

    pub(crate) fn draw_toast(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        let Some(ts) = &self.toast else { return };
        let t = ts.t0.elapsed().as_secs_f32();
        let al = ca * (1.0 - ss(ts.dur - 0.25, ts.dur, t));
        if al <= 0.01 {
            return;
        }
        let (x0, cy) = (CX - w / 2.0, h / 2.0);
        if let ToastKind::Notification { id } = &ts.kind {
            self.draw_notif_toast(g, w, h, al, t, *id);
            return;
        }

        // (warna aksen, judul, subjudul, fraksi ring, teks kanan)
        let (rgb, title, sub, frac, right) = match &ts.kind {
            ToastKind::Charging { pct } => (GREEN, "Charging".to_string(), "Charger connected".to_string(), *pct as f32 / 100.0, Some(format!("{}%", pct))),
            ToastKind::Unplugged { pct } => ([235, 235, 240], "On battery".to_string(), "Charger disconnected".to_string(), *pct as f32 / 100.0, Some(format!("{}%", pct))),
            ToastKind::LowBattery { pct } => (ACCENT, "Battery low".to_string(), "Plug in your charger".to_string(), *pct as f32 / 100.0, Some(format!("{}%", pct))),
            ToastKind::Bluetooth { name, pct, connected } => {
                let s = match (connected, pct) {
                    (true, Some(p)) => format!("Connected \u{b7} {}%", p),
                    (true, None) => "Connected".to_string(),
                    (false, _) => "Disconnected".to_string(),
                };
                (if *connected { BLUE } else { GREY }, name.clone(), s, pct.map_or(1.0, |p| p as f32 / 100.0), None)
            }
            ToastKind::Notification { .. } => unreachable!(),
        };

        // Ring + ikon: membesar dengan pegas, ring menyapu dari 0.
        let sc = 0.55 + 0.45 * pop(t).clamp(0.0, 1.2);
        let (rcx, r) = (x0 + 36.0, 17.0 * sc);
        let sweep = {
            let k = ((t - 0.1) / 0.7).clamp(0.0, 1.0);
            1.0 - (1.0 - k).powi(3)
        };
        let breathe = if matches!(ts.kind, ToastKind::Charging { .. } | ToastKind::LowBattery { .. }) {
            0.78 + 0.22 * (t * 5.0).sin()
        } else {
            1.0
        };
        let fg = paint(rgb[0], rgb[1], rgb[2], al);
        g.ring(rcx, cy, r, frac * sweep, 3.5, &paint(rgb[0], rgb[1], rgb[2], 0.2 * al), &fg);
        let ic = paint(rgb[0], rgb[1], rgb[2], al * breathe);
        match &ts.kind {
            ToastKind::Charging { .. } => g.icon_bolt(rcx, cy, 17.0 * sc, &ic),
            ToastKind::Unplugged { pct } => g.icon_battery(rcx, cy, 16.0 * sc, *pct as f32 / 100.0, &ic),
            ToastKind::LowBattery { .. } => g.icon_battery(rcx, cy, 16.0 * sc, 0.12, &ic),
            ToastKind::Bluetooth { .. } => g.icon_bluetooth(rcx, cy, 17.0 * sc, &ic),
            ToastKind::Notification { .. } => {}
        }

        // Teks: geser 6 px + fade dengan jeda singkat.
        let k = ss(0.12, 0.4, t);
        let off = (1.0 - k) * 6.0;
        let tx = x0 + 66.0 + off;
        let max_w = w - 66.0 - 76.0;
        let title = if g.width_ex(&title, 15.0, true) > max_w { ellipsize(g, &title, 15.0, max_w) } else { title };
        g.txtb(&title, tx, cy - 21.0, 15.0, [255, 255, 255], al * k, max_w + 2.0);
        g.txt(&sub, tx, cy + 2.0, 12.0, GREY, al * k, max_w + 2.0);

        // Kanan: persen (charger) atau orb headphone dengan riak (Bluetooth).
        if let Some(txt) = right {
            let col = if matches!(ts.kind, ToastKind::LowBattery { .. }) { ACCENT } else { [255, 255, 255] };
            let tw = g.width(&txt, 24.0);
            g.txt(&txt, x0 + w - 26.0 - tw - (1.0 - k) * -6.0, cy - 16.0, 24.0, col, al * k, 100.0);
        } else if let ToastKind::Bluetooth { connected: true, .. } = &ts.kind {
            let (ox, oy) = (x0 + w - 40.0, cy);
            for i in 0..2 {
                let ph = ((t * 0.85) + i as f32 * 0.5).rem_euclid(1.0);
                let rr = 20.0 + 13.0 * ph;
                let a = (1.0 - ph) * 0.4 * al * k;
                let p = paint(BLUE[0], BLUE[1], BLUE[2], a);
                g.ring(ox, oy, rr, 0.0, 1.5, &p, &p);
            }
            g.circle(ox, oy, 19.0 * sc, &paint(20, 40, 58, al * k));
            let edge = paint(BLUE[0], BLUE[1], BLUE[2], 0.7 * al * k);
            g.ring(ox, oy, 19.0 * sc, 0.0, 1.5, &edge, &edge);
            g.icon_headphone(ox, oy, 17.0 * sc, &paint(205, 235, 255, al * k));
        }
    }
}

impl App {
    /// Toast notifikasi Windows: ikon aplikasi, judul tebal, isi satu baris, lonceng bergoyang di kanan.
    fn draw_notif_toast(&self, g: &mut Gfx, w: f32, h: f32, al: f32, t: f32, id: u32) {
        let Some(item) = self.notifs.iter().find(|n| n.id == id) else { return };
        let (x0, cy) = (CX - w / 2.0, h / 2.0);
        let sc = 0.55 + 0.45 * pop(t).clamp(0.0, 1.2);
        let isz = 34.0 * sc;
        notif::draw_avatar(g, item, x0 + 20.0 + (34.0 - isz) / 2.0, cy - isz / 2.0, isz, al);

        let k = ss(0.12, 0.4, t);
        let tx = x0 + 66.0 + (1.0 - k) * 6.0;
        let max_w = w - 66.0 - 56.0;
        let title = if item.title.is_empty() { item.app.as_str() } else { item.title.as_str() };
        let title = if g.width_ex(title, 14.0, true) > max_w { ellipsize(g, title, 14.0, max_w) } else { title.to_string() };
        g.txtb(&title, tx, cy - 21.0, 14.0, [255, 255, 255], al * k, max_w + 2.0);
        let body = if item.body.is_empty() { item.app.clone() } else { item.body.replace('\n', " ") };
        let body = if g.width(&body, 12.0) > max_w { ellipsize(g, &body, 12.0, max_w) } else { body };
        g.txt(&body, tx, cy + 2.0, 12.0, GREY, al * k, max_w + 2.0);

        let swing = (t * 22.0).sin() * (1.0 - t / 0.9).max(0.0);
        g.icon_bell(x0 + w - 30.0, cy, 16.0 * sc, swing, &paint(255, 255, 255, al * k * 0.9));
    }
}
