<p align="center"><img src="LyricIsland.App/Assets/LyricIsland-v2.png" width="96" alt="歌词岛图标"></p>
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

当前界面与后台位于 `Rust/`，设置界面使用 GPUI，胶囊使用 Windows 原生图形接口。`build/` 包含构建及打包工具，其余目录保留历史实现与回归测试。

公开仓库不包含独立 OPPO 字体文件、签名私钥、个人配置、缓存和发行产物。编译前请按构建说明从官方来源准备字体。

## 来源与许可

本项目基于 [ANYNC/TaskbarLyrics](https://github.com/ANYNC/TaskbarLyrics) 继续开发，保留原作者版权与 MIT 许可。

项目代码采用 [MIT License](LICENSE)。第三方组件与字体不因本项目采用 MIT 而改变许可，详见 [第三方声明](THIRD-PARTY-NOTICES.md) 和 [字体条款](FONT-NOTICE.md)。

## 反馈

请通过 [Issues](https://github.com/Heiamer/1/issues) 提交问题，附上版本、Windows 版本、缩放比例和复现步骤。发送截图或日志前请移除个人通知内容。
