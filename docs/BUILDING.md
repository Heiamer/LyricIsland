# 构建点屿 2.0.2

本目录说明开发环境、依赖配置和发行构建。普通使用说明请参阅根目录的 NATIVE-README.md。

所有构建、打包与端到端验证都由 Rust 工具 `lyric-tools` 完成；源码不再包含 Python、PowerShell 或 C#。

## 环境

- Windows x64、Git、Rust stable 的 `x86_64-pc-windows-gnu` 工具链。
- LLVM 的 `llvm-dlltool.exe`。
- Windows SDK 的 `fxc.exe`。
- C 编译器驱动；现有配置使用调用 `zig cc` 的包装脚本。
- 打包时需要 SQLite 原生库，默认读取 `%USERPROFILE%/.nuget/packages/sqlitepclraw.lib.e_sqlite3/2.1.6/runtimes/win-x64/native/e_sqlite3.dll`，也可用 `--sqlite` 指定。

## 字体资源

源码已包含完整 Noto Sans SC 与 Noto Sans JP 变量字体，无需额外下载或安装到系统。请保留 `Assets/Fonts/` 与对应许可文件，打包时会校验完整字体的 SHA256。两套字体均使用 SIL Open Font License 1.1，见 `FONT-NOTICE.md`。

## 编译

先构建工具本身：

```powershell
rustup default stable-x86_64-pc-windows-gnu
cargo build --release --package lyric-tools
```

再由工具配置 GNU 工具链、运行工作区测试并构建发行二进制：

```powershell
./Rust/target/release/lyric-tools.exe build `
  --dlltool 'C:/Tools/LLVM/bin/llvm-dlltool.exe' `
  --fxc 'C:/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/fxc.exe' `
  --zig-cc 'C:/Tools/zig-cc.cmd'
```

依赖已缓存时可加 `--offline`。输出位于 `Rust/target-native/release/`，包括主程序、设置程序、安装程序、卸载程序、更新器与工具本身。

运行时需要完整 `Assets/Fonts/`、设置程序、更新器及 SQLite 原生库，不能只复制主程序。

## 打包

```powershell
./Rust/target-native/release/lyric-tools.exe package `
  --version 2.0.2 `
  --binary-dir Rust/target-native/release `
  --output publish/outputs202 --iscc C:/Tools/InnoSetup/ISCC.exe
```

打包产出安装版、便携包、OTA 兼容包、源码包与 SHA256 清单，并生成 `publish/DotIsle-2.0.2/` 运行时目录。安装版由 Inno Setup 6.7+ 标准向导编译，Windows 卸载登记与程序文件清理由 Inno Setup 管理；迁移、数据所有权校验、可选数据清理由 Rust lyric-bootstrap 负责。官方简体中文语言文件随源码保存于 build/。不需要 .NET Framework、Python 或 PowerShell。

## 端到端验证

```powershell
./Rust/target-native/release/lyric-tools.exe verify e2e publish/DotIsle-2.0.2 `
  --version 2.0.2 --artifacts publish/outputs202 --report publish/verification.json
./Rust/target-native/release/lyric-tools.exe verify redesign publish/DotIsle-2.0.2 publish/verify-redesign
./Rust/target-native/release/lyric-tools.exe verify notifications --runtime publish/DotIsle-2.0.2 --work publish/verify-notifications
./Rust/target-native/release/lyric-tools.exe verify installer publish/verify-installer publish/verify-installer/report.json
```

更新器与进度验证需要一个以测试管道运行的实例：

```powershell
$env:LYRICISLAND_TEST_PIPE = 'LyricIsland.Settings.test.progress.local'
./publish/DotIsle-2.0.2/LyricIsland.exe --data-dir publish/verify-progress/Data --run-ms 120000
./Rust/target-native/release/lyric-tools.exe verify progress $env:LYRICISLAND_TEST_PIPE publish/verify-progress publish/verify-progress/report.json
```

```powershell
./Rust/target-native/release/lyric-tools.exe verify updater `
  publish/outputs202/OTA-Compatibility/LyricIsland-2.0.2-Portable.zip `
  <OTA-SHA256> publish/verify-updater publish/verify-updater/report.json 2.0.2
```

端到端验证会启动打包好的运行时、驱动设置管道、运行设置窗口自检、重启持久化，并核对安装包自检与产物摘要。`success: true` 才算通过。
