# 歌词岛 1.2

胶囊：Rust、Win32、Direct2D、DirectWrite、DirectComposition。设置：GPUI 0.2.2（DirectX 后端）。后台：Rust。

默认胶囊宽度为 260px（逻辑像素）；新建设置和恢复紧凑尺寸均采用该值。已有自定义宽度继续保留。

完整解压运行包后打开 `LyricIsland.exe`，从托盘打开设置。“显示与外观”中新增“点击任意区域展开”；默认关闭，开启后可点击收起的岛内任意区域展开。

设置左侧的“更新日志”包含从 0.2.0 独立版开始的历史记录，与随包 `CHANGELOG.md` 使用同一份内容。其中未发布版本与优化分支已单独标注。

中文与西文使用随包完整 Noto Sans SC 变量字体，400、500、600 字重使用字体实际的 wght 轴，无需安装到系统。检测到假名的日文行统一使用随包 Noto Sans JP。两套字体同属 Noto 家族，均随源码提供并附 SIL OFL 1.1 许可。韩文使用可用的系统 Malgun Gothic。高亮与普通文字采用互斥区域绘制，避免重复覆盖抗锯齿边缘。

底部进度按渲染帧推进，小幅同步误差在继续向前播放时修正；暂停、切歌与明确跳转直接显示确认位置。拖动能力仍由播放器的 SMTC 能力决定。

用户设置及缓存位于程序旁的 `Data`，可用 `--data-dir` 指定目录。正常使用时只启动一个主程序；测试可通过专用测试管道隔离实例。

## 设置界面（GPUI）

设置界面由 `lyric-settings` crate 构建，产物仍是 `lyric-island-settings.exe`（发行时重命名为
`LyricIsland.Settings.exe`），使用 GPUI 0.2.2 的 DirectX 渲染后端。
crate 名、产物名与命名管道的线上协议都没有改，启动器、打包脚本与设置管道沿用原约定。
七个页面：显示与外观、动画与反馈、歌词与同步、播放器白名单、关于与许可、更新日志、通知提醒。

- 新增设置键 `SettingsTheme`（`System` 默认、`Light`、`Dark`），由原生侧 `lyric-native/src/config.rs`
  校验并持久化。它只决定设置窗口自己的明暗外观，胶囊仍固定深色；设置窗口的标题栏跟随应用自己的选择，
  不跟随系统设置。
- 展开卡片字体颜色、展开卡片组件颜色、歌词进度填色改用内嵌取色器：饱和度-明度面板、色相条、预设色与
  实时十六进制值，取代原来的十六进制文本框。
- 播放位置校准、自定义播放器名单、额外通知应用名单是单行输入框（按 Enter 提交）。输入框实现了 gpui 的
  `EntityInputHandler`，**支持输入法组字**：组字串带下划线显示、提交后落地，因此也能用输入法输入非 ASCII
  自由文本（普通字符与组字都经平台回调进入，不由按键猜测）。已实现点击定位光标、拖动选择、Shift+方向键
  选择、`Ctrl+A/C/X/V` 与 Home/End、退格/删除；双击选词、三击选行与撤销/重做不在范围内。已实测：用
  微软拼音真实组字时拼音串被 marked 文本跟踪、并在输入法 Enter 上屏进字段；另用 `WM_DEADCHAR` 写入
  marked 片段后由下一个字符正确替换，再注入含非 BMP emoji 的 `测试🎵` 并回车提交，落盘值逐个码点核对
  通过。日文、韩文输入法未测。三处的提交都已接通：设置端会把后两者改写成 `update` +
  `AllowedSources` / `NotificationAllowedApps` 再发给原生端，所以原生端只会看到 `update`，不需要
  （也没有）对应的命令分支。自定义播放器一项已用合成键盘输入实测：输入 `testplayer.exe` 回车后，
  `settings.json` 的 `AllowedSources` 中确实出现了该项。
- GPUI 以 Apache-2.0 授权，归属见 `THIRD-PARTY-NOTICES.md`，许可全文见 `licenses/GPUI-Apache-2.0.txt`。

## 动画与构建

展开、收起与中途反向操作使用连续弹簧动画，保留当前的位置与速度。弹簧数值已完成对照验证，文字、模糊和阴影的视觉表现仍需持续核对。

`build/native.ps1` 构建胶囊及设置。构建设置 EXE 还需要 Windows SDK 的 `fxc.exe`（GPUI 在 release 构建里编译 HLSL 着色器）、一个 C 编译器（本机使用 `zig cc` 包装，供少量传递依赖编译 C 代码），以及 `build/ktmw32/libktmw32.a` 导入库（与已有的 `build/shlwapi/libshlwapi.a` 并列，GNU sysroot 不提供）。脚本接受 `-DllTool`、`-FxcPath`、`-ZigCc` 参数，也可加 `-Offline` 只用本地缓存。

## 自检

```
LyricIsland.exe --font-check font-check.json
LyricIsland.exe --media-info media-info.json
LyricIsland.exe --animation-preview frames
LyricIsland.exe --demo --expanded --run-ms 10000
```

字体自检记录实际 GlyphRun 文件路径和字重轴；媒体自检只读能力，不改变播放。

## Windows 通知

通知总开关默认关闭，“免证书兼容模式”默认开启，“暂停通知”默认关闭，通知白名单默认不启用。

1. 将便携包完整解压，或运行单文件 EXE。在设置或托盘开启通知即可。默认兼容模式使用目标电脑当前用户的通知存储，只读接收启用后的新通知，不需要安装证书、注册身份包或设置系统权限。
2. QQNT 与旧 QQ 默认加入白名单。其他应用使用完整 AUMID 加入白名单。通知必须实际发送至 Windows 通知中心；应用自己的聊天弹窗不在接收范围内。
3. 可选启用通知白名单。关闭时接收各应用的新通知；开启后按已选应用过滤，原名单会保留。
4. 设置页和托盘的暂停状态同步，暂停时收起横幅，恢复后不补弹暂停期间的消息。已移除跟随 Windows 勿扰模式选项。
5. 兼容模式依赖 Windows 内部数据库结构。在其他系统版本、权限受限或数据库被占用时可能暂不可用，界面会提示并重试；本机通过不代表所有电脑已经验证。不会修改数据库、恢复删除内容或记录/上传通知正文。
6. 若兼容模式不可用，可关闭它改用系统接口。该模式仍需通知身份和 Windows 授权：使用一键启用会准备当前开发证书信任与身份组件，首次可能需要管理员确认，随后自动重启申请访问。可运行 notification-support/Uninstall-Notifications.ps1 撤销可选组件。默认兼容模式完全不执行这些安装步骤。

接收器与音乐界面共用原生绘制链，运行包包含 SQLite 组件并按当前用户路径定位通知存储。分享时请使用发行 EXE 或完整便携 ZIP；不要只发送主程序。系统接口可选组件未注册也不影响默认兼容模式。

日文字体来自 Google Fonts 的 Noto Sans JP 原始变量 TTF（9,589,900 字节），保持文件完整，随包附 SIL Open Font License 1.1 全文于 licenses/NotoSansJP-OFL.txt。


## 1.1 更新说明

通知横幅按正文的实际 DirectWrite 排版在 64、80、96 逻辑像素之间选择高度，减少底部留白。来源图标首先读取 Windows Shell 登记的应用图标，读取失败时尝试系统通知提供的应用标志；不从网上下载或手绘 QQ 图标。

GPUI 设置的通信在后台执行。短暂断连后会自动重试并清除过期的连接错误；业务输入验证错误仍保留到修正。设置 EXE 的应用图标由 build/embed_icon.py 在打包时写入，不修改字体与图形渲染框架。

运行 1.1 前请退出旧版及其设置窗口。单文件启动器首次启动 1.1 时会迁移 1.0 的 settings.json 与 database 文件夹，保留旧版原文件。
