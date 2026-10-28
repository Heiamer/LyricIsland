param([string]$PlayerPath)
$ErrorActionPreference='Stop'
try {
 $running=@(Get-CimInstance Win32_Process -Filter "Name='cloudmusic.exe'")
 $listeners=@(Get-NetTCPConnection -State Listen -LocalPort 9223 -ErrorAction SilentlyContinue)
 if($running.Count -gt 0){
  $valid=$listeners.Count -eq 1 -and $listeners[0].LocalAddress -eq '127.0.0.1' -and $running.ProcessId -contains $listeners[0].OwningProcess
  if($valid){Write-Host '网易云本机静默控制已开启，可以直接使用点屿进度条。';exit 0}
  throw '网易云正在运行，但未开启静默通道。请先从网易云托盘菜单完全退出，再运行此脚本；脚本不会自动结束播放器。'
 }
 if($listeners.Count -gt 0){throw '本机 9223 端口已被占用，未启动播放器。'}
 if(-not $PlayerPath){
  $candidates=@("${env:ProgramFiles(x86)}\NetEase\CloudMusic\cloudmusic.exe","$env:ProgramFiles\NetEase\CloudMusic\cloudmusic.exe","$env:LOCALAPPDATA\NetEase\CloudMusic\cloudmusic.exe")
  $PlayerPath=$candidates | Where-Object {Test-Path -LiteralPath $_ -PathType Leaf} | Select-Object -First 1
 }
 if(-not $PlayerPath){throw '未找到网易云安装位置。可通过 -PlayerPath 参数指定 cloudmusic.exe。'}
 $player=Get-Item -LiteralPath $PlayerPath
 if($player.Name -ine 'cloudmusic.exe' -or -not (Test-Path -LiteralPath (Join-Path $player.DirectoryName 'cloudmusic.dll'))){throw '播放器路径不符合网易云客户端格式。'}
 $process=Start-Process -FilePath $player.FullName -ArgumentList '--remote-debugging-address=127.0.0.1','--remote-debugging-port=9223' -WorkingDirectory $player.DirectoryName -PassThru
 $ready=$false
 for($attempt=0;$attempt -lt 30;$attempt++){
  Start-Sleep -Milliseconds 250
  $listeners=@(Get-NetTCPConnection -State Listen -LocalPort 9223 -ErrorAction SilentlyContinue)
  if($listeners.Count -gt 0){
   if($listeners.Count -ne 1 -or $listeners[0].LocalAddress -ne '127.0.0.1' -or $listeners[0].OwningProcess -ne $process.Id){throw '未确认到当前网易云的仅本机监听。请退出本次播放器，勿将调试端口开放到局域网。'}
   $ready=$true;break
  }
 }
 if(-not $ready){throw '网易云未建立本机控制通道，可能不支持此启动参数。'}
 Write-Host '网易云本机静默控制已开启。打开点屿后，展开卡片、概览和音乐页共用此通道。'
 Write-Host '完全退出网易云后，需再次使用此启动脚本。不会修改安装文件或自启动设置。'
}catch{Write-Host $_.Exception.Message -ForegroundColor Red;exit 1}
