# Rust 迁移状态 · 1.0

从官方 0.7.8.3 源码构建。1.0 完成架构迁移：**胶囊保持 WPF，设置改为 Rust 前端，非 GUI 后台全部使用 Rust**。
设置界面在原生预览版 `Native-Preview-20261015r2` 由 Slint 换成 **GPUI 0.2.2**（Zed 的 GPU 界面框架，
Windows 上使用 DirectX 渲染后端）。

| 路径 | 实际运行实现 |
| --- | --- |
| 身份匹配、检索计划、候选准入、最终选择 | Rust `lyric-core`（planner / policy / resolution） |
| LRC/QRC/KRC/YRC/TTML 解析与 QRC/KRC 解密 | Rust `lyric-core`（formats / crypto） |
| 本地歌词扫描、取词与索引预热 | Rust `lyric-core::local` |
| 显示帧与逐字进度计算 | Rust `lyric-core::sync`，固定布局帧 ABI + 文本缓存 |
| 原始 / 解析 / 最终结果缓存 | Rust：`JsonStore`（UTF-8 `RawValue`、按需解析、原子替换）+ `lyric-backend::storage::FinalCache` |
| 在线歌词与封面请求 | Rust `lyric-backend::http`（WinHTTP，access type NO_PROXY）与 `providers` |
| QQ / 网易云 / 酷狗 / LRCLIB 协议 | Rust `lyric-backend::providers`（网易云 EAPI AES/MD5、QQ QRC 解密等） |
| 流程编排、代际取消、重试、多提供商并行 | Rust `lyric-backend::runtime` |
| 歌曲映射与 SQLite | Rust `lyric-backend::storage`，经原生 `e_sqlite3.dll` 只读访问 `database/song_maps.db` |
| 配置读写、日志轮转 | Rust `lyric-backend::io` |
| 设置管道服务端 | Rust `lyric-backend::ipc`（同用户 SID 校验、大小与时间上限） |
| 开机注册事务与回滚、托盘菜单、网易云喜欢快捷键 | Rust `lyric-backend::shell` |
| 媒体会话（SMTC）、封面、媒体命令 | Rust `lyric-backend::media`（WinRT） |
| 网易云进程内存时间轴 | Rust `lyric-backend::netease`（只读扫描） |
| 单调时钟与时长补全 | Rust `lyric-backend::clock` / `duration` |
| 设置界面 | **GPUI 0.2.2**（`Rust/crates/lyric-settings`，Windows 上使用 DirectX 渲染后端；crate 名 `lyric-settings` 与产物名 `lyric-island-settings` 不变，命名管道协议也不变） |
| 胶囊窗口、形变动画、布局、显示控件与交互状态 | C# WPF（`LyricIsland.App`，保留原动画实现） |
| 模型转换、原生 IO / 设置 / 启动 / 喜欢 的桥接 | C# + `TaskbarLyrics.GuiContracts`（PInvoke、SafeHandle、日志桥、identity 桥） |

生产构建（`UseRustBackend=true`，默认）在 `LyricIsland.App.csproj` 中排除托管的后台实现
（MusicEngine、SMTC、歌词缓存、封面、时长、设置 IO、启动注册等）。这些文件仍留在源码树中供参考，
可用 `UseRustBackend=false` 构建出旧托管后台，**它不是 1.0 的生产路径**，仅为跑历史回归测试而保留。

所有运行时 HTTP 强制直连：不读取系统或环境变量代理，也不修改用户的 Windows 代理设置。

## 构建与发行

本机工具链的约束不是可选项：

- GNU 工具链的 `ld` 在旧 Slint 构建脚本里崩溃（`0xC0000005`），因此改用 `rust-lld` +
  `linker-flavor=ld.lld`；`gnu-lld` 需要不稳定选项，不可用。GPUI 构建沿用同一套链接设置。
- GNU sysroot 缺少 `libshlwapi.a`，而 `webbrowser` crate 需要 `AssocQueryStringW`；
  只用 `build\shlwapi\libshlwapi.a` 为 `lyric-settings` 补上（LLVM `dlltool` 生成，`.def` 一并保留）。
- 设置 crate 现在还需要 Windows SDK 的 `fxc.exe`（GPUI 在 release 构建里用 `fxc` 编译自己的 HLSL 着色器）
  和一个 C 编译器（本机用 `zig cc` 包装，供少量传递依赖编译 C 代码）；GNU sysroot 同样缺少 GPUI
  传递依赖需要的 `ktmw32` 导入库，已随源码提供 `build\ktmw32\libktmw32.a`。
- `llvm-dlltool.exe` 是 `llvm-tools-preview` 里约 67 MB 的工具，不随源码包发出，构建时从工具链或
  `..\native-tools` 取用。

一条命令产出全部交付物（Rust 后台 → 设置 EXE → WPF publish → 组合 → 打包 → `--verify-package`）：

```powershell
.\build\release.ps1              # 完整链路（1.0 的 WPF 发行链）
.\build\release.ps1 -SkipRust    # 已构建过 Rust 时跳过 Rust 部分
```

只重建 Rust 半边（迭代用）：

```powershell
.\build\native.ps1 -DllTool <llvm-dlltool.exe> -FxcPath <fxc.exe> -ZigCc <zig cc> -Offline
.\build-rust.cmd --dlltool <llvm-dlltool.exe> --fxc-path <fxc.exe> --zig-cc <zig cc>
.\build-rust.cmd --skip-settings  # 只构建后台 DLL，并把 DLL 放到 Rust\artifacts
```

`build\native.ps1` 先跑 `cargo test -p lyric-native -p lyric-settings`，再 `--release` 构建这两个可执行文件；
`-Offline` 表示只用本地缓存。`build-rust.cmd` 也已改成 GPUI 配方：构建 `lyric_ffi.dll`，再（除
`--skip-settings` 外）构建 GPUI 设置 EXE，并接受 `--dlltool` / `--fxc-path` / `--zig-cc` / `--target-dir` /
`--online`，也接受 `build\native.ps1` 的 `-DllTool` / `-FxcPath` / `-ZigCc` / `-TargetDirectory` / `-Offline`
拼写；`RUSTFLAGS` 里同时带 `-C target-feature=+crt-static` 与 `build\shlwapi`、`build\ktmw32` 两个导入库
目录，默认 `--offline`；每次构建后用 `llvm-objdump -p` 断言产物导入表中没有 `libgcc_s_seh-1.dll` /
`libwinpthread-1.dll`。它**不跑测试、也不构建 CLI**。另有历史遗留：`LyricIsland.sln` 里仍有一条指向并
不存在的 `build-rust.ps1` 的方案项，不影响命令行构建（本文档不修改 `.sln`）。

托管后台的历史回归（需要全局属性，否则 App 的还原图仍是生产图、缺 EF Core）：

```powershell
dotnet test LyricIsland.Tests -c Release -p:UseRustBackend=false
dotnet test TaskbarLyrics.Core.Tests -c Release
```

发行布局由 `build\merge_runtime.py` 组合并断言：必需文件齐全、GNU 运行库与 WinUI/.pri 一律不存在、
完整字体摘要匹配。`build\package_release.py` 负责便携 ZIP、源码 ZIP、分享 EXE 与 SHA256，
随后 `release.ps1` 用分享 EXE 的 `--verify-package` 实际释放一遍作为交付门槛。

分享 EXE 仍是 Windows 内置 .NET Framework 的小型启动器，内嵌便携 ZIP，首次运行释放到
`%LOCALAPPDATA%\LyricIsland\Runtime\1.0`。`LyricIsland.exe --engine-info` 可检查版本与原生引擎，
`--smoke-test` / `--presence-test` / `--settings-test` 是生产模式的自检入口。

## 验证

见 [VALIDATION.md](VALIDATION.md)。本版**不做体积与内存对比，也不附带性能报告**。
