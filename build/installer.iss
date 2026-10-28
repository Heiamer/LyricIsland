#ifndef AppVersion
  #define AppVersion "3.1.3"
#endif
#ifndef AppId
  #define AppId "DotIsle"
#endif
#ifndef OutputBase
  #define OutputBase "DotIsle-" + AppVersion + "-Setup"
#endif
[Setup]
AppId={#AppId}
AppName=点屿 DotIsle
AppVersion={#AppVersion}
AppPublisher=DotIsle
DefaultDirName={localappdata}\Programs\DotIsle
DefaultGroupName=DotIsle
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
DisableWelcomePage=no
DisableDirPage=no
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\LyricIsland.exe
UninstallDisplayName=点屿 DotIsle
SetupIconFile=..\Assets\LyricIsland-v3.ico
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBase}
Compression=lzma2/fast
SolidCompression=yes
CloseApplications=yes
CloseApplicationsFilter=LyricIsland.exe,LyricIsland.Settings.exe,LyricIsland.Updater.exe
RestartApplications=no
SetupLogging=yes

[Languages]
Name: chinese; MessagesFile: "ChineseSimplified.isl"

[Tasks]
Name: desktopicon; Description: "创建桌面快捷方式"; Flags: unchecked

[Files]
; unins000.* keeps Inno's own uninstaller out of the copy. It must NOT be written as unins*:
; Inno matches Excludes case-insensitively, so unins* also swallows Uninstall.exe - the Rust
; maintenance program the uninstaller copies to {tmp} before it runs. Without it every uninstall
; stops with "uninstall maintenance program missing".
Source: "{#StageDir}\*"; DestDir: "{app}"; Excludes: "DotIsle.Setup.exe,OtaInstall.json,Portable.json,Installed.json,Data\*,Logs\*,unins000.*"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#StageDir}\Uninstall.exe"; DestDir: "{tmp}"; DestName: "DotIsle.Maintenance.exe"; Flags: dontcopy
Source: "{#EngineFile}"; DestDir: "{tmp}"; DestName: "DotIsle.Verify.exe"; Flags: dontcopy

[Icons]
Name: "{group}\点屿"; Filename: "{app}\LyricIsland.exe"
Name: "{group}\卸载点屿"; Filename: "{uninstallexe}"
Name: "{autodesktop}\点屿"; Filename: "{app}\LyricIsland.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\LyricIsland.exe"; Description: "启动点屿"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\Installed.json"

[Code]
var
  RemoveUserData: Boolean;
  CleanupExe: String;

function Quoted(const Value: String): String;
begin
  Result := '"' + Value + '"';
end;

procedure Maintenance(const ExeName, Arguments: String);
var ExitCode: Integer;
begin
  if not Exec(ExeName, Arguments, '', SW_HIDE, ewWaitUntilTerminated, ExitCode) then
    RaiseException('无法启动点屿安装维护程序。');
  if ExitCode <> 0 then
    RaiseException('点屿安装维护失败，错误码 ' + IntToStr(ExitCode) + '。数据未被擅自清除，请查看安装日志。');
end;

function InitializeSetup: Boolean;
var VerifyDir: String; ExitCode: Integer;
begin
  Result := True;
  VerifyDir := ExpandConstant('{param:VERIFYDIR|}');
  if VerifyDir <> '' then begin
    ExtractTemporaryFile('DotIsle.Verify.exe');
    Exec(ExpandConstant('{tmp}\DotIsle.Verify.exe'), '--verify-package ' + Quoted(VerifyDir), '', SW_HIDE, ewWaitUntilTerminated, ExitCode);
    Result := False;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var Args, Migration: String;
begin
  Result := '';
  ExtractTemporaryFile('DotIsle.Maintenance.exe');
  Args := '--inno-prepare --install-dir ' + Quoted(ExpandConstant('{app}')) + ' --record ' + Quoted(ExpandConstant('{tmp}\DotIsle.Migration.json'));
  Migration := ExpandConstant('{param:MIGRATEDATA|}');
  if Migration <> '' then Args := Args + ' --migrate-data ' + Quoted(Migration);
  try
    Maintenance(ExpandConstant('{tmp}\DotIsle.Maintenance.exe'), Args);
  except
    Result := GetExceptionMessage;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var Args, DataDir: String;
begin
  if CurStep = ssPostInstall then begin
    Args := '--inno-finalize --install-dir ' + Quoted(ExpandConstant('{app}')) + ' --record ' + Quoted(ExpandConstant('{tmp}\DotIsle.Migration.json')) + ' --registry-key ' + Quoted('Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppId}_is1');
    DataDir := ExpandConstant('{param:DATADIR|}');
    if DataDir <> '' then Args := Args + ' --data-dir ' + Quoted(DataDir);
    Maintenance(ExpandConstant('{app}\Uninstall.exe'), Args);
  end;
end;

function InitializeUninstall: Boolean;
var Form: TSetupForm; Keep: TNewCheckBox; Info: TNewStaticText; ConfirmButton, CancelButton: TNewButton;
begin
  RemoveUserData := ExpandConstant('{param:REMOVEDATA|0}') = '1';
  Result := True;
  if not UninstallSilent then begin
    Form := CreateCustomForm(430, 180, False, False);
    try
      Form.Caption := '卸载点屿';
      Form.ClientWidth := ScaleX(430); Form.ClientHeight := ScaleY(180);
      Info := TNewStaticText.Create(Form); Info.Parent := Form;
      Info.SetBounds(ScaleX(24), ScaleY(20), ScaleX(380), ScaleY(48));
      Info.Caption := '卸载点屿程序。你可以选择是否保留设置、待办、流程、历史记录及缓存。'; Info.WordWrap := True;
      Keep := TNewCheckBox.Create(Form); Keep.Parent := Form;
      Keep.SetBounds(ScaleX(24), ScaleY(82), ScaleX(380), ScaleY(24));
      Keep.Caption := '保留用户数据（取消勾选将一并清除）'; Keep.Checked := True;
      ConfirmButton := TNewButton.Create(Form); ConfirmButton.Parent := Form;
      ConfirmButton.SetBounds(ScaleX(238), ScaleY(132), ScaleX(80), ScaleY(28));
      ConfirmButton.Caption := '卸载'; ConfirmButton.ModalResult := mrOk; ConfirmButton.Default := True;
      CancelButton := TNewButton.Create(Form); CancelButton.Parent := Form;
      CancelButton.SetBounds(ScaleX(328), ScaleY(132), ScaleX(80), ScaleY(28));
      CancelButton.Caption := '取消'; CancelButton.ModalResult := mrCancel; CancelButton.Cancel := True;
      Result := Form.ShowModal = mrOk;
      RemoveUserData := not Keep.Checked;
    finally
      Form.Free;
    end;
  end;
  if Result then begin
    CleanupExe := ExpandConstant('{tmp}\DotIsle.Cleanup.exe');
    if not CopyFile(ExpandConstant('{app}\Uninstall.exe'), CleanupExe, False) then begin
      MsgBox('卸载维护程序缺失，请重新安装点屿后卸载。', mbError, MB_OK); Result := False;
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Args: String;
begin
  if CurUninstallStep = usUninstall then begin
    Args := '--inno-cleanup --install-dir ' + Quoted(ExpandConstant('{app}'));
    if RemoveUserData then Args := Args + ' --remove-data';
    Maintenance(CleanupExe, Args);
  end;
end;
