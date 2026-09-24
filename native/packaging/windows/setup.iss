#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "."
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif
[Setup]
AppId={{BDF98A1E-FFF6-47E4-8237-F6347E5329C8}
AppName=ONIX DAC Control
AppVersion={#AppVersion}
AppPublisher=romanio427
AppPublisherURL=https://github.com/romanio427/ONIX-XI1-Control
AppSupportURL=https://github.com/romanio427/ONIX-XI1-Control/issues
AppUpdatesURL=https://github.com/romanio427/ONIX-XI1-Control/releases
DefaultDirName={autopf}\ONIX DAC Control
DefaultGroupName=ONIX DAC Control
DisableProgramGroupPage=yes
LanguageDetectionMethod=uilanguage
UsePreviousLanguage=no
ShowLanguageDialog=yes
OutputDir={#OutputDir}
OutputBaseFilename=ONIX-DAC-Control-windows-x64-setup
SetupIconFile=..\..\ui\icon.ico
UninstallDisplayIcon={app}\onix-xi1-pc.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
RestartIfNeededByRun=no
AppMutex=Local\ONIX_XI1_Control_SingleInstance
VersionInfoVersion={#AppVersion}
VersionInfoProductName=ONIX DAC Control
VersionInfoDescription=ONIX DAC Control Setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "startmenuicon"; Description: "{cm:CreateStartMenuIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\onix-xi1-pc.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-LICENSES.html"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#SourceDir}\LGPL-3.0.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#SourceDir}\SLINT-LICENSE.md"; DestDir: "{app}\licenses"; Flags: ignoreversion

[InstallDelete]
Type: files; Name: "{app}\onix-xi1-watch.exe"

[Icons]
Name: "{group}\ONIX DAC Control"; Filename: "{app}\onix-xi1-pc.exe"; Tasks: startmenuicon
Name: "{autodesktop}\ONIX DAC Control"; Filename: "{app}\onix-xi1-pc.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\onix-xi1-pc.exe"; Parameters: "--init-language {language}"; Flags: runhidden runasoriginaluser
Filename: "{app}\onix-xi1-pc.exe"; Description: "{cm:LaunchApp}"; Flags: postinstall nowait runasoriginaluser skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\onix-xi1-watch.exe"

[CustomMessages]
english.ConfigureAccess=Configuring access to ONIX XI1...
russian.ConfigureAccess=Настройка доступа к ONIX XI1...
english.ConfigureAccessLater=ONIX XI1 control access could not be configured now. Connect the device and open the app to configure access.
russian.ConfigureAccessLater=Сейчас не удалось настроить доступ к управлению ONIX XI1. Подключите устройство и откройте приложение для настройки доступа.
english.LaunchApp=Launch ONIX DAC Control
russian.LaunchApp=Запустить ONIX DAC Control
english.CreateStartMenuIcon=Create a Start menu shortcut
russian.CreateStartMenuIcon=Создать ярлык в меню «Пуск»
english.DeleteDataPrompt=Delete ONIX DAC Control settings and logs for Windows account {username}? This also deletes settings shared with the portable edition.
russian.DeleteDataPrompt=Удалить настройки и журналы ONIX DAC Control для учётной записи Windows «{username}»? Настройки переносной версии также будут удалены.
english.DeleteDataFailed=Could not remove all settings and logs. You can remove them manually from: {localappdata}\ONIX DAC Control
russian.DeleteDataFailed=Не удалось удалить все настройки и журналы. Их можно удалить вручную из: {localappdata}\ONIX DAC Control

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

var
  DeleteUserData: Boolean;

procedure RemoveInstalledStartupValue(const ValueName, ExeName: String);
var
  Command, InstalledExe: String;
begin
  InstalledExe := '"' + ExpandConstant('{app}\' + ExeName) + '"';
  if RegQueryStringValue(HKEY_CURRENT_USER, RunKey, ValueName, Command) and
     (Pos(Lowercase(InstalledExe), Lowercase(Command)) = 1) then
    RegDeleteValue(HKEY_CURRENT_USER, RunKey, ValueName);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
  AppExe: String;
begin
  if CurStep <> ssPostInstall then
    Exit;
  AppExe := ExpandConstant('{app}\onix-xi1-pc.exe');
  if Exec(AppExe, '--device-presence', '', SW_HIDE,
      ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
  begin
    WizardForm.StatusLabel.Caption := CustomMessage('ConfigureAccess');
    if (not Exec(AppExe, '--install-driver-quiet', '', SW_HIDE,
        ewWaitUntilTerminated, ResultCode)) or (ResultCode <> 0) then
      SuppressibleMsgBox(CustomMessage('ConfigureAccessLater'),
        mbInformation, MB_OK, IDOK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    DeleteUserData := DirExists(ExpandConstant('{localappdata}\ONIX DAC Control')) and
      (SuppressibleMsgBox(ExpandConstant(CustomMessage('DeleteDataPrompt')),
        mbConfirmation, MB_YESNO, IDNO) = IDYES);
    Exit;
  end;
  if CurUninstallStep <> usPostUninstall then
    Exit;
  RemoveInstalledStartupValue('ONIX DAC Control', 'onix-xi1-pc.exe');
  RemoveInstalledStartupValue('ONIX XI1 PC Control', 'onix-xi1-pc.exe');
  RemoveInstalledStartupValue('ONIX XI1 Control', 'onix-xi1-pc.exe');
  RemoveInstalledStartupValue('ONIX XI1 Remote', 'onix-xi1-pc.exe');
  RemoveInstalledStartupValue('ONIX XI1 PC Control', 'onix-xi1-watch.exe');
  RemoveInstalledStartupValue('ONIX XI1 Control', 'onix-xi1-watch.exe');
  RemoveInstalledStartupValue('ONIX XI1 Remote', 'onix-xi1-watch.exe');
  if DeleteUserData and
     (not DelTree(ExpandConstant('{localappdata}\ONIX DAC Control'),
       True, True, True)) then
    SuppressibleMsgBox(ExpandConstant(CustomMessage('DeleteDataFailed')),
      mbError, MB_OK, IDOK);
end;
