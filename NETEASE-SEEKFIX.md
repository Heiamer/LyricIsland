# 当前状态 · 3.1.6

### 已知问题 · 网易云进度

网易云后台进度调节尚未彻底修复，仍可能引起任务栏闪烁或主界面出现。当前版本保留自动激活保护、短时注意力清除和有界重试，部分场景验收通过，但用户仍报告异常；不能视为已解决。按本次发布决定暂缓继续处理，后续单独修复。

---

# 网易云无感进度控制 · DotIsle 3.1.4

## 使用

正常启动网易云音乐并播放歌曲，再启动点屿，进度会自动读取并可拖动。无需重启网易云、运行脚本、安装插件或开放调试端口。网易云在后台、最小化及托盘隐藏时均可使用。

本机验收客户端为官方 Windows 3.1.42.205594，Windows 11、150% DPI。后台控件路径会校验 orpheus.ntpk 资源的 SHA256；其他资源版本尚未验证，会拒绝此路径而不是冒险点击。已存在的本机 CDP 通道仍可作为旧版兼容路径，不会自动重启播放器来开启通道。

## 实现与边界

通过只读原生时间轴获取当前歌曲、时长、位置及播放状态；进度提交只发往该进程主窗口的 CEF 渲染控件，不使用系统鼠标、键盘或前台激活。最小化时 CEF 控件会收缩成 0×0；点屿只临时恢复其子控件尺寸，随后还原，主窗口始终保持最小化或隐藏。若用户中途还原主窗口，清理代码让出控制，不用旧的 0×0 覆盖用户界面。

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

## 3.1.6：任务栏闪烁回归

旧路径虽然没有调用前台激活 API，CEF 处理后台鼠标按下时仍会请求激活，Windows 因而触发任务栏注意力闪烁。新路径仅对验证过的 Chrome_WidgetWin_0 子窗口短时设置 WS_EX_NOACTIVATE，先用带超时的空消息拒绝无响应控件，再投递成对按下/抬起，并由作用域清理恢复该属性。不会禁用窗口，不改变网易云主窗口属性，也不关闭系统任务栏提醒。拒绝无响应或层级变化的控件。验收新增 SHELLHOOK / HSHELL_FLASH 监听，分别检查跳转确认、任务栏注意力、前台与窗口状态。

Microsoft SetFocus / SetForegroundWindow / WS_EX_NOACTIVATE 文档：
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setfocus
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow
- https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles

最小化时 CEF 即使收到 NOACTIVATE 保护仍会请求主窗口注意力。本版另以独立消息线程监听系统 SHELLHOOK，只在已验证的当前进度操作开始后 600 ms 内，针对相同进程与同一主窗口的 HSHELL_FLASH 立即调用 FLASHW_STOP。该线程平时等待消息，不轮询播放器或系统偏好；不会关闭系统提醒，也不会禁用或修改主窗口。回归记录保留注意力请求与清除重绘两个事件，验证每次清除确认在 16 ms 内，不能将“请求被及时清除”写成“播放器从未发出请求”。

最小化控件首次重建增加短暂就绪等待。若投递后 2.6 秒内没有真实时间轴确认，只对网易云后台路径重试一次；重试前继续检查歌曲、进程与命令代次，切歌或有更新拖动时取消旧请求。不会重试其他播放器的媒体命令。
