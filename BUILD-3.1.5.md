# DotIsle 3.1.5 本地构建与验证

本版在 3.1.4 后台进度修复的基础上，修正切歌按钮对比度、收敛控制中心自动配色、重做并增加十二款原创音效，以及折叠式音效选择界面。修改未提交、未推送、未发布。

构建环境与 tools/li-cargo.ps1 的用法见 BUILD-3.1.4.md。本版同样使用 Windows x64、GNU Rust、Zig 0.13、dlltool、Python 和 Inno Setup 6。

```powershell
python ./tools/generate_sounds.py
& ./tools/li-cargo.ps1 build --release --workspace --bins --examples --offline
& ./tools/li-cargo.ps1 test --workspace --offline
```

音效生成器仅使用 Python 标准库，可确定性重建 Assets/Sounds 的十二个 PCM WAV。它们也嵌入主程序；修改音效后必须重新编译主程序。

打包使用 build/package_release.py，版本参数为 3.1.5，其余参数见前版构建说明。设置程序的 `--verify <目录>` 会检查音效分类互斥展开、十二项选择与逐项试听的可达性、选择后的折叠和自定义文件保留，并捕获深浅主题、两种窗口宽度、极端封面色及切歌按钮交互状态。主程序的 `--immersive-preview <目录>` 使用生产渲染器捕获灵动岛音乐卡片。

首次读取旧版设置时，SoundLibraryVersion 升级至 2，恢复内置推荐音色；旧版非空 SoundPath 迁移为 Custom，保留自定义路径、音量与启用状态。选择内置音色不会擦除已保存的自定义路径；单项试听不会保存选择。后续启动保留用户新选的音色。

本版验证报告包含工作区测试、生产界面捕获、原生 IPC、安装与卸载生命周期、归档哈希，以及真实运行实例的版本和音效偏好迁移检查。网易云实现与 3.1.4 一致；本轮未重新向真实播放器发送跳转命令。
