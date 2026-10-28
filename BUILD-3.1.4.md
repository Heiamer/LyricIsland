# DotIsle 3.1.4 本地构建与验证

本版基于用户提供的 outputs-r5/DotIsle-3.1.3-Source.zip，修改未提交、未推送、未发布。

使用 Windows x64、Rust stable-x86_64-pc-windows-gnu、Zig 0.13、GNU dlltool、Python 和 Inno Setup 6。tools/li-cargo.ps1 会设置静态 CRT、链接器和导入库路径；设置 DOTISLE_DLLTOOL、DOTISLE_ZIG 指向本机工具。DOTISLE_TARGET_DIR 可选，用于复用现有编译缓存。

```powershell
& ./tools/li-cargo.ps1 build --release --workspace --bins --examples --offline
& ./tools/li-cargo.ps1 test --workspace --offline
node ./tools/test_netease_control.mjs
```

打包入口 build/package_release.py 使用 --binary-dir、--output、--work-dir、--iscc、--version、--shader-cache；输出目录需要存在本版 Notes.md，运行时子目录必须不存在。二进制包含主程序、设置、更新器和卸载器。

examples/verify_netease_background.rs 使用正式 Backend，在刻意未开启的 CDP 端口配置下验证连续前后跳转、暂停后跳转和恢复，运行需明确 --authorized-and-restore 参数。examples/verify_netease_seek.rs 提供一次跳转并恢复的验收；probe_netease_window.rs 只读检查窗口树。

本机官方网易云 3.1.42.205594 的 orpheus.ntpk SHA256：170413b1675708e87a9e89c2d122a8643a51e86b5b5ae1c6260cfeaf25b19be4。更改此指纹前必须重新验证正常后台、最小化、托盘隐藏、DPI、尺寸、主窗口事件和真实进度回读，不能只换常量。

本次生成的 Verification.json 汇总工作区测试、打包 UI 捕获、原生 IPC、安装/卸载、归档校验与真实网易云测试。Validation.zip 保留对应报告、日志与截图。
