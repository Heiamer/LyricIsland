# 1.0 验证

1.0 是架构迁移版：胶囊保持 WPF，设置改为 Rust 前端，非 GUI 后台全部使用 Rust，WinUI 与 Windows App SDK
移出发行树。设置前端在 1.0 是 Slint，原生预览版 `Native-Preview-20261015r2` 起换成 GPUI 0.2.2
（Windows 上使用 DirectX 渲染后端）。**按用户要求不做体积与内存对比，本版不含任何性能对比附件或测量结论。**
下方先列本次设置前端更换的验证状态，其后为历史验证记录。

## 设置界面改为 GPUI（Native-Preview-20261015r2）

本节只记录原生预览版设置前端的验证状态；再往下的表格是 1.0（Slint 前端）当时的记录。

| 项目 | 结果 |
| --- | --- |
| 设置 crate 构建 | `build\native.ps1` 在 `-Offline` 下 `cargo test -p lyric-native -p lyric-settings` 通过，随后两个 `--release` 可执行文件构建成功；crate 名 `lyric-settings` 与产物名 `lyric-island-settings` 未变 |
| 不再使用 Slint | `lyric-settings` 的依赖只有 gpui 0.2.2、`lyric-backend`、`serde_json`、`raw-window-handle`、`windows`；`src\build.rs` 只嵌入激活清单（`embed-manifest`），Slint 的 `ui/settings.slint` 与整个 `ui\` 目录已从源码树移除，源码树内不再有任何 crate 引用 Slint |
| 取色器 | 由本次改动者用合成鼠标输入驱动饱和度-明度面板与色相条，写入 `settings.json` 的十六进制值与手算结果一致 |
| 滑块 | 由本次改动者用合成鼠标输入拖动，写入 `settings.json` 的数值与手算结果一致 |
| 明暗外观 | `SettingsTheme` 取 `System` / `Light` / `Dark`（默认 `System`），由 `lyric-native/src/config.rs` 校验并持久化；设置窗口标题栏跟随应用自己的选择，不跟随系统设置 |
| 单行输入框 | 三处（播放位置校准、自定义播放器名单、额外通知应用名单）均已接通。设置端把后两者改写成 `update` + `AllowedSources` / `NotificationAllowedApps` 后发出，原生端只需处理 `update`。自定义播放器一项由本次改动者用合成键盘输入实测：输入 `testplayer.exe` 回车后，`settings.json` 的 `AllowedSources` 中出现该项 |
| 输入法组字 | **已实现并实测**：`text_field.rs` 实现 gpui 的 `EntityInputHandler`——`replace_and_mark_text_in_range` 承载组字（组字段落按下划线绘制）、`replace_text_in_range` 承载提交，`marked_text_range` / `unmark_text` 同步组字状态，因此可输入非 ASCII 自由文本；普通字符也走 `WM_CHAR` 的同一条提交路径，不由按键猜测。同时实现点击定位光标、拖动选择、Shift+方向键选择、`Ctrl+A/C/X/V`、Home/End、退格/删除、Enter 提交（组字中 Enter 归输入法）；双击选词、三击选行、撤销/重做不在范围内。实测证据：①用微软拼音真实组字，拼音串被 marked 文本跟踪、并在输入法 Enter 上屏进字段；②`WM_DEADCHAR` 写入 marked 片段 "´" 后，下一个字符**替换**该片段（非追加）；③`SendInput` 注入含非 BMP emoji 的 `测试🎵` 后回车提交，落盘值为 `ae测试🎵`、逐个码点核对（U+0061 U+0065 U+6D4B U+8BD5 U+1F3B5，**不含** U+00B4），证明 marked range 语义与 UTF-16↔UTF-8 换算正确。日文、韩文输入法未测 |
| 便携 ZIP | 已实际组包并通过校验，产物在 `2026-10-15\w\outputs\gpui-settings\` 的 `LyricIsland-Native-Preview-20261015r2-*`；SHA256 见同目录 `LyricIsland-Native-Preview-20261015r2-SHA256.txt`（此处不内嵌哈希，以免重打包后失效）。用户提供的 r2 输入包未被覆盖 |
| 组包自校验 | 用分享 EXE 执行 `--verify-package` 解包后 `verification.json` = `{"success":true,...,"gui":"Direct2D","settingsUi":"GPUI","backend":"Rust"}`；便携包内含 `LyricIsland.Settings.exe`、`licenses/GPUI-Apache-2.0.txt`，且不再包含 Slint 许可 |
| 组包环境 | `build\notification_identity.py` 需要 Python 的 `cryptography`（本机两个解释器都没有）。已装到工作目录本地（`work\gpui-settings\pydeps`，未改动任何全局 site-packages），并从上一版便携包中恢复**已签名**的 `notification-support\`，因此走复用签名路径：不注册身份、不改动证书存储、不附带私钥 |

未列出的行为不作已验证声明。

## 已实测通过 · 1.0（Slint 设置前端；证据文件在 `work/qa08/`、`work/qa10-final/`、`work/qa10-probe/`）

| 项目 | 结果 |
| --- | --- |
| Slint 设置编译链接 | `cargo build -p lyric-settings` 成功；`lyric-island-settings.exe` 13,676,544 字节 |
| Slint 五页渲染 | `--settings-test` 退出码 0，产出 5 张 PNG 与 `report.json`（`{"ipc":"Rust","pages":5,"renderer":"femtovg","settings":"Slint","success":true}`） |
| Slint 归属展示 | 关于页实际渲染官方 `AboutSlint`（`#MadeWithSlint` / `Version 1.18.1` / `https://slint.dev/`） |
| 完整字体 | `OPPOSans.ttf` 22,741,096 字节，SHA256 `6C7D5864…84A9F4`，与 0.7.8.3 一致 |
| WPF 编译 | 0 警告 0 错误 |
| 胶囊 GUI 回归 | **35/35 通过**（含 `playingIndicatorMoves`、`capsuleMorphsContinuously`、`contentBlurDecays`、`reversalContinuous`、`interruptionSettles`、`flushWithTopEdge`、`noGreenFocusBorder`、`topmost`、`singleMaterial`、几何 140×40 / 340×200） |
| FFI 加载与后台自述 | `{"version":"1.0","crateVersion":"1.0.0","compiler":"rustc","abi":1}` |
| 媒体存在状态 | 出现 / 暂停隐藏 / 恢复 / 隐藏 / 重现 / 竞态取消 / 手动隐藏 全部通过（合成快照驱动状态机） |
| **真实播放曲目全链路** | 只读探针直连 `lyric_ffi.dll` 读取真实 SMTC 会话（见下节）：曲目、进度、时长、封面、歌词、逐字进度、翻译全部正确 |
| 设置管道 | 真实管道往返成功（上面五页渲染即由其驱动）；`ipc.rs` 的连接丢弃原因会写入 `%APPDATA%\LyricIsland\Logs`（类别 `IPC_DROP`，仅失败时记录） |
| 运行时 HTTP 直连 | 在已被 `curl` 证明有效的死端口代理下（`NO_PROXY` 清空）仍 `metadataDirect`/`coverDirect`/`lyricsDirect` 全 true，`httpBackend = "WinHTTP NO_PROXY"`；用户 Windows 代理未被改动 |
| Rust 单元测试 | `cargo test --release --workspace` **140 项通过、0 失败**（core 111 / ffi 18 / backend 10 / cli 1），含开机注册事务与回滚 |
| 托管后端回归 | `LyricIsland.Tests` 在 `-p:UseRustBackend=false` 下 **124 项通过、0 失败** |
| 发行运行库 | 用导入表确认 `lyric-island-settings.exe` 与 `lyric_ffi.dll` 均不依赖 GNU 运行库；发行不携带 `libgcc_s_seh-1.dll` / `libwinpthread-1.dll` |

### 真实播放曲目（只读探针）

用一个临时探针（**只调用 `li_backend_create` / `li_backend_snapshot` / `li_backend_destroy`，
不导入 `li_backend_command`**，因此全程不控制播放器）对正在播放的网易云曲目取样，发行版 `lyric_ffi.dll`
（SHA256 `4776A6D1…65224`）实测：

| 字段 | 值 |
| --- | --- |
| 曲目 / 歌手 / songId | `I Trust You` / `高橋李依` / `2662238263` |
| isPlaying / 进度 / 时长 | `true` / `158.54s`（同比上一秒 157.53s 递增）/ `255.40s` |
| 时长来源 / 原生状态 | 网易云实时读取 / 网易云实时进度 |
| 歌词来源 | `QQMusic`（网易云无此曲，多提供商回退成功） |
| 当前 / 下一句 | `もう怖いものはない` / `この世界にあなたがいるから`，行号 22 |
| 行内进度 / 逐字进度 | `0.0301` / `0.0378` |
| 翻译 | `让我再没有任何畏惧`，`hasTrackTranslation = true` |
| 封面 | 首次快照返回 125,104 字符 base64（真实封面字节） |


## 修复的缺陷

- **在线歌词搜索从未成功（严重，用户实测发现）**：`runtime.rs` 为每家 provider 各建一个 coordinator 且不传
  信任策略，`create_default` 要求覆盖完整在线顺序，单源会以 `MissingProvider` 失败，再被
  `unwrap_or_else(|_| unavailable())` 吞掉。诊断日志 `LYRIC_MISS` 实证：四家 provider 全部 `status:Unavailable`、
  无一家超时。后果是只有旧版 0.7.8.3 写入缓存的歌词能显示。已改为传入单 provider 信任策略；
  修复后同一真实曲目 `lyricSource=Netease`、歌词行与行号正确（`护航`，`currentLineIndex=25`）。
  回归测试：`runtime::tests::one_source_coordinator_requires_a_matching_trust_policy`。
- **托盘右键菜单位置错误（用户实测发现）**：父进程 per-monitor DPI 感知，传的是物理像素；设置进程原先无感知，
  `TrackPopupMenuEx` 按逻辑像素解释，在 150% 缩放的 2560×1600 屏上落点偏移到 1.5 倍、托盘在右侧时跑到屏外。
  已在 `--tray-menu` 分支创建菜单前声明 per-monitor DPI 感知。
  测试：`shell::tests::tray_menu_process_becomes_per_monitor_dpi_aware`。
- **设置管道响应被丢弃**：写响应后立即 `DisconnectNamedPipe` 会丢弃客户端尚未读走的缓冲数据，
  客户端因此拿到 `ERROR_PIPE_NOT_CONNECTED (233)`。已改为写后先 `FlushFileBuffers` 再断开。
  原版 0.7.8.3 用 `NamedPipeServerStream` + `AutoFlush` 靠关闭句柄收尾，没有这个竞态，是迁移时引入的。
- **`e_sqlite3.dll` 未随发行复制**：新架构下 `LyricIsland.App` 不再引用 `TaskbarLyrics.Core`，
  而 Rust 后台用 `LoadLibraryW("e_sqlite3.dll")` 读 `song_maps.db`，缺失会让歌曲映射静默失效。
  现在由 `build/merge_runtime.py` 从 `SQLitePCLRaw.lib.e_sqlite3` 2.1.6 复制并断言存在。
- **托管后端测试无法编译**：`MainWindow` 的 AutoStart 调用已换原生签名，旧分支缺少匹配重载；
  另需在命令行为整个构建（含还原）指定 `-p:UseRustBackend=false`，否则 App 的还原图仍是生产图、缺 EF Core。

### 真实切歌与进度连续性（只读探针，被动观察）

同一个只读探针连续采样，用户自然切歌、未做任何干预：

- `最後祝福（剧集《好心作怪》片尾曲）` 解析出歌词后，行号随播放自然推进 **24 → 48**，
  每行停留 2–27 秒，与歌词时序吻合；`lineProgress` 同步变化，说明显示帧在真实推进。
- 中途发生自然切歌 → `グレースノート`：新曲重新检索（先 `currentLineIndex = -1` 检索中），
  随后自行解析成功（`lyricSource = Netease`，行号从 0 开始），**没有沿用上一首的歌词**。
  这验证了"切歌身份一致、旧请求不覆盖新曲"。

## 其余风险项的结论（接手包提出，已核对代码与运行证据）

| 风险项 | 结论 |
| --- | --- |
| `duration.rs` 是否真的接入 | **已接入**：`run()` 每轮轮询都调用 `durations.enrich(&mut sample.playback)`，缓存落 `database/track-durations-v1.json`。仅当网易云原生时间轴不可用（`native == false`）时才把 `durationSource` 切到它；本机原生可用，所以显示"网易云实时读取"。 |
| SMTC 创建 / 读取直接失败的回退 | 创建失败 → 记 `SMTC` 日志并 5 秒后重试；读取失败 → 记日志、丢弃会话并 1 秒后重试。SMTC 报告无曲目时回退到**网易云窗口标题**（`netease_window_metadata`）。本机日志无 `SMTC` 错误，未触发。 |
| 旧网易云 750 ms 刷新 / 会话重绑定 | 新轮询间隔 **160 ms**（≈6 Hz），每轮都 `netease.read(online)`，比旧版 750 ms 更密；曲目键变化由 `accept()` 重建，重绑定随之完成。 |

## 仍需人工确认

- 托盘右键菜单的实际落点（机制已修并有用例守住：`shell::tests::tray_menu_process_becomes_per_monitor_dpi_aware`，
  但"菜单出现在鼠标旁"最终要你右键看一眼）。
- 胶囊在屏幕上的观感（配色、动画节奏）——自动化自检覆盖了 35 项几何与动画断言，但好不好看是人眼的事。


# 验证记录 · Lyric Island 0.7.8 · 2026-10-14

- 应用回归测试 118 项通过，歌词核心 209 项通过。
- 进度条在 0 / 20 / 50 / 90 / 100% 验证填色终点等于圆点中心，且圆点中心等于轨道宽度乘进度；00:00 填色宽度为 0。
- 新歌回零和向后跳转不会从上一位置继续做插值动画，回归通过。拖动热区仍为 20px。
- 已安装歌词库 IL 证明原 ClearProxy 仍继承默认代理，而 DisableProxy 明确禁用代理。生产初始化与重试均切换为后者。
- 直连实测使用隔离进程的不可用默认代理，分别验证歌曲信息、生产封面下载器和生产网易云歌词适配器；结果保存于 direct-network.json。
- 不改 Windows 代理、DNS、网络接口、注册表或环境变量；不控制真实播放器。
- 保留贴顶、胶囊连续形变、播放活动条、无绿色焦点框、新图标及系统透明/高对比度适配。

## Rust 内核（0.7.8.2）

- 决策核心搬入 Rust（`LyricSearchPlanner.CreatePlan` / `LyricIdentityEvaluator.Evaluate` /
  `LyricResolutionCoordinator.SelectBestOutcome`），新增 FFI `li_plan_search`、
  `li_evaluate_candidate`、`li_select_best`；选择规则在 Rust 侧只有一份实现。
- 测试：Rust `cargo test` **126 项通过**（core 106 + ffi 14 + cli 6）；
  `TaskbarLyrics.Core.Tests` 209 项在 Rust 引擎下 209 通过、强制 C# 亦 209 通过；
  `LyricIsland.Tests` 118 通过；`dotnet build LyricIsland.sln` 6 个项目 0 错 0 警告。
- A/B 新增发现的差异：C# 测试用 `Assert.Same` 断言检索计划返回的 `TrackIdentity` 是调用方实例，
  FFI 往返会产生副本 → 已改为字段完全相同时复用调用方实例。
- 打包（0.7.8.2）：
  - `LyricIsland-0.7.8.2-Rust-Portable.zip`：483 项、`LyricIsland-0.7.8.2/` 前缀，
    7 项必需文件齐全（注意磁盘上真实文件名为 `Microsoft.ui.xaml.dll`，大小写与 Bundle 检查无关）；
    SHA-256 见 `outputs\SHA256SUMS.txt`。
  - `LyricIsland-0.7.8.2-Rust.exe`（分享版自解压）：`--verify-package` 返回
    `{"success":true,"version":"0.7.8.2","settings":true}`；从释放目录跑 `--engine-info` 输出
    `app version : 0.7.8.2` / `lyric engine: rust 0.7.8.2` / `native dll: loaded` / 探针 100 与 0。
  - 分享版释放目录为 `%LOCALAPPDATA%\LyricIsland\Runtime\0.7.8.2`，不覆盖已有的 0.7.8。
- 未验证：灵动岛 UI 实际运行（需人工启动观察）、SMTC / 网易云内存时间轴、在线歌词源。

## Rust 内核（0.7.8.1）

- `cd Rust && cargo test`：120 项通过（core 104 + ffi 10 + cli/lib 6），`cargo build` 无警告；
  `Rust/dist/LyricIsland-Rust-0.7.8.2.exe` SHA-256
  `e493c21e36832dbbc56ca3cb4657f555491001cd1f0db37d3dc99469641ffaa`。
- 独立 CLI：单文件静态链接（`+crt-static`），空目录 + 最小 PATH 下可运行，导入表无 MinGW 运行库；
  五种格式夹具解析、`match`（跨文字歌手 94 / 版本冲突 0）、本地目录 `resolve`、`frame`（3.6s → 行索引 1、35.0%）、
  `--menu` 菜单交互、断管不 panic 均已验证。
- C# ↔ Rust 接入（`lyric_ffi.dll` + `RustLyricEngine`）：
  - `TaskbarLyrics.Core.Tests` 209 项：**Rust 引擎 209 通过**；`LYRICISLAND_LYRIC_ENGINE=csharp` 强制 C# 亦 209 通过。
  - `LyricIsland.Tests` 118 项：118 通过（Rust 引擎生效）。
  - `RustInterop.Smoke`：ALL CHECKS PASSED（引擎加载、评分 0/100/94、QRC 逐字时序、LRC 平移与翻译对齐、KRC 解密、本地侧车文件）。
  - 本地 provider 双引擎对照（7 个夹具：`艺人 - 标题`、`[艺人] 标题`、音轨号前缀 + 非 ASCII、`.qrc`、`.krc`、标题兜底、完全不匹配）：
    6 项完全一致；1 项为有意差异——原 C# `ParseKrc` 丢弃 `.krc` 逐字时间后按字数合成，
    Rust 保留真实逐字时间（`krcsong` 由 7 个合成音节变为 2 个真实音节）。
  - A/B 发现的真实缺陷：`informationLineStartTimesMs` 诊断未透传 → 1 项测试失败 → FFI 增加 diagnostics 参数后两侧一致。
  - 回退验证：DLL 缺失时构建与运行照常（引擎自检报 `csharp fallback`），不抛异常。
- 打包验证（含 Rust 引擎）：
  - 应用发布：`LyricIsland.exe`(94.9 MB) + `lyric_ffi.dll`(2.7 MB) + `Assets/` + WPF 原生库；`--engine-info` 报
    `lyric engine: rust 0.7.8.2` / `native dll: loaded` / 探针 100 与 0。
  - 便携包：`LyricIsland-0.7.8-Rust-Portable.zip`（182.7 MB，483 项，`LyricIsland-0.7.8/` 前缀）
    SHA-256 `de5a22c07c22d3e95318047b369bd799d58d67d138e9cfe0372f002c479425c1`。
  - 分享版自解压 exe：`LyricIsland-0.7.8-Rust.exe`（250 MB）；`--verify-package` 返回
    `{"success":true,"version":"0.7.8","settings":true}`，从释放目录再跑 `--engine-info` 仍报 `rust 0.7.8.2 / loaded`。
  - 未验证：灵动岛 UI 实际运行（需人工启动观察）、SMTC / 网易云内存时间轴、在线歌词源。

