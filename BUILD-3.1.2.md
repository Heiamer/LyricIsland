# 点屿 3.1.2 源码构建

此包包含 9 个在用 Rust crate，GUI 全部为 Vello Hybrid/wgpu/Parley。旧 GPUI 工程及 Python 辅助脚本不在此包内。字体与第三方许可保留。本包对应 GitHub v3.1.2 正式 Release 的源码附件。

Windows x64：安装 Rust stable 的 `x86_64-pc-windows-gnu` 工具链（至少 1.88）、Zig、LLVM 的 llvm-dlltool。Zig 与 llvm-dlltool 放入 PATH；也可设置 `DOTISLE_ZIG`、`DOTISLE_DLLTOOL` 指向可执行文件。需要联网取得 Cargo.lock 指定的依赖，已有缓存时可加 `--offline`。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/li-cargo.ps1 build --release --package lyric-native --package dotisle-app --package lyric-bootstrap
powershell -NoProfile -ExecutionPolicy Bypass -File tools/li-cargo.ps1 test --workspace
```

产物位于 target/release。便携包重命名：`lyric-island-settings.exe` → `LyricIsland.Settings.exe`，`lyric-updater.exe` → `LyricIsland.Updater.exe`；保留 `LyricIsland.exe`。将 Assets、licenses、Portable.json 及许可文档放在三个 EXE 旁。Portable.json 内容为 `{"portable":true,"version":"3.1.2"}`。

自启动事务回归测试使用独立 HKCU 临时键；受限沙盒需允许此测试键写入。无需修改用户真实 Run 项。

运行时不需要 Rust 工具链、Zig、LLVM 或 Python。源码中的旧 MiniExpandedMaterial 键仅用于兼容，两个模式已共用展开材质与卡片几何。

## 混合渲染

渲染栈为 Vello Hybrid 0.2.0、vello_common 0.2.0、wgpu 29.0.3（core/hal 29.0.4）、Parley/Fontique 0.5。`dotisle-scene` 记录有限绘图命令，在真实物理像素尺寸下重放到 Hybrid；没有同时常驻旧 Vello compute 后端。

本地第三方补丁为 `Rust/vendor/vello_hybrid-0.2.0` 的窄数据纹理，以及 `Rust/vendor/wgpu-hal-29.0.4` 的 DXBC 字节码缓存，均由 workspace 的 patch 配置锁定。前者保持主/图层的地址位数一致；窗口尺寸上限继续使用设备限制。1024 像素数据行当前允许最多 1024 行，异常复杂的图形场景可能超过容量；点屿随附页面、动画和分辨率回归均已覆盖。

`Assets/ShaderCache/Hybrid29` 包含本版着色器的通用 DXBC 缓存。文件键包含 HLSL、入口、着色器阶段和编译标志，负载使用 SHA256 校验；缺失、损坏或目录只读不会阻止回退编译。可以用 `DOTISLE_SHADER_CACHE_DIR` 指定缓存目录。不包含显卡专用 PSO，本版不使用旧 `.vello` 和 compute PSO 缓存。

主岛和控制中心仍使用各自的 GPU 设备。图片图集页面为 1024×1024，调用方先把封面缩到最大 512 像素；已上传图片按资源 ID 复用，不逐帧复制像素。普通裁剪直接裁路径；有透明度的组保留独立合成，避免重复乘 alpha。GPU 小块分配范围为 4 MiB；大资源仍按实际需求分配。

验证入口：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/li-cargo.ps1 test --workspace --release
LyricIsland.Settings.exe --verify <目录>
LyricIsland.exe --performance-check <目录> --data-dir <隔离数据目录>
LyricIsland.exe --font-check <报告.json> --data-dir <隔离数据目录>
LyricIsland.Updater.exe --self-test <报告.json>
LyricIsland.Updater.exe --overlay-preview <目录> <运行包目录> <隔离数据目录> 3.1.2
```

测试时设置 `LYRICISLAND_TEST_PIPE=LyricIsland.Settings.test.<独立名称>`，避免影响正式实例。`DOTISLE_TRACE_GPU=1` 打印初始化与分配报告；`DOTISLE_PROFILE_DIR` 指向启动报告目录。控制中心 `--preview --warm` 在隐藏窗口下绘制首帧并在空闲一分钟后退出。

实测口径见随附中文报告。工作集不是系统物理内存净增量，主岛与设置的数字也不能直接视为共享页面去重后的总和。本版没有强制清空工作集或周期性修剪；设置可见窗口仍高于 100 MiB，不保证所有显卡、分辨率和场景都达到相同占用。真实播放器、IME 与桌面关闭/热重开的本轮人工交互验收尚未完成，离屏与隔离状态验证范围见发布说明。

## 安装包

安装程序沿用 build/installer.iss 的 Inno Setup 标准向导和 Rust lyric-bootstrap 维护组件。先安装 Inno Setup 6.7+，再运行：

```powershell
python build/package_release.py --binary-dir target/release --output <输出目录> --work-dir <打包临时目录> --iscc <ISCC.exe> --version 3.1.2 --shader-cache Assets/ShaderCache/Hybrid29
```

输出目录需先放置 DotIsle-3.1.2-Notes.md；打包入口会校验 ZIP、嵌入当前图标与带 SHA256 的安装载荷，生成安装版、便携版及 OTA 兼容包。安装器由 Inno 管理文件与卸载登记，Rust 组件负责迁移和数据所有权校验。`--qa-setup` 额外生成使用独立 AppId 的安装验收包。
