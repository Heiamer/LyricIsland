# 点屿 · 原生运行与构建

主程序采用 Rust、Win32、Direct2D、DirectWrite 与 DirectComposition，控制中心采用 Rust / GPUI 的 DirectX 渲染后端。安装、卸载、更新、构建、打包与验证工具均使用 Rust。

## 运行与数据

完整解压运行包后打开 `LyricIsland.exe`，从系统托盘进入控制中心。请保留随包字体、设置程序、更新器、SQLite 和其他组件。

- 便携版保留 `Portable.json`，设置及缓存位于程序旁的 `Data`。
- 安装版由 `Installed.json` 指向用户数据目录，可从 Windows 设置中卸载。
- 可用 `--data-dir` 指定独立数据目录；正常使用时只运行一个主程序。
- 音乐、通知、待办和计时共用灵动岛；上拖隐藏不停止播放与计时，可从托盘找回。

控制中心按灵动岛、概览、通知、待办、流程与专注、设置、关于与更新分组。相关功能的设置抽屉编辑同一份配置。软件内显示简短更新摘要，完整历史记录见 [CHANGELOG.md](CHANGELOG.md)。

播放器默认响应，`IgnoredSources` 保存不响应名单，浏览器媒体默认不响应。旧允许名单会自动迁移。媒体控制、歌词与设备检测能力取决于播放器接口及 Windows 驱动。

## 字体与显示

随包提供完整 Noto Sans SC 与 Noto Sans JP 字体，无需额外安装。歌词支持翻译、逐字高亮、双行及宽度自适应。音乐卡片可选择舒展、紧凑和歌词聚焦布局。

封面配色使用 Celebi 量化、Material Score 选色及 HCT 色调板，保留深色背景与强调色对比度保护。界面动画可关闭；连续手势会保留当前运动状态。

## 构建与验证

工具链准备、构建参数、完整打包和端到端验证见 [构建说明](docs/BUILDING.md)。发行时需要主程序、控制中心、更新器、原生运行库、字体与资源，不能只分发单个主程序。

本地验证应使用独立数据目录和以 `LyricIsland.Settings.test.` 开头的测试管道，避免连接正在使用的实例。

## 许可

字体与第三方组件许可分别见 [FONT-NOTICE.md](FONT-NOTICE.md) 和 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。
