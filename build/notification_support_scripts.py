"""Generate explicit install/uninstall steps for the signed development build."""

def write_scripts(support, identity, thumbprint):
    trust=r'''param([ValidateSet('Add','Remove')][string]$Action='Add')
$ErrorActionPreference='Stop'
$CertificateFile=Join-Path $PSScriptRoot 'LyricIslandPreview.cer'
$CertificateObject=New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertificateFile)
if ($CertificateObject.Thumbprint -ne '__THUMBPRINT__') { throw '证书与本版本不匹配。' }
$MachinePath='Cert:\LocalMachine\TrustedPeople\__THUMBPRINT__'
if ($Action -eq 'Add') {
 Import-Certificate -FilePath $CertificateFile -CertStoreLocation Cert:\LocalMachine\TrustedPeople -ErrorAction Stop | Out-Null
} elseif (Test-Path -LiteralPath $MachinePath) {
 Remove-Item -LiteralPath $MachinePath -ErrorAction Stop
}
'''
    install=r'''param([switch]$ConfirmInstall)
$ErrorActionPreference='Stop'
$NativeBase=Split-Path $PSScriptRoot
$CertificateFile=Join-Path $PSScriptRoot 'LyricIslandPreview.cer'
$CertificateObject=New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertificateFile)
if ($CertificateObject.Thumbprint -ne '__THUMBPRINT__') { throw '证书与本版本不匹配。' }
if (-not $ConfirmInstall) {
 Write-Host '本预览版需要管理员确认，将本版公钥证书加入本地计算机的“受信任人”，然后只为当前用户注册通知身份。'
 Write-Host '不加入根证书。系统通知读取仍受 Windows 权限控制。'
 Write-Host ('程序目录：'+$NativeBase)
 if ((Read-Host '输入 Y 确认安装，其他输入取消') -ne 'Y') { return }
}
if (-not (Test-Path -LiteralPath 'Cert:\LocalMachine\TrustedPeople\__THUMBPRINT__')) {
 $TrustScript=Join-Path $PSScriptRoot 'Certificate-Trust.ps1'
 $TrustProcess=Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList ('-NoProfile -ExecutionPolicy Bypass -File "'+$TrustScript+'" -Action Add') -Wait -PassThru -ErrorAction Stop
 if ($TrustProcess.ExitCode -ne 0) { throw '证书信任未完成，通知身份尚未注册。' }
}
# The deployment service may not read an install source inside a protected
# user profile. Public Documents provides a readable, temporary source only.
$Stage=Join-Path ([Environment]::GetFolderPath('CommonDocuments')) ('LyricIsland-Notification-'+[Guid]::NewGuid().ToString('N'))
$StagePackage=Join-Path $Stage 'Identity.msix'
$Marker=Join-Path $PSScriptRoot 'identity-enabled'
try {
 New-Item -ItemType Directory -Path $Stage -ErrorAction Stop | Out-Null
 Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LyricIsland.NotificationIdentity.msix') -Destination $StagePackage -ErrorAction Stop
 Add-AppxPackage -Path $StagePackage -ExternalLocation $NativeBase -ErrorAction Stop
 if (-not $?) { throw 'Windows 未完成通知身份注册。' }
 if (-not (Get-AppxPackage -Name '__IDENTITY__' -ErrorAction Stop)) { throw '未找到刚注册的通知身份。' }
 Set-Content -LiteralPath $Marker -Value '__IDENTITY__' -Encoding Ascii -ErrorAction Stop
} catch {
 if (Test-Path -LiteralPath $Marker) { Remove-Item -LiteralPath $Marker -ErrorAction SilentlyContinue }
 throw
} finally {
 if (Test-Path -LiteralPath $StagePackage) { Remove-Item -LiteralPath $StagePackage -ErrorAction SilentlyContinue }
 if (Test-Path -LiteralPath $Stage) { Remove-Item -LiteralPath $Stage -ErrorAction SilentlyContinue }
}
Write-Host '通知支持已注册。请重新启动本目录的歌词岛，在设置中开启通知提醒，并检查系统通知访问权限。'
'''
    uninstall=r'''param([switch]$ConfirmUninstall)
$ErrorActionPreference='Stop'
if (-not $ConfirmUninstall) {
 Write-Host '将移除当前用户的歌词岛通知身份，并通过管理员确认移除本版开发证书。'
 if ((Read-Host '输入 Y 确认移除，其他输入取消') -ne 'Y') { return }
}
Get-AppxPackage -Name '__IDENTITY__' -ErrorAction Stop | Remove-AppxPackage -ErrorAction Stop
if (-not $?) { throw '通知身份移除失败。' }
$Marker=Join-Path $PSScriptRoot 'identity-enabled'
if (Test-Path -LiteralPath $Marker) { Remove-Item -LiteralPath $Marker -ErrorAction Stop }
if (Test-Path -LiteralPath 'Cert:\LocalMachine\TrustedPeople\__THUMBPRINT__') {
 $TrustScript=Join-Path $PSScriptRoot 'Certificate-Trust.ps1'
 $TrustProcess=Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList ('-NoProfile -ExecutionPolicy Bypass -File "'+$TrustScript+'" -Action Remove') -Wait -PassThru -ErrorAction Stop
 if ($TrustProcess.ExitCode -ne 0) { throw '通知身份已移除；开发证书尚未移除。' }
}
$UserCertificate='Cert:\CurrentUser\TrustedPeople\__THUMBPRINT__'
if (Test-Path -LiteralPath $UserCertificate) { Remove-Item -LiteralPath $UserCertificate -ErrorAction Stop }
Write-Host '通知支持与本版开发证书已移除，便携音乐显示仍可使用。'
'''
    setup=r'''param([Parameter(Mandatory=$true)][string]$ResultFile)
$ErrorActionPreference='Stop'
try {
 & (Join-Path $PSScriptRoot 'Install-Notifications.ps1') -ConfirmInstall
 @{success=$true;message='通知组件已准备好'} | ConvertTo-Json | Set-Content -LiteralPath $ResultFile -Encoding UTF8
 exit 0
} catch {
 @{success=$false;message=('通知准备失败：'+$_.Exception.Message)} | ConvertTo-Json | Set-Content -LiteralPath $ResultFile -Encoding UTF8
 exit 1
}
'''
    for name,script in [('Certificate-Trust.ps1',trust),('Install-Notifications.ps1',install),('Uninstall-Notifications.ps1',uninstall),('Setup-Notifications.ps1',setup)]:
        (support/name).write_text(script.replace('__THUMBPRINT__',thumbprint).replace('__IDENTITY__',identity),encoding='utf-8-sig')
