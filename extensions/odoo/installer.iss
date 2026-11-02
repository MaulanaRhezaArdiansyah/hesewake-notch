; Installer for the Odoo extension of Hesewake Notch (Inno Setup 6). Built by the "Odoo extension" workflow after
; `cargo build --release` in this folder: ISCC installer.iss
; It puts the exe in place and writes the extension's manifest for the notch; nothing runs as administrator.

#define AppName "Odoo for Hesewake Notch"
#define AppExe "hesewake-odoo.exe"
#define VerFull GetVersionNumbersString("target\release\hesewake-odoo.exe")
; "0.2.0.0" -> "0.2.0"
#define AppVersion Copy(VerFull, 1, Len(VerFull) - 2)
#define ExtDir "{userappdata}\hesewake-notch\extensions\odoo"

[Setup]
; Fixed, so an upgrade replaces the earlier install.
AppId={{2CDF1D02-4B76-4E40-9600-617656A28102}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Hesewake Notch contributors
AppComments=Odoo tickets on Hesewake Notch, as an extension.
DefaultDirName={localappdata}\Programs\Hesewake Odoo
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=installer_output
OutputBaseFilename=HesewakeOdoo-Setup-{#AppVersion}
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
FinishedLabel=The Odoo extension is installed.%n%nIn Hesewake Notch (0.13.0 or later): open Settings > Extensions, press Rescan and switch Odoo on. Then press Set up on the Odoo slide and enter your server, login and API key.%n%nAn upgrade needs none of this: the notch starts the new version by itself.

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
begin
  StopExtension();
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
  Lines[1] := '  "id": "odoo",';
  Lines[2] := '  "name": "Odoo",';
  Lines[3] := '  "version": "{#AppVersion}",';
  Lines[4] := '  "exe": "' + Exe + '",';
  Lines[5] := '  "slide": true,';
  Lines[6] := '  "settings": "window",';
  Lines[7] := '  "processes": "none",';
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
var
  R: Integer;
begin
  // the settings and the key stay unless asked: a reinstall then works at once
  if CurUninstallStep = usPostUninstall then
    if not UninstallSilent then
      if MsgBox('Also delete your Odoo settings and the saved API key?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      begin
        DelTree(ExpandConstant('{userappdata}\hesewake-notch-odoo'), True, True, True);
        Exec(ExpandConstant('{sys}\cmdkey.exe'), '/delete:HesewakeOdoo/Odoo', '', SW_HIDE, ewWaitUntilTerminated, R);
      end;
end;
