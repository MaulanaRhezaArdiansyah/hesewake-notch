; Installer for the Coding Agents extension of Hesewake Notch (Inno Setup 6). Built by the "Agents extension" workflow
; after `cargo build --release` in this folder: ISCC installer.iss
; It puts the exe in place and writes the extension's manifest for the notch; nothing runs as administrator.
; Installing never touches Claude Code's settings: the person presses Connect on the slide. Uninstalling takes our
; hooks out again (`--disconnect-all`).

#define AppName "Coding Agents for Hesewake Notch"
#define AppExe "hesewake-agents.exe"
#define VerFull GetVersionNumbersString("target\release\hesewake-agents.exe")
; "0.1.0.0" -> "0.1.0"
#define AppVersion Copy(VerFull, 1, Len(VerFull) - 2)
#define ExtDir "{userappdata}\hesewake-notch\extensions\agents"

[Setup]
; Fixed, so an upgrade replaces the earlier install.
AppId={{7DE7B0B9-D97B-47B1-8B63-4BC414B3040B}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Hesewake Notch contributors
AppComments=Claude Code sessions on Hesewake Notch, as an extension.
DefaultDirName={localappdata}\Programs\Hesewake Agents
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=installer_output
OutputBaseFilename=HesewakeAgents-Setup-{#AppVersion}
SetupIconFile=..\..\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile=..\..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
FinishedLabel=The Coding Agents extension is installed.%n%nIn Hesewake Notch (0.13.0 or later): Settings > Extensions, Rescan, switch Coding Agents on. Then press Connect on its slide to link Claude Code.%n%nAn upgrade needs none of this: the notch starts the new version by itself.

[Files]
Source: "target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion

[UninstallDelete]
Type: files; Name: "{#ExtDir}\extension.json"
Type: dirifempty; Name: "{#ExtDir}"

[Code]
procedure StopExtension();
var
  R: Integer;
begin
  // the notch starts it again a few seconds later, from the new exe
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#AppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, R);
end;

function InitializeSetup(): Boolean;
begin
  StopExtension();
  Result := True;
end;

function InitializeUninstall(): Boolean;
var
  R: Integer;
begin
  StopExtension();
  // take our hooks out of Claude Code's settings while the exe is still there; everything else in the file stays
  Exec(ExpandConstant('{app}\{#AppExe}'), '--disconnect-all', '', SW_HIDE, ewWaitUntilTerminated, R);
  Result := True;
end;

// The manifest the notch reads (docs/extensions.md); JSON wants every \ in the path doubled.
procedure WriteManifest();
var
  Exe: String;
  Lines: TArrayOfString;
begin
  Exe := ExpandConstant('{app}\{#AppExe}');
  StringChangeEx(Exe, '\', '\\', True);
  ForceDirectories(ExpandConstant('{#ExtDir}'));
  SetArrayLength(Lines, 10);
  Lines[0] := '{';
  Lines[1] := '  "id": "agents",';
  Lines[2] := '  "name": "Coding Agents",';
  Lines[3] := '  "version": "{#AppVersion}",';
  Lines[4] := '  "exe": "' + Exe + '",';
  Lines[5] := '  "slide": true,';
  Lines[6] := '  "settings": "panel",';
  Lines[7] := '  "processes": "allow",';
  Lines[8] := '  "proto": 1';
  Lines[9] := '}';
  if not SaveStringsToUTF8File(ExpandConstant('{#ExtDir}\extension.json'), Lines, False) then
    MsgBox('Could not write the extension''s manifest in ' + ExpandConstant('{#ExtDir}') + '.', mbError, MB_OK);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    WriteManifest();
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // the data stays unless asked: a reinstall then shows the same sessions and options
  if CurUninstallStep = usPostUninstall then
    if not UninstallSilent then
      if MsgBox('Also delete the Coding Agents data (session list and options)?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(ExpandConstant('{userappdata}\hesewake-notch-agents'), True, True, True);
end;
