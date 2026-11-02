# Odoo extension

Odoo tickets in Hesewake Notch, as an extension ([docs/extensions.md](../../docs/extensions.md)). It does what the Office
edition's Tickets slide did:

- **List**: my tickets in the stages that are on, with a chip per stage. A dot marks tickets that are new or changed stage
  since the slide was last looked at.
- **Ticket page**: stage, project, due date, created, attachments, the description and the last three comments, with
  **Open** (Odoo in the browser) and **Copy**.
- **Report**: what I worked on that day (my comments, comments of the automatic accounts, stage changes) and today's queue,
  with a day stepper and **Copy** (the same text as the Office edition).
- **Monitor**: the list is refreshed every 1, 5, 15, 30 or 60 minutes; new tickets and stage changes come as a toast and a dot
  on the slide's navigation dot.
- **Setup window**: server address, database, login and API key. **Connect** logs in first and saves only what works.
- **Settings** (a page of the slide): **Account** (the setup window), the refresh interval, toasts on or off, which stages
  are shown, and **Test**, which logs in again and reloads the stages.

## Setup

The first time, the slide shows **Set up** (also *Settings > Extensions > Settings*, or **Account** on the slide's Settings
page). It opens a small window:

- **Server address**: `https://` and the Odoo address, for example `https://company.odoo.com`.
- **Database**: the Odoo database name.
- **Login**: the email you log in to Odoo with.
- **API key**: made in Odoo under *your profile > Preferences > Account Security > New API Key*. It is kept in Windows
  Credential Manager (target `HesewakeOdoo/Odoo`) and never written anywhere else. Left empty, the saved key is kept.

**Connect** logs in, finds your user and the stages, and only then saves. The stages are all shown at first (except ones
named like "done"); hide the ones that are not yours on the slide's Settings page.

Settings are in `%APPDATA%\hesewake-notch-odoo\odoo.json`, next to the last list (for the monitor). An Office edition's
setup is picked up as it is: its `odoo.json` is copied there once, and its key (`HesewakeNotchOffice/Odoo`) is read when
this extension has none of its own.

## Install

Needs Hesewake Notch 0.13.0 or later.

1. Run `HesewakeOdoo-Setup-x.y.z.exe` (made by the *Odoo extension* workflow). It installs for this Windows user only, in
   `%LOCALAPPDATA%\Programs\Hesewake Odoo`, and writes the notch's manifest
   (`%APPDATA%\hesewake-notch\extensions\odoo\extension.json`).
2. In the notch: *Settings > Extensions*, **Rescan**, then switch **Odoo** on.
3. On the Odoo slide, press **Set up** (see *Setup* above).

An upgrade is the same installer: it stops the running extension and the notch starts the new one by itself. Uninstalling
(*Settings > Apps*) removes the manifest too, and asks whether to delete the settings and the saved key.

Without the installer: build with `cargo build --release`, put the exe somewhere fixed and write the manifest yourself
(the exe's full path, `\` doubled in JSON):

```json
{
  "id": "odoo",
  "name": "Odoo",
  "version": "0.2.0",
  "exe": "C:\\Users\\me\\AppData\\Local\\Programs\\Hesewake Odoo\\hesewake-odoo.exe",
  "slide": true,
  "settings": "window",
  "processes": "none",
  "proto": 1
}
```

It talks only to the Odoo server that was set up (https) and to the notch, and starts no other programs.
