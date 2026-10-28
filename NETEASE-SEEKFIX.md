# 网易云无感进度控制 · DotIsle 3.1.4

## 使用

正常启动网易云音乐并播放歌曲，再启动点屿，进度会自动读取并可拖动。无需重启网易云、运行脚本、安装插件或开放调试端口。网易云在后台、最小化及托盘隐藏时均可使用。

本机验收客户端为官方 Windows 3.1.42.205594，Windows 11、150% DPI。后台控件路径会校验 orpheus.ntpk 资源的 SHA256；其他资源版本尚未验证，会拒绝此路径而不是冒险点击。已存在的本机 CDP 通道仍可作为旧版兼容路径，不会自动重启播放器来开启通道。

## 实现与边界

通过只读原生时间轴获取当前歌曲、时长、位置及播放状态；进度提交只投递到该进程主窗口的 CEF 渲染控件，不使用系统鼠标、键盘或前台激活。最小化时 CEF 控件会收缩成 0×0；点屿只临时恢复其子控件尺寸，随后还原，主窗口始终保持最小化或隐藏。若用户中途还原主窗口，清理代码让出控制，不用旧的 0×0 覆盖用户界面。

歌曲切换、异常尺寸、多个主窗口或控件、客户端更新及超时均会拒绝或取消操作。请求串行并合并待发拖动，使用新的真实时间轴样本确认进度。长音频受原播放器进度条的像素精度约束；不会把本地计时当作真实跳转成功。

系统媒体读取独立运行，不会阻塞网易云原生时间轴。进度模块不启动或结束播放器、不修改播放器安装包、不写进程内存、不持有全局鼠标捕获。

## 调研来源

- Microsoft PostMessageW：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew
- Microsoft SetWindowPos（SWP_ASYNCWINDOWPOS、SWP_NOACTIVATE）：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos
- Microsoft ShowWindowAsync（分析旧版同步显示风险）：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindowasync
- AF-Media-Bar（只读原生时间轴参考）：https://github.com/Fervent-Tempo/AF-Media-Bar
- MediaDuet（分析协议路径）：https://github.com/TIANLI0/MediaDuet
- cloudmusic-desktop-mcp（CDP 路径比较）：https://github.com/Seraph310/cloudmusic-desktop-mcp

后台 CEF 子控件路径为本次针对本机官方客户端验证的新实现。协议命令会触发 app/activeWindow，故不作为无感路径；原重启与重播实现已从本版移除。
