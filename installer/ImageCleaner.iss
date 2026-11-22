; Image Cleaner - Inno Setup 스크립트 (SPDX-License-Identifier: GPL-3.0-or-later)
; 빌드: installer\build.ps1  (또는 iscc /DAppVersion=1.0.0 installer\ImageCleaner.iss)
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#define AppName "Image Cleaner"
#define AppExe "ImageCleaner.exe"

[Setup]
AppId={{B7E2C1A4-5D3F-4A89-9C61-2F0E8A7D4B15}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Image Cleaner
AppPublisherURL=https://github.com/designloves2/image-cleaner
AppSupportURL=https://github.com/designloves2/image-cleaner/issues
AppUpdatesURL=https://github.com/designloves2/image-cleaner/releases
DefaultDirName={autopf}\Image Cleaner
DefaultGroupName=Image Cleaner
DisableProgramGroupPage=yes
; 사용자 단위 설치: 관리자 권한 없이 설치/자동 업데이트 가능 (원하면 설치 시 관리자 모드 선택)
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=ImageCleaner-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
#if FileExists(CompilerPath + "Languages\Korean.isl")
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
#endif

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\image_cleaner.exe"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
; 일반 설치: 마지막 페이지의 "실행" 체크박스
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; 자동 업데이트(조용한 설치) 후에는 앱을 다시 실행
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: WizardSilent
