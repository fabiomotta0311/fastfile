; FastFile — Inno Setup Script (Issue #18)
; Gera o instalador standalone FastFile-Setup.exe para Windows

#define MyAppName "FastFile"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Fabio Motta"
#define MyAppURL "https://github.com/fabiomotta0311/fastfile"
#define MyAppExeName "ff-ui.exe"

[Setup]
AppId={{C6B500D1-6F88-4F23-9BE2-2BE3A5CFD704}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE-APACHE
OutputDir=..\target\installer
OutputBaseFilename=FastFile-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ChangesEnvironment=yes

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "contextmenu"; Description: "Adicionar ao menu de contexto do Windows Explorer"; GroupDescription: "Integrações:"
Name: "addtopath"; Description: "Adicionar ao PATH do sistema (permite comandos 'ff' e 'fastfile')"; GroupDescription: "Integrações:"

[Files]
Source: "..\target\release\ff.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\fastfile.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\ff-ui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\ff-indexer.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\ff.exe"; Parameters: "register-context-menu"; Tasks: contextmenu; Flags: runhidden
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\ff.exe"; Parameters: "unregister-context-menu"; Flags: runhidden
