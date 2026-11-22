; Installateur Inno Setup 6 pour ParkinsonAide.
; Compiler avec installer\build.ps1 (compile l'app en release puis ce script),
; ou directement : ISCC.exe /DAppVersion=0.1.0 installer\ParkinsonAide.iss

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppName "ParkinsonAide"
#define AppExe  "ParkinsonAide.exe"
#define Root    ".."

[Setup]
; Ne jamais changer AppId : il identifie l'application pour les mises à jour/désinstallation.
AppId={{9FCB147F-E403-4A19-BB16-5CCB4D3589CA}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=ParkinsonAide
; Installation pour l'utilisateur courant : pas besoin de droits administrateur,
; et le dossier reste accessible en écriture.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Root}\target\installer
OutputBaseFilename={#AppName}-Setup-{#AppVersion}
SetupIconFile=ParkinsonAide.ico
UninstallDisplayIcon={app}\ParkinsonAide.ico
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Dirs]
Name: "{userdocs}\{#AppName}"

[Files]
; Application (runtime C lié statiquement, voir .cargo\config.toml)
Source: "{#Root}\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "ParkinsonAide.ico"; DestDir: "{app}"; Flags: ignoreversion
; Bibliothèque Vosk (reconnaissance vocale) et ses dépendances MinGW
Source: "{#Root}\vosk\*.dll"; DestDir: "{app}"; Flags: ignoreversion
; Petit modèle français Vosk (détecté automatiquement à côté de l'exe)
Source: "{#Root}\vosk_small_fr\*"; DestDir: "{app}\vosk_small_fr"; Flags: ignoreversion recursesubdirs createallsubdirs
; Phrases d'exemple dans Documents (modifiables), sans écraser celles de l'utilisateur
Source: "{#Root}\phrases.txt"; DestDir: "{userdocs}\{#AppName}"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "{#Root}\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{userdocs}\{#AppName}"; IconFilename: "{app}\ParkinsonAide.ico"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{userdocs}\{#AppName}"; IconFilename: "{app}\ParkinsonAide.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; WorkingDir: "{userdocs}\{#AppName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
