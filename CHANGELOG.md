# Changelog

Format mengikuti [Keep a Changelog](https://keepachangelog.com/), versi mengikuti [Semantic Versioning](https://semver.org/).

## [Unreleased] - 0.3.1

### Fixed
- **Teks Jepang, Mandarin, dan Korea** (judul lagu, lirik, notifikasi, to-do) sebelumnya tampil sebagai kotak kosong karena font bawaan tidak punya kana, kanji, atau hangul. Karakter CJK kini dirender lewat GDI dari font sistem (Yu Gothic UI, Meiryo UI, MS Gothic, Microsoft YaHei UI, Malgun Gothic) dan masuk ke cache glyph yang sama. Memuat font CJK ke `fontdue` sempat dicoba tetapi menaikkan memori sekitar 420 MB, jadi tidak dipakai; jalur GDI tidak menambah memori.
- Flag uji `--demo-text=judul|isi` untuk melihat notifikasi dengan teks non-Latin.

## [0.3.0] - 2026-10-25

### Added
- **HUD volume dan kecerahan** di notch kecil: bar dengan ikon speaker/matahari dan persen, muncul saat volume berubah dari luar (tombol media, mixer Windows) atau kecerahan layar berubah (tombol Fn, slider Windows). Volume dipantau lewat callback `IAudioEndpointVolume` (otomatis pindah saat perangkat audio default berganti), kecerahan lewat notifikasi daya `GUID_DEVICE_POWER_POLICY_VIDEO_BRIGHTNESS` (terbukti di laptop ini). Perubahan beruntun memperbarui bar di tempat. Toggle di Settings > Features. HUD bawaan Windows tetap muncul (tidak bisa dimatikan secara resmi).

- **Slide System** (default **tersembunyi**; nyalakan di Settings > Slides): CPU dengan sparkline, memori (GB + ring), jaringan (unduh/unggah + sparkline, hanya adapter fisik yang aktif), dan baterai (persen, status, sisa waktu). Pengukuran hanya berjalan selama slide terlihat.
- Tab Slides di Settings kini 3 kolom agar muat lebih banyak slide; slide opsional baru dari versi mendatang otomatis tersembunyi.
- **Cuaca di kiri jam** (notch idle, tanpa lagu): suhu ("27°") dan ikon kondisi bergantian tiap ±4 dtk dengan transisi geser + fade. Ikon vektor beranimasi dari kode cuaca WMO + siang/malam: matahari, bulan, berawan, hujan/gerimis (tetesan jatuh), salju, kabut, petir. Varian **panas**: cerah/berawan tipis siang hari dengan suhu >= 32 C memakai matahari oranye berdenyut. Data dari Open-Meteo (tanpa API key) tiap 30 menit di thread latar belakang, di-cache di `weather.txt`; jaringan hanya dipakai saat fitur aktif. Toggle dan pilihan kota (36 kota Indonesia, termasuk Karawang) di Settings > Display.
- Flag uji `--demo-weather=kode,suhu,siang`.
- **Multi-monitor**: pilih monitor notch di Settings > Display > Monitor: Primary, Follow (pindah ke monitor tempat kursor berada, hanya saat notch tidak dipakai), atau Display 1, 2, ... Notch dipindah ulang otomatis saat monitor dicolok/dicabut (`WM_DISPLAYCHANGE`); monitor yang hilang jatuh kembali ke layar utama. Deteksi layar penuh dan zona hover kini memakai monitor notch, bukan layar utama saja.
- Settings punya tab keempat **Extras** (HUD volume/kecerahan, cuaca, kota); Notifications dan Contact photos dikelompokkan di Features.

### Fixed
- Pesan daya Windows (kecerahan, charger, layar mati/hidup) kini diteruskan lewat pesan yang di-post. Sebelumnya bisa terlewat bila tiba saat state aplikasi sedang dipegang, sehingga toast charger atau pemulihan timer setelah layar hidup berisiko hilang.
- HUD (tombol kunci, volume, kecerahan) tidak lagi diantre saat notch sedang terbuka atau toast lain tampil, sehingga tidak muncul terlambat.

## [0.2.0] - 2026-10-24

### Added
- **Slide Settings** dengan tiga tab. Slide ini **hanya muncul di carousel saat dibuka** lewat klik kanan/tray > "Settings", lalu hilang lagi setelah kamu pindah slide atau notch menutup (tidak memakan tempat di titik slide):
  - **Slides**: tampilkan/sembunyikan tiap slide dan ubah urutannya dengan tombol naik/turun (sel bertukar tempat dengan animasi).
  - **Display**: Minimal notch, "Thin on maximize", penyesuaian waktu lirik (-500 sampai +1000 ms), foto kontak pada notifikasi.
  - **Features**: notifikasi Windows dan autostart.
  Menu klik kanan dan tray punya item "Settings".
- **Foto profil kontak pada notifikasi** (mis. WhatsApp Web): avatar bulat di toast dan daftar, dengan logo aplikasi sebagai lencana kecil. Gambar diambil dari payload toast di `wpndatabase.db` (dibuka read-only, hanya satu id, hanya berkas lokal) dan dari salinan yang dibuat pemantau folder `Notification Resources` milik Chrome/Edge/Brave, karena browser menghapus berkas gambarnya sesaat setelah toast tampil. Bisa dimatikan di Settings > Display > "Contact photos".
- **Toast tombol kunci**: Caps Lock, Num Lock, dan Scroll Lock menampilkan HUD kecil On/Off di notch (hook keyboard low-level yang hanya terpasang saat fitur aktif; toggle di Settings > Features > "Lock keys"). HUD beruntun menggantikan yang sedang tampil tanpa antre.
- **Indikator mikrofon/kamera**: titik oranye (mic) atau hijau (kamera) di pojok kanan atas notch kecil selama ada aplikasi yang memakainya, plus toast "Microphone/Camera in use" dengan nama aplikasi saat mulai. Dipantau lewat event registry `CapabilityAccessManager` (tanpa polling). Toggle di Settings > Features > "Mic / camera dot".
- **Musik**: scroll mouse di atas baris kontrol slide Music mengatur **volume master** (2% per takik) dengan indikator di dalam slide; jika ada lebih dari satu pemutar media, header "Now Playing · <pemutar>" bisa diklik untuk berpindah sumber.
- **Installer** (`hesewake-notch.iss`, Inno Setup 6) dan `scriptsuild-installer.ps1`: pasang per-pengguna tanpa admin, pilihan autostart, menutup aplikasi yang sedang jalan sebelum memperbarui, dan opsi menghapus data saat uninstall.
- Registri slide (`SlideId`) dan model pengaturan (`settings.txt`: `slides=`, `lyric_lead_ms=`, `busy_on_maximized=`, ...) yang kompatibel dengan berkas lama.

### Changed
- Menu klik kanan disederhanakan: item air (Remove 250 ml, Snooze, Reset, goal harian, interval pengingat) dihapus; semuanya ada di slide Water (tombol Settings dan Snooze).
- **Runtime C/C++ ditanam ke exe** (static CRT): tidak lagi butuh Microsoft Visual C++ Redistributable di PC lain (exe ±1,9 MB).
- Glow mata saat idle memakai palet **biru** (sebelumnya merah), serasi dengan tema.
- Jendela **maximized biasa tidak lagi** membuat notch menyusut jadi garis tipis; hanya layar penuh sungguhan dan aplikasi Adobe. Bisa dikembalikan lewat Settings > Display > "Thin on maximize".
- Pindah slide, titik slide, dan hover lonceng memakai identitas slide (bukan nomor tetap), jadi tetap benar saat slide disembunyikan atau diurutkan ulang.

### Fixed
- Menempatkan kursor dengan klik mouse di kartu Scrap tidak berfungsi karena memeriksa nomor slide yang salah.

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
