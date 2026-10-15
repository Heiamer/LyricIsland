<p align="center"><img src="Assets/LyricIsland-v2.png" width="96" alt="歌词岛图标"></p>
<h1 align="center">歌词岛 · Lyric Island</h1>
<p align="center">Windows 桌面歌词与音乐控制 · 1.1</p>

歌词岛在屏幕顶部显示当前歌词，点击封面展开音乐卡片，查看翻译、歌曲信息与播放进度。支持连续展开与收起动画，也可以将白名单应用的 Windows 通知显示为横幅。

## 功能

- **桌面歌词**：逐字高亮、翻译切换、本地与在线歌词匹配。
- **音乐卡片**：封面、歌名、歌手、播放控制与平滑进度；进度跳转按播放器能力启用。
- **动画与交互**：连续形变、按压反馈、顶部居中或自由悬浮，可开启点击岛内任意区域展开。
- **个性化设置**：尺寸、颜色、动画、播放器白名单，支持浅色与深色设置界面，调整自动保存。
- **通知提醒**：应用白名单、系统应用图标、内容自适应高度，以及独立的“跟随 Windows 勿扰模式”开关。
- **字体**：中文使用 OPPO Sans 4.0，日文使用 Noto Sans JP；字体分别遵循各自许可。

## 使用

适用于 Windows 10 / 11 x64。请在支持 Windows 媒体会话的播放器中播放音乐。

完整解压运行包后打开 `LyricIsland.exe`，通过系统托盘打开设置。升级至 1.1 前，请先退出旧版主程序及设置窗口。

- 默认收起宽度为 **260 逻辑像素**，已有自定义尺寸保留。
- 默认点击封面展开，也可在设置中开启“点击任意区域展开”。
- 通知默认关闭。启用前请阅读 [通知与使用说明](NATIVE-README.md)，按需安装通知支持并授予系统通知访问权限。
- QQNT 与 QQ 默认列入通知白名单；只有发送至 Windows 通知中心的消息才能被接收。
- 设置及缓存保存在运行目录的 `Data` 文件夹。

[查看更新日志](CHANGELOG.md) · [构建说明](docs/BUILDING.md) · [字体说明](FONT-NOTICE.md)

## 源码

当前界面与后台位于 `Rust/`，设置界面使用 GPUI，胶囊使用 Windows 原生图形接口。`Assets/` 保存图标与字体，`build/` 包含当前构建及打包工具。`LyricIsland.Bootstrap/` 是单文件分享版的解压启动器。

公开仓库不包含独立 OPPO 字体文件、签名私钥、个人配置、缓存和发行产物。编译前请按构建说明从官方来源准备字体。

## 开源来源与致谢

歌词岛基于 [ANYNC/TaskbarLyrics](https://github.com/ANYNC/TaskbarLyrics) 继续开发。感谢以下项目与作者；移植和重写部分同样保留对应的版权与许可声明。

| 项目 / 资源 | 在歌词岛中的用途 | 许可 / 版权 |
| --- | --- | --- |
| [TaskbarLyrics](https://github.com/ANYNC/TaskbarLyrics) | 原始项目、歌词处理与 Windows 媒体会话适配基础 | MIT · anync |
| [AF-Media-Bar](https://github.com/Fervent-Tempo/AF-Media-Bar) | 网易云播放进度适配参考 | MIT · AmorFate |
| [NetEase-Cloud-Music-DiscordRPC](https://github.com/Kxnrl/NetEase-Cloud-Music-DiscordRPC) | 网易云歌曲与播放进度适配参考 | MIT · Kyle |
| [Lyricify-Lyrics-Helper](https://github.com/WXRIW/Lyricify-Lyrics-Helper) | 在线歌词协议处理的移植参考 | Apache-2.0 · XY Wang、WXRIW |
| [GPUI](https://github.com/zed-industries/zed) | 设置界面 | Apache-2.0 · Zed Industries, Inc. |
| [SQLite](https://www.sqlite.org/) / [SQLitePCLRaw](https://github.com/ericsink/SQLitePCL.raw) | 歌曲映射数据库及原生库来源 | SQLite：公有领域；SQLitePCLRaw：Apache-2.0 |
| [OPPO Sans 4.0](https://www.coloros.com/article/A00000074/) | 中文界面字体 | OPPO 字体使用条款 · OPPO 广东移动通信有限公司 |
| [Noto Sans JP](https://github.com/google/fonts/tree/main/ofl/notosansjp) | 日文字体 | SIL OFL 1.1 · Adobe |

## 许可证

项目自身代码采用 [MIT License](LICENSE)，保留原作者 `Copyright (c) 2026 anync`，新增修改署名为 Heiamer。

第三方代码、协议移植与字体分别遵循其原始许可，不统一重新授权为 MIT。详细归属及版权声明见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)，许可文本见 [licenses/](licenses/)，字体来源与使用条款见 [FONT-NOTICE.md](FONT-NOTICE.md)。其他直接及间接依赖以 [Cargo.lock](Rust/Cargo.lock) 锁定的版本和各上游包的许可、NOTICE 文件为准。

## 反馈

请通过 [Issues](https://github.com/Heiamer/1/issues) 提交问题，附上版本、Windows 版本、缩放比例和复现步骤。发送截图或日志前请移除个人通知内容。
