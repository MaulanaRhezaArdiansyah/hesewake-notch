# Changelog

Format mengikuti [Keep a Changelog](https://keepachangelog.com/), versi mengikuti [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-24

Rilis pertama **Hesewake Notch**, fork pribadi dari [Poch](https://github.com/klawcodes/poch) 0.7.0 oleh Commonly Works (MIT).

### Added
- **Lirik sinkron** dari LRCLIB: baris aktif tampil di notch kecil (notch melebar mengikuti panjang baris, teks tebal, tanpa running text) dan di slide Music.
- **Mode lirik** di slide Music (tombol balon): panel kiri cover, judul, progres, kontrol; panel kanan daftar lirik yang meluncur halus mengikuti baris aktif. Klik baris untuk lompat ke posisi lagu itu.
- **Cover album** menggantikan ikon aplikasi di notch kecil saat lagu main, dengan transisi pegas.
- **Water baru**: angka besar menghitung naik, toggle pengingat, progress bar gradasi, tombol Drink dan Settings, grafik batang per jam, pilihan goal/interval/ukuran gelas di dalam slide, tombol Snooze saat pengingat aktif.
- **Toast di notch**: charger tersambung, charger dicabut, baterai rendah 20%, dan perangkat audio Bluetooth tersambung/terputus, dengan animasi pegas dan riak drop-wave.
- **Notifikasi Windows di notch** (UserNotificationListener): toast saat notifikasi masuk, ikon lonceng dengan badge belum dibaca di notch kecil, dan slide **Notifications** (daftar, hapus per item, Clear all, scroll). Bisa dimatikan dari menu; aplikasi bisa dibisukan lewat `notif_mute.txt`.
- **Tray icon + menu**, **Start with Windows**, mode **Minimal notch**, indikator koneksi internet di slide Music, **single instance**, dan jeda render/polling saat layar mati.
- Ikon aplikasi baru (notch + mata, gradasi biru) dan tema aksen biru: glow slide To-do/Ledge/Scrap/Notifications, titik slide aktif, spektrum equalizer, dan progress bar lagu.
- Alat diagnosa: log performa/memori lewat `HESEWAKE_PERF=1` (`perf.log`), argumen `--demo-toast=...` dan `--demo-slide=N` untuk pratinjau visual.

### Changed
- **Rebrand** dari Poch ke Hesewake Notch: nama paket/exe, folder data (`%APPDATA%\hesewake-notch`), mutex, class window, dan entri autostart. Data dan autostart lama dimigrasi otomatis. About menyebut Poch sebagai dasar.
- **Performa**: `opt-level = 3`, animasi memakai waitable timer resolusi tinggi dan loop pesan sendiri (menggantikan `WM_TIMER`), cache glyph lebih besar dengan pemanasan awal, alokasi per frame dikurangi.
- Memori dipangkas ±tiap 60 detik (sebelumnya tidak pernah saat musik main) dan decode gambar dibatasi 32 MB.

### Fixed
- Cover lagu sebelumnya yang "nyangkut" (thumbnail GSMTC basi) dan cover yang hilang karena balapan antar-thread.
- Pencarian cover online gagal untuk judul berakhiran " / ", " - ...", atau "(feat. ...)"; ditambah percobaan ulang.
- Posisi lagu/lirik yang macet pada pemutar yang jarang memperbarui `LastUpdatedTime` (mis. Spotify).
- Tes tidak lagi menimpa `water.txt` asli.
