# UX audit run on a throwaway GitHub Actions Windows runner: drives the real Hesewake Notch 0.13.5 (demo flags only)
# with mouse and keyboard inside the runner, and saves screenshots + logs to $env:OUT.
param([string]$AgentsExe, [string]$OdooExe, [string]$Only = '')
$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class U {
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,int x,int y,int d,IntPtr e);
 [DllImport("user32.dll")] public static extern void keybd_event(byte vk,byte sc,uint f,IntPtr e);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
 public struct RECT { public int L, T, R, B; }
 public static List<IntPtr> Of(uint pid) {
  var r = new List<IntPtr>();
  EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) r.Add(h); return true; }, IntPtr.Zero);
  return r;
 }
}
"@
[U]::SetProcessDPIAware() | Out-Null
$OUT = $env:OUT; New-Item -ItemType Directory -Force $OUT | Out-Null
$LOG = "$OUT\log.txt"
$T0 = [Diagnostics.Stopwatch]::StartNew()
function Note($s) { $l = "{0,7:n1}s  {1}" -f $T0.Elapsed.TotalSeconds, $s; $l; $l | Add-Content $LOG }
function Wait($ms) { Start-Sleep -Milliseconds $ms }
$SW = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width
$SH = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height
$CX = [int]($SW / 2); $NX = $CX - 270
$EXE = "$env:LOCALAPPDATA\Programs\Hesewake Notch\hesewake-notch.exe"
function NP($x, $y) { @{ X = $NX + $x; Y = $y } }
function Go($x, $y) { [U]::SetCursorPos($x, $y) | Out-Null; Wait 15 }
function Glide($x0, $y0, $x1, $y1, $ms) { $n = [math]::Max(2, [int]($ms / 16)); for ($i = 0; $i -le $n; $i++) { Go ([int]($x0 + ($x1 - $x0) * $i / $n)) ([int]($y0 + ($y1 - $y0) * $i / $n)) } }
function Away { Go ($SW - 60) ($SH - 80) }
function Click($x, $y) { Go $x $y; Wait 40; [U]::mouse_event(2,0,0,0,[IntPtr]::Zero); Wait 60; [U]::mouse_event(4,0,0,0,[IntPtr]::Zero); Wait 300 }
function RClick($x, $y) { Go $x $y; Wait 40; [U]::mouse_event(8,0,0,0,[IntPtr]::Zero); Wait 60; [U]::mouse_event(16,0,0,0,[IntPtr]::Zero); Wait 500 }
function Wheel($x, $y, $d) { Go $x $y; [U]::mouse_event(0x800,0,0,$d,[IntPtr]::Zero); Wait 350 }
function Keys([byte[]]$vks) {
  foreach ($v in $vks) { [U]::keybd_event($v,0,0,[IntPtr]::Zero); Wait 40 }
  [array]::Reverse($vks); foreach ($v in $vks) { [U]::keybd_event($v,0,2,[IntPtr]::Zero); Wait 40 }
  Wait 300
}
function CtrlAlt($vk) { Keys @(0x11, 0x12, $vk) }
function Shot($name, $w = 760, $h = 240, $x = -1, $y = 0) {
  if ($x -lt 0) { $x = $CX - [int]($w / 2) }
  try {
    $b = New-Object System.Drawing.Bitmap $w, $h; $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size $w, $h)); $b.Save("$OUT\$name.png"); $g.Dispose(); $b.Dispose()
  } catch { Note "shot $name failed: $_" }
}
function Full($name) { Shot $name $SW $SH 0 0 }
function Burst($name, $n, $gap, $w = 760, $h = 240) { for ($i = 0; $i -lt $n; $i++) { Shot ("{0}-{1:d2}" -f $name, $i) $w $h; Wait $gap } }
function Wins($proc = 'hesewake-notch') {
  foreach ($p in (Get-Process $proc -ErrorAction SilentlyContinue)) {
    foreach ($h in [U]::Of([uint32]$p.Id)) {
      $c = New-Object System.Text.StringBuilder 128; $t = New-Object System.Text.StringBuilder 256
      [U]::GetClassName($h, $c, 128) | Out-Null; [U]::GetWindowText($h, $t, 256) | Out-Null
      $r = New-Object U+RECT; [U]::GetWindowRect($h, [ref]$r) | Out-Null
      [pscustomobject]@{ H = $h; Proc = $proc; Class = $c.ToString(); Title = $t.ToString(); L = $r.L; T = $r.T; W = $r.R - $r.L; Hh = $r.B - $r.T }
    }
  }
}
function WinShot($name, $wnd) {
  try {
    $b = New-Object System.Drawing.Bitmap $wnd.W, $wnd.Hh; $g = [System.Drawing.Graphics]::FromImage($b); $dc = $g.GetHdc()
    [U]::PrintWindow($wnd.H, $dc, 2) | Out-Null; $g.ReleaseHdc($dc); $b.Save("$OUT\$name.png"); $g.Dispose(); $b.Dispose()
  } catch { Note "winshot $name failed: $_" }
}
function SetWin { Wins | Where-Object { $_.Class -ne 'HesewakeNotchWindow' -and $_.W -gt 600 } | Select-Object -First 1 }
function Stop-Notch { Get-Process hesewake-notch, hesewake-agents, hesewake-odoo -ErrorAction SilentlyContinue | Stop-Process -Force; Wait 800 }
function Start-Notch([string[]]$demo, [string[]]$settings) {
  Stop-Notch
  $d = "$env:APPDATA\hesewake-notch"
  if ($null -ne $settings) { New-Item -ItemType Directory -Force $d | Out-Null; [IO.File]::WriteAllLines("$d\settings.txt", [string[]]$settings, (New-Object Text.UTF8Encoding $false)) }
  Start-Process $EXE -ArgumentList $demo | Out-Null
  Note "launch $($demo -join ' ')"
}
# Width of the drawn notch on the top rows, from a 1-px strip (near-black run around the centre).
function NotchW([int]$row = 2) {
  $b = New-Object System.Drawing.Bitmap 1000, 1; $g = [System.Drawing.Graphics]::FromImage($b)
  $g.CopyFromScreen($CX - 500, $row, 0, 0, (New-Object System.Drawing.Size 1000, 1)); $g.Dispose()
  $l = 500; while ($l -gt 0) { $p = $b.GetPixel($l - 1, 0); if ($p.R + $p.G + $p.B -gt 24) { break }; $l-- }
  $r = 500; while ($r -lt 999) { $p = $b.GetPixel($r + 1, 0); if ($p.R + $p.G + $p.B -gt 24) { break }; $r++ }
  $b.Dispose(); $r - $l + 1
}
function NotchH {
  $b = New-Object System.Drawing.Bitmap 1, 260; $g = [System.Drawing.Graphics]::FromImage($b)
  $g.CopyFromScreen($CX - 60, 0, 0, 0, (New-Object System.Drawing.Size 1, 260)); $g.Dispose()
  $h = 0; while ($h -lt 259) { $p = $b.GetPixel(0, $h); if ($p.R + $p.G + $p.B -gt 60 -and $h -gt 4) { break }; $h++ }
  $b.Dispose(); $h
}
function Section($name, [scriptblock]$body) { if ($Only -and -not $Only.Contains($name.Substring(0, 1))) { return }; Note "=== $name"; try { & $body } catch { Note "!! $name failed: $_" }; Away }
function SetPage($y, $name) { $s = SetWin; if (-not $s) { Note "no settings window for $name"; return }; Click ($s.L + 80) ($s.T + $y); Wait 700; WinShot $name (SetWin) }

Note "screen $SW x $SH"
Full 'z-desktop'

Section 'A first run (fresh, no settings)' {
  Away
  Start-Process $EXE -ArgumentList '--demo-notifs' | Out-Null
  Burst 'a1-splash' 18 250
  Burst 'a2-idle' 24 1000
  Note ("idle width " + (NotchW))
  Get-ChildItem "$env:APPDATA\hesewake-notch" | ForEach-Object { Note "data file $($_.Name)" }
  Get-Content "$env:APPDATA\hesewake-notch\settings.txt" -ErrorAction SilentlyContinue | Out-File "$OUT\a-settings-after-first-run.txt"
}

Section 'B hover intent' {
  Start-Notch @('--demo-notifs') $null; Away; Wait 5000
  # time to open: enter the idle hit zone and sample height every 30 ms
  Glide $CX 400 $CX 10 400
  $clk = [Diagnostics.Stopwatch]::StartNew(); $seen = $null
  while ($clk.ElapsedMilliseconds -lt 1500) { $h = NotchH; if (-not $seen -and $h -gt 100) { $seen = $clk.ElapsedMilliseconds }; Wait 20 }
  Note "open: height>100 after $seen ms"
  Shot 'b1-open'
  Go $CX 400
  $clk.Restart(); $gone = $null
  while ($clk.ElapsedMilliseconds -lt 1500) { $h = NotchH; if (-not $gone -and $h -lt 60) { $gone = $clk.ElapsedMilliseconds }; Wait 20 }
  Note "close: height<60 after $gone ms"
  Away; Wait 1500
  foreach ($t in @(@('fast-sweep-y6', 100, 6, ($SW - 100), 6, 500), @('slow-sweep-y6', 100, 6, ($SW - 100), 6, 2000), @('pass-y20', ($CX - 400), 20, ($CX + 400), 20, 700), @('pass-y30', ($CX - 400), 30, ($CX + 400), 30, 700))) {
    $max = 0; $n = [math]::Max(2, [int]($t[5] / 16))
    for ($i = 0; $i -le $n; $i++) { Go ([int]($t[1] + ($t[3] - $t[1]) * $i / $n)) ([int]($t[2] + ($t[4] - $t[2]) * $i / $n)); $h = NotchH; if ($h -gt $max) { $max = $h } }
    Wait 150; $after = NotchH; Shot "b2-$($t[0])"
    Note "$($t[0]): max height during $max, right after $after"
    Away; Wait 1500
  }
  # brief touch: 150 ms in the zone
  Go $CX 10; Wait 150; Go $CX 400; Wait 400; Note "touch 150ms -> height $(NotchH)"; Shot 'b3-touch150'
  Away; Wait 1500
}

Section 'C navigation' {
  Glide $CX 400 $CX 10 300; Wait 1200
  $pNext = NP 498 92; $pPrev = NP 41 92
  for ($i = 0; $i -lt 9; $i++) { Click $pNext.X $pNext.Y; Wait 600; Shot ("c1-next-{0:d2}" -f $i) }
  Click $pPrev.X $pPrev.Y; Wait 600; Shot 'c2-prev'
  $wp = NP 270 30
  for ($i = 0; $i -lt 3; $i++) { Wheel $wp.X $wp.Y -120; Wait 400; Shot "c3-wheel-$i" }
  $d = NP 298 160; Click $d.X $d.Y; Wait 700; Shot 'c4-dot'
  Away; Wait 1500
  foreach ($k in 1..9) { CtrlAlt (0x30 + $k); Wait 700; Shot "c5-ctrlalt$k"; CtrlAlt (0x30 + $k); Wait 900; Note "Ctrl+Alt+$k then again: height $(NotchH)" }
  CtrlAlt 0x48; Wait 900; Shot 'c6-ctrlaltH'; Note "after Ctrl+Alt+H height $(NotchH)"
  CtrlAlt 0x48; Wait 900; Shot 'c6-ctrlaltH-2'
}

Section 'D thin line over a maximized window' {
  $np = Start-Process notepad -PassThru; Wait 1500
  [U]::ShowWindow($np.MainWindowHandle, 3) | Out-Null; [U]::SetForegroundWindow($np.MainWindowHandle) | Out-Null; Wait 1200
  Away; Wait 800; Shot 'd0-max-rest'; Note "max rest width $(NotchW) height $(NotchH)"
  Glide $CX 400 $CX 40 600; Wait 700; Shot 'd1-near40'; Note "near y40 width $(NotchW 1) height $(NotchH)"
  Go $CX 14; Wait 700; Shot 'd2-near14'; Note "y14 height $(NotchH)"
  Go $CX 3; Wait 900; Shot 'd3-aim-line'; Note "aim y3 height $(NotchH)"
  Go ($CX + 200) 3; Wait 900; Shot 'd4-line-off-centre'; Note "y3 x+200 height $(NotchH)"
  Away; Wait 600
  Stop-Process $np -Force
}

Section 'E menu and Settings (EN)' {
  Start-Notch @('--demo-notifs') $null; Wait 4500
  Glide $CX 400 $CX 10 300; Wait 1200
  RClick $CX 60; Wait 600; Shot 'e1-menu' 900 420
  foreach ($i in 1..5) { Keys @(0x28) }; Keys @(0x0D); Wait 1800
  Wins | Format-Table -AutoSize | Out-String | Add-Content $LOG
  $pages = @(@(79,'general'), @(115,'look'), @(151,'slides'), @(187,'screens'), @(223,'music'), @(259,'notifs'), @(295,'alerts'), @(331,'weather'), @(367,'calendar'), @(403,'extensions'), @(529,'about'))
  foreach ($p in $pages) {
    SetPage $p[0] ("e2-set-" + $p[1])
    $s = SetWin; if ($s) { Wheel ($s.L + 500) ($s.T + 330) -360; Wait 500; WinShot ("e3-set-" + $p[1] + "-scrolled") (SetWin) }
  }
  # Calendar: Paste with a non-link in the clipboard, then with http://
  SetPage 367 'e4-cal'
  $s = SetWin
  Set-Clipboard 'hello'; Click ($s.L + 735) ($s.T + 187); Wait 500; WinShot 'e4-cal-paste-text' (SetWin)
  Set-Clipboard 'http://example.com/cal.ics'; Wait 3300; Click ($s.L + 735) ($s.T + 187); Wait 500; WinShot 'e4-cal-paste-http' (SetWin)
  Keys @(0x1B); Wait 800; Note "after Esc settings window: $([bool](SetWin))"
}

Section 'F toasts and hover' {
  Start-Notch @('--demo-toast=water','--demo-notifs') $null; Away; Wait 3600; Shot 'f1-water'
  Glide $CX 400 $CX 25 300; Wait 800; Shot 'f2-water-hovered'; Note "water toast hovered height $(NotchH)"
  Click ($CX + 146) 33; Wait 900; Shot 'f3-after-drank'; Away; Wait 1500; Shot 'f3b-after-drank-away'
  Start-Notch @('--demo-toast=notif','--demo-notifs') $null; Go $CX 10; Wait 3700; Burst 'f4-notif-cursor-parked' 8 300; Note "notif with cursor on notch height $(NotchH)"
  Away; Wait 1200
  Start-Notch @('--demo-toast=notif','--demo-notifs') $null; Away; Wait 3600; Glide $CX 400 $CX 25 250; Wait 600; Shot 'f5-notif-hovered'; Note "notif hovered height $(NotchH)"
  Away; Wait 2000
  Start-Notch @('--demo-notifs') $null; Away; Wait 4500
  Keys @(0x14); Wait 300; Shot 'f6-caps-real'; Keys @(0x14); Wait 300; Shot 'f6-caps-real-off'
  Keys @(0xAF); Wait 300; Shot 'f7-volume-key'; Keys @(0xAE)
  # three quick HUD-type events, then a notification: what survives
  Keys @(0x14); Keys @(0x90); Keys @(0x14); Keys @(0x90); Burst 'f8-hud-burst' 10 200
}

Section 'G playing: width over time and lyrics' {
  Start-Notch @('--demo-playing','--demo-notifs') $null; Away; Wait 4000
  $csv = "$OUT\g-width.csv"; 'ms,width' | Set-Content $csv
  $clk = [Diagnostics.Stopwatch]::StartNew(); $k = 0
  while ($clk.Elapsed.TotalSeconds -lt 90) { "$($clk.ElapsedMilliseconds),$(NotchW)" | Add-Content $csv; if ($k % 15 -eq 0) { Shot ("g1-collapsed-{0:d3}" -f $k) 760 60 }; $k++; Wait 200 }
  Glide $CX 400 $CX 10 300; Wait 1200; Shot 'g2-music-open'
  $lb = NP 413 127; Click $lb.X $lb.Y; Wait 1200; Shot 'g3-lyrics-mode'
  Away; Wait 1500; Shot 'g4-lyrics-after-leave'
}

Section 'H eye and weather states' {
  Start-Notch @('--demo-eye') $null; Away; Wait 3500; Burst 'h1-eye' 30 200
  Start-Notch @('--demo-weather=61,27,1','--demo-prayer=8') $null; Away; Wait 3500; Burst 'h2-rain-prayer' 12 1000
  Start-Notch @('--demo-timer=count','--demo-prayer=9','--demo-weather=3,29,1','--demo-priv=mic') $null; Away; Wait 3500; Burst 'h3-crowded' 14 1000
}

Section 'I Indonesian' {
  Start-Notch @('--demo-notifs') @('lang=1'); Wait 4500
  Glide $CX 400 $CX 10 300; Wait 1200
  $pNext = NP 498 92
  for ($i = 0; $i -lt 8; $i++) { Shot ("i1-slide-{0:d2}" -f $i); Click $pNext.X $pNext.Y; Wait 600 }
  RClick $CX 60; Wait 600; Shot 'i2-menu' 900 420
  foreach ($i in 1..5) { Keys @(0x28) }; Keys @(0x0D); Wait 1800
  foreach ($p in @(@(79,'general'), @(151,'slides'), @(295,'alerts'), @(331,'weather'), @(367,'calendar'), @(403,'extensions'), @(529,'about'))) { SetPage $p[0] ("i3-set-" + $p[1]) }
  Keys @(0x1B)
}

Section 'J extensions (EN)' {
  Start-Notch @('--demo-notifs') @('lang=0'); Wait 4500
  Glide $CX 400 $CX 10 300; Wait 1000; RClick $CX 60; Wait 500
  foreach ($i in 1..5) { Keys @(0x28) }; Keys @(0x0D); Wait 1800
  SetPage 403 'j1-ext-empty'
  # what the installers do while the notch runs
  foreach ($m in @(@('agents','Coding Agents',$AgentsExe,'panel','allow'), @('odoo','Odoo',$OdooExe,'window','none'))) {
    $d = "$env:APPDATA\hesewake-notch\extensions\$($m[0])"; New-Item -ItemType Directory -Force $d | Out-Null
    $j = [ordered]@{ id = $m[0]; name = $m[1]; version = '0.1.0'; exe = $m[2]; slide = $true; settings = $m[3]; processes = $m[4]; proto = 1 }
    ($j | ConvertTo-Json) | Set-Content -Encoding utf8 "$d\extension.json"
  }
  Wait 3000; SetPage 403 'j2-ext-before-rescan'
  $s = SetWin; Click ($s.L + 587) ($s.T + 275); Wait 1200; WinShot 'j3-ext-after-rescan' (SetWin)
  $s = SetWin; Click ($s.L + 745) ($s.T + 187); Wait 3000; WinShot 'j4-ext-first-on' (SetWin)
  $s = SetWin; Click ($s.L + 745) ($s.T + 303); Wait 3000; WinShot 'j5-ext-both-on' (SetWin)
  Get-Process hesewake-agents, hesewake-odoo -ErrorAction SilentlyContinue | ForEach-Object { Note "running $($_.Name)" }
  Keys @(0x1B); Wait 800; Away; Wait 1000
  # Agents slide (8th) and Odoo (9th)
  CtrlAlt 0x38; Wait 1200; Shot 'j6-agents-slide'
  $b = NP 270 118; Click $b.X $b.Y; Wait 2500; Shot 'j7-agents-after-connect'
  $cs = "$env:USERPROFILE\.claude\settings.json"
  Note "claude settings exists after Connect: $(Test-Path $cs)"; if (Test-Path $cs) { Copy-Item $cs "$OUT\j-claude-settings-after-connect.json" }
  Away; Wait 1200
  # events from a long-lived node process, the way Claude Code (node) runs its hooks
  $sim = Start-Process node -ArgumentList "`"$PSScriptRoot\hook-sim.js`"", "`"$AgentsExe`"", "`"$PSScriptRoot\hook-events.json`"" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$OUT\hook-sim.log"
  Note "hook simulator pid $($sim.Id)"
  Burst 'j8-agent-toasts' 40 500
  $sj = "$env:APPDATA\hesewake-notch-agents\sessions.json"; if (Test-Path $sj) { Copy-Item $sj "$OUT\j-sessions.json" } else { Note 'no sessions.json' }
  CtrlAlt 0x38; Wait 1200; Shot 'j9-agents-sessions'
  $r = NP 270 60; Click $r.X $r.Y; Wait 1200; Shot 'j10-agents-row-clicked'
  Away; Wait 1500
  CtrlAlt 0x39; Wait 1200; Shot 'j11-odoo-slide'
  $b = NP 270 118; Click $b.X $b.Y; Wait 2500
  $ow = Wins 'hesewake-odoo' | Where-Object { $_.W -gt 200 } | Select-Object -First 1
  if ($ow) { WinShot 'j12-odoo-setup' $ow; [U]::SetForegroundWindow($ow.H) | Out-Null; Wait 400
    [System.Windows.Forms.SendKeys]::SendWait('https://example.invalid{TAB}demo{TAB}me@example.com{TAB}notakey'); Wait 400; WinShot 'j13-odoo-filled' $ow
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}'); Wait 6000; WinShot 'j14-odoo-after-connect' (Wins 'hesewake-odoo' | Where-Object { $_.W -gt 200 } | Select-Object -First 1)
    Keys @(0x1B); Wait 600
  } else { Note "no odoo setup window"; Full 'j12-no-odoo-window' }
  Away; Wait 1000
  # Remove Coding Agents from the notch's Settings and check what stays in Claude's settings
  Glide $CX 400 $CX 10 300; Wait 1000; RClick $CX 60; Wait 500
  foreach ($i in 1..5) { Keys @(0x28) }; Keys @(0x0D); Wait 1800
  SetPage 403 'j15-ext-status'
  $s = SetWin; Click ($s.L + 483) ($s.T + 251); Wait 400; WinShot 'j16-remove-first-press' (SetWin)
  Click ($s.L + 483) ($s.T + 251); Wait 1500; WinShot 'j17-remove-second-press' (SetWin)
  if (Test-Path $cs) { Copy-Item $cs "$OUT\j-claude-settings-after-notch-remove.json"; Note "hooks left in claude settings after notch Remove: $((Select-String -Path $cs -Pattern 'hesewake-agents' -SimpleMatch).Count) lines" }
  Keys @(0x1B)
}

Stop-Notch
Note "done"
