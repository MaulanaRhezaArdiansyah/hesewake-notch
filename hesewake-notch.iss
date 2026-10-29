; Installer Hesewake Notch (Inno Setup 6). Bangun dengan: scripts\build-installer.ps1
; Prasyarat: `cargo build --release` sudah dijalankan (exe di target\release), Inno Setup 6 terpasang.

#define AppName "Hesewake Notch"
#define AppExe "hesewake-notch.exe"
#define VerFull GetVersionNumbersString("target\release\hesewake-notch.exe")
; "0.2.0.0" -> "0.2.0"
#define AppVersion Copy(VerFull, 1, Len(VerFull) - 2)

[Setup]
; GUID tetap agar upgrade menimpa instalasi lama.
AppId={{9774475C-E292-40DD-AD1F-4419C48C82EE}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=dirga
AppComments=Personal notch for the top of your screen. Based on Poch by Commonly Works (MIT).
DefaultDirName={localappdata}\Programs\Hesewake Notch
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=installer_output
OutputBaseFilename=HesewakeNotch-Setup-{#AppVersion}
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile=LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Start {#AppName} when I sign in to Windows"; GroupDescription: "Options:"

[Files]
Source: "target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"

[Registry]
; Nama nilai sama dengan yang dipakai aplikasi (menu "Autostart" di Settings), sehingga keduanya saling sinkron.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "HesewakeNotch"; \
    ValueData: """{app}\{#AppExe}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Remove the Claude Code hooks (if connected) so they don't point at a missing exe.
Filename: "{app}\{#AppExe}"; Parameters: "--claude-disconnect"; Flags: runhidden waituntilterminated; RunOnceId: "ClaudeDisconnect"

[Code]
procedure StopApp();
var
  R: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#AppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, R);
end;

function InitializeSetup(): Boolean;
begin
  StopApp();
  Result := True;
end;

function InitializeUninstall(): Boolean;
begin
  StopApp();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // Data pengguna (to-do, catatan, pengaturan) tidak dihapus kecuali pengguna memilihnya.
  if CurUninstallStep = usPostUninstall then
    if not UninstallSilent then
      if MsgBox('Also delete your Hesewake Notch data (to-do list, notes, water log, settings)?',
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(ExpandConstant('{userappdata}\hesewake-notch'), True, True, True);
end;
