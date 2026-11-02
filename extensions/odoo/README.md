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
- **Settings** (a page of the slide, or *Settings > Extensions > Settings*): the refresh interval, toasts on or off, which
  stages are shown, and **Test**, which logs in again and reloads the stages.

## Setup

It uses the Office edition's setup:

- `%APPDATA%\hesewake-notch-office\odoo.json` is copied once to `%APPDATA%\hesewake-notch-odoo\odoo.json`; changes made in
  the extension's settings go to that copy only. The last list (for the monitor) is kept next to it.
- The API key is read from Windows Credential Manager, target `HesewakeNotchOffice/Odoo`. It is never written anywhere else.

## Install

1. Build it with `cargo build --release` in this folder (or take `hesewake-odoo.exe` from the *Odoo extension* workflow).
2. Put the exe somewhere fixed, for example `%LOCALAPPDATA%\Programs\Hesewake Odoo\hesewake-odoo.exe`.
3. Write `%APPDATA%\hesewake-notch\extensions\odoo\extension.json` (with the exe's full path; `\` doubled in JSON):

   ```json
   {
     "id": "odoo",
     "name": "Odoo",
     "version": "0.1.0",
     "exe": "C:\\Users\\me\\AppData\\Local\\Programs\\Hesewake Odoo\\hesewake-odoo.exe",
     "slide": true,
     "settings": "window",
     "processes": "none",
     "proto": 1
   }
   ```

4. In the notch: *Settings > Extensions*, **Rescan**, then switch **Odoo** on.

It talks only to the Odoo server in `odoo.json` (https) and to the notch, and starts no other programs.
