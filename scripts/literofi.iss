; LiteRofi Windows Installer (Inno Setup 6)
; Builds a standalone, zero-dependency Windows wizard installer with EN (default) + TR languages.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef StageDir
  #define StageDir "..\dist\_inno_stage"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{8E5F2C3A-9D4B-4F1E-B2A7-6C3D9E1F4A82}
AppName=LiteRofi
AppVersion={#AppVersion}
AppVerName=LiteRofi {#AppVersion} (Rofi on Windows)
AppPublisher=LiteRofi
AppPublisherURL=https://github.com/merb2y/LiteRofi
DefaultDirName={localappdata}\LiteRofi
DefaultGroupName=LiteRofi
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#OutputDir}
OutputBaseFilename=LiteRofi-{#AppVersion}-win64-Setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\literofi.exe
CloseApplications=yes
RestartApplications=no
SetupLogging=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[CustomMessages]
english.TypeFull=Full Installation (LiteRofi + Theme Studio + All Themes)
english.TypeCompact=LiteRofi Core Only
english.TypeCustom=Custom Installation
english.CompCore=LiteRofi Application Launcher (Alt+Space) & Music Widget
english.CompStudio=LiteRofi Theme Studio (Visual Theme & Color Picker)
english.CompAssets=Themes & Color Palettes Bundle (206 Themes + 406 Palettes)
english.TaskGroupStartup=Startup & Shortcuts:
english.TaskGroupSystem=System:
english.TaskAutostart=Start LiteRofi automatically with Windows (Alt+Space)
english.TaskDesktopStudio=Create a Desktop shortcut for LiteRofi Theme Studio
english.TaskAddPath=Add LiteRofi to user PATH
english.IconUninstall=Uninstall LiteRofi
english.RunDaemon=Start LiteRofi background service now (Alt+Space)
english.RunStudio=Launch LiteRofi Theme Studio now

turkish.TypeFull=Tam Kurulum (LiteRofi + Theme Studio + Tüm Temalar)
turkish.TypeCompact=Sadece LiteRofi
turkish.TypeCustom=Özel Kurulum
turkish.CompCore=LiteRofi Uygulama Başlatıcı (Alt+Space) ve Müzik Kontrolü
turkish.CompStudio=LiteRofi Theme Studio (Görsel Tema & Renk Seçici)
turkish.CompAssets=Tema ve Renk Paleti Paketi (206 Tema + 406 Palet)
turkish.TaskGroupStartup=Başlangıç ve Kısayollar:
turkish.TaskGroupSystem=Sistem:
turkish.TaskAutostart=Windows açılışında LiteRofi'yi otomatik başlat (Alt+Space)
turkish.TaskDesktopStudio=Masaüstüne LiteRofi Theme Studio kısayolu oluştur
turkish.TaskAddPath=LiteRofi'yi kullanıcı PATH ortam değişkenine ekle
turkish.IconUninstall=LiteRofi Kaldır
turkish.RunDaemon=LiteRofi arka plan servisini şimdi başlat (Alt+Space)
turkish.RunStudio=LiteRofi Theme Studio'yu şimdi aç

[Types]
Name: "full"; Description: "{cm:TypeFull}"
Name: "compact"; Description: "{cm:TypeCompact}"
Name: "custom"; Description: "{cm:TypeCustom}"; Flags: iscustom

[Components]
Name: "core"; Description: "{cm:CompCore}"; Types: full compact custom; Flags: fixed
Name: "studio"; Description: "{cm:CompStudio}"; Types: full
Name: "assets"; Description: "{cm:CompAssets}"; Types: full

[Tasks]
Name: "autostart"; Description: "{cm:TaskAutostart}"; GroupDescription: "{cm:TaskGroupStartup}"; Flags: checkedonce
Name: "desktopstudio"; Description: "{cm:TaskDesktopStudio}"; GroupDescription: "{cm:TaskGroupStartup}"; Components: studio; Flags: checkedonce
Name: "addpath"; Description: "{cm:TaskAddPath}"; GroupDescription: "{cm:TaskGroupSystem}"; Flags: checkedonce

[Files]
; Core binaries & docs
Source: "{#StageDir}\literofi.exe"; DestDir: "{app}"; Components: core; Flags: ignoreversion
Source: "{#StageDir}\README.md"; DestDir: "{app}"; Components: core; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#StageDir}\LICENSE"; DestDir: "{app}"; Components: core; Flags: ignoreversion skipifsourcedoesntexist

; Optional Theme Studio native binary + custom icon (zero dependencies)
Source: "{#StageDir}\literofi-studio.exe"; DestDir: "{app}"; Components: studio; Flags: ignoreversion
Source: "{#StageDir}\studio.ico"; DestDir: "{app}"; Components: studio; Flags: ignoreversion skipifsourcedoesntexist

; Themes, color palettes, images, launchers, wallust (preserve existing user edits)
Source: "{#StageDir}\assets\themes\*"; DestDir: "{userappdata}\LiteRofi\themes"; Components: assets; Flags: recursesubdirs createallsubdirs onlyifdoesntexist skipifsourcedoesntexist
Source: "{#StageDir}\assets\colors\*"; DestDir: "{userappdata}\LiteRofi\colors"; Components: assets; Flags: recursesubdirs createallsubdirs onlyifdoesntexist skipifsourcedoesntexist
Source: "{#StageDir}\assets\images\*"; DestDir: "{userappdata}\LiteRofi\images"; Components: assets; Flags: recursesubdirs createallsubdirs onlyifdoesntexist skipifsourcedoesntexist
Source: "{#StageDir}\assets\launchers\*"; DestDir: "{userappdata}\LiteRofi\launchers"; Components: assets; Flags: recursesubdirs createallsubdirs onlyifdoesntexist skipifsourcedoesntexist
Source: "{#StageDir}\assets\wallust\*"; DestDir: "{userappdata}\LiteRofi\wallust"; Components: assets; Flags: recursesubdirs createallsubdirs onlyifdoesntexist skipifsourcedoesntexist

[Icons]
Name: "{group}\LiteRofi"; Filename: "{app}\literofi.exe"; Parameters: "-show drun"; IconFilename: "{app}\literofi.exe"
Name: "{group}\LiteRofi Theme Studio"; Filename: "{app}\literofi-studio.exe"; IconFilename: "{app}\studio.ico"; Components: studio
Name: "{group}\LiteRofi Theme Selector"; Filename: "{app}\literofi.exe"; Parameters: "-show theme"; IconFilename: "{app}\literofi.exe"
Name: "{group}\LiteRofi Color Palette"; Filename: "{app}\literofi.exe"; Parameters: "-show colors"; IconFilename: "{app}\literofi.exe"
Name: "{group}\LiteRofi Music"; Filename: "{app}\literofi.exe"; Parameters: "-show music -theme music-player"; IconFilename: "{app}\literofi.exe"
Name: "{group}\LiteRofi Settings"; Filename: "{app}\literofi.exe"; Parameters: "--settings"; IconFilename: "{app}\literofi.exe"
Name: "{group}\LiteRofi Themes Folder"; Filename: "{app}\literofi.exe"; Parameters: "--themes-dir"; IconFilename: "{app}\literofi.exe"
Name: "{group}\{cm:IconUninstall}"; Filename: "{uninstallexe}"

; Desktop shortcut for Theme Studio (optional task)
Name: "{userdesktop}\LiteRofi Theme Studio"; Filename: "{app}\literofi-studio.exe"; IconFilename: "{app}\studio.ico"; Components: studio; Tasks: desktopstudio

; Windows Startup shortcut for background daemon (optional task)
Name: "{userstartup}\LiteRofi Daemon"; Filename: "{app}\literofi.exe"; Parameters: "--start-daemon"; IconFilename: "{app}\literofi.exe"; Tasks: autostart

[Run]
Filename: "{app}\literofi.exe"; Parameters: "--start-daemon"; Description: "{cm:RunDaemon}"; Flags: nowait postinstall runhidden
Filename: "{app}\literofi-studio.exe"; Description: "{cm:RunStudio}"; Components: studio; Flags: nowait postinstall skipifsilent

[Code]
procedure StopRunningProcesses();
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/F /IM literofi.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/F /IM literofi-studio.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(250);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopRunningProcesses();
  Result := '';
end;

procedure EnsureStarterConfig();
var
  ConfigDir, ConfigPath, Content: String;
begin
  ConfigDir := ExpandConstant('{userappdata}\LiteRofi');
  ForceDirectories(ConfigDir);
  ForceDirectories(ConfigDir + '\themes');
  ConfigPath := ConfigDir + '\config.rasi';
  if not FileExists(ConfigPath) then
  begin
    Content :=
      'configuration {' + #13#10 +
      '    modes: "drun,run,window,combi,filebrowser";' + #13#10 +
      '    show-icons: true;' + #13#10 +
      '    font: "Segoe UI 11";' + #13#10 +
      '    drun-display-format: "{name}";' + #13#10 +
      '    daemon-hotkey: "Alt+Space";' + #13#10 +
      '    daemon-mode: "drun";' + #13#10 +
      '}' + #13#10 + #13#10 +
      '@theme "dracula_config2"' + #13#10;
    SaveStringToFile(ConfigPath, Content, False);
  end;
end;

procedure AddInstallDirToUserPath();
var
  CurrentPath, InstallDir: String;
begin
  InstallDir := ExpandConstant('{app}');
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', CurrentPath) then
    CurrentPath := '';
  if Pos(LowerCase(InstallDir), LowerCase(CurrentPath)) = 0 then
  begin
    if (CurrentPath <> '') and (CurrentPath[Length(CurrentPath)] <> ';') then
      CurrentPath := CurrentPath + ';';
    CurrentPath := CurrentPath + InstallDir;
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', CurrentPath);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    EnsureStarterConfig();
    if WizardIsTaskSelected('addpath') then
      AddInstallDirToUserPath();
  end;
end;

function InitializeUninstall(): Boolean;
begin
  StopRunningProcesses();
  Result := True;
end;
