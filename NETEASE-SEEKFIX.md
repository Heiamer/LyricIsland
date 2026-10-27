# 网易云静默进度适配

2026-10-27 · DotIsle 3.1.2 正式版，包含网易云静默进度控制。

## 使用

退出旧点屿，将本包完整解压到新的可写目录，运行 LyricIsland.exe。迁移时先备份，再复制旧目录 Data；不要让两个版本写入同一目录。

网易云需要以本机控制通道启动：完全退出网易云后，双击 Start-NetEase-SilentControl.cmd，再正常播放歌曲。若客户端已开启该通道，脚本只报告已连接，不重复启动；普通方式启动的客户端不会被脚本自动结束。自定义安装路径可在 PowerShell 中运行 .\Start-NetEase-SilentControl.ps1 -PlayerPath 'D:\安装目录\cloudmusic.exe'。

展开音乐卡片、控制中心概览和音乐页共用静默通道。网易云完全退出后，下次仍需通过该脚本启动；脚本不修改安装文件、快捷方式或自启动设置。正常打开播放器会按其自身行为显示窗口，进度拖动不再调用激活入口。

客户端已通过脚本开启本机控制通道时，无需重复重启。

## 修复原因与实现

原 webcmd / orpheus 协议在网易云前端同时触发 app/activeWindow，因此仅隐藏启动器控制台仍会弹出播放器。

新路径通过 Windows 原生 WinHTTP HTTP/WebSocket 访问 127.0.0.1:9223，核对端口所属进程和网易云页面，沿现有 React Provider 读取播放 store，以绝对秒数调度 playing/setPlayingPosition。只读取当前播放字段，不安装 webpack hook、不注入 DLL、不写播放器内存、不修改安装资源；没有 Node 常驻进程，也不会每次拖动启动脚本或外部程序。

当前歌曲、播放实例或页面不匹配时取消；加载中、恢复中、视频及无有效实例时拒绝发送。进度真正改变仍由网易云原生时间轴回读确认。请求有时限和大小限制，队列合并连续待发拖动，切歌取消旧请求。失败不会回退到会弹窗的协议，不会强行隐藏播放器或恢复焦点来掩盖弹窗。

通道只允许 IPv4 127.0.0.1，不开放局域网；启动脚本与后端均检查监听。CDP 是较强的本机调试接口，同机其他程序也可能访问；关闭网易云即可关闭本次通道。系统媒体控制仍供支持该能力的其他播放器使用。

## 真实验收

网易云版本 3.1.42.205594。一次已授权的生产 Backend 验收：播放中约 161.609 秒跳至请求 173.609 秒，原生回读约 173.729 秒；随后恢复到约 161.731 秒（含测试期间正常播放时间），原生回读约 162.011 秒，两次均 confirmed，播放状态保持。

WinEvent 观察窗口显示、还原和前台切换，同时轮询前台窗口。主窗口测试前后均保持最小化；显示/还原事件 0，网易云前台激活事件 0，前台窗口句柄不变。没有鼠标、键盘、隐藏窗口或抢回焦点操作。

这是本机本版本普通在线歌曲的一次实测；未覆盖其他版本、暂停歌曲、本地文件、电台或实体鼠标拖动。两种 UI 的绝对秒数输入和隔离 IPC 另行回归。此次未重测内存或冷/热启动，不能据此宣称达到 100 MB。

## 研究来源

- cloudmusic-desktop-mcp 的本机 CEF 方案：https://github.com/Seraph310/cloudmusic-desktop-mcp
- pi-assistant 的网易云研究：https://github.com/Passer1072/pi-assistant/blob/9472a58974a90ced8c5d1093f22097b63695c65f/packages/desktop-assistant/mcp-servers/netease-music/RESEARCH.md
- Dynamic island 的有界 CDP 控制：https://github.com/alent17/Wind0ws_Dynamic_island/blob/a6aa5b203ea1a7565e97b8853dcaf1bcadcbf74d/src-tauri/src/services/netease_cdp.rs
- 微软 WinHTTP WebSocket API：https://learn.microsoft.com/windows/win32/api/winhttp/nf-winhttp-winhttpwebsocketreceive
- 本机官方客户端 orpheus.ntpk 的 webmessage / activeWindow 与 playing/setPlayingPosition 分支（仅研究，不随包分发）。

生产适配器为独立实现，未复制或安装这些项目的插件或客户端。
