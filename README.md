<div align="center">

<img src="Assets/LyricIsland-v2.png" width="104" alt="点屿 DotIsle">

# 点屿 · DotIsle

**重要的事，一眼可见。**

Windows 桌面灵动岛 · 音乐歌词 · 流程与专注 · 通知待办 · 设备提醒

[下载最新版](https://github.com/Heiamer/LyricIsland/releases/latest) · [更新日志](CHANGELOG.md) · [反馈问题](https://github.com/Heiamer/LyricIsland/issues)

</div>

## 下载与使用

适用于 Windows 10 / 11 x64。请到[最新发行版](https://github.com/Heiamer/LyricIsland/releases/latest)选择所需文件：

| 类型 | 使用方式 |
| --- | --- |
| 安装版（以 `Setup.exe` 结尾） | 使用标准安装向导，可选择安装目录，并从 Windows 设置中卸载。 |
| 独立便携版（以 `DotIsle` 开头的 `Portable.zip`） | 完整解压到可写文件夹，运行 `LyricIsland.exe`。 |
| 完整源码（以 `Source.zip` 结尾） | 包含 Rust 源码、资源、构建工具与验证工具。 |
| 校验清单（以 `SHA256.txt` 结尾） | 核对安装包、便携包和源码包的完整性。 |

首次运行可跟随动画教程学习手势，也可从系统托盘打开控制中心。请保留完整运行目录，字体、SQLite 和其他随包组件需要一起使用。

便携版的设置和缓存保存在旁边的 `Data` 文件夹，移动时复制整个目录并保留 `Portable.json`。安装版的数据保存在用户目录。切换运行方式前，先退出当前点屿。

## 主要功能

- **音乐与歌词**：显示封面、歌曲信息、原文和翻译，支持播放控制、进度调整、三种展开布局与切歌翻面动画；封面配色采用 Material You 取色。
- **流程与专注**：番茄钟、计时器、秒表与流程编排；模块可以保存、复制、改名和整卡拖动，落点实时反馈。
- **灵动岛手势**：点击或悬停展开，左右拖动切换活动，上拖暂时隐藏；可交互教程演示从任务栏托盘找回。
- **通知与待办**：通知历史、白名单和黑名单；待办支持定时、重复、稍后提醒及关联专注。
- **系统与设备提醒**：锁定键、麦克风静音和触控板状态提示；设备连接与断开提醒可分类设置，提示可放在屏幕下方。
- **播放器管理**：默认响应播放器，可按来源排除；浏览器媒体默认不响应。
- **外观与操作**：主题、配色、尺寸、动画、音效和自定义快捷键集中管理，设置自动保存。

播放器控制和歌词获取能力取决于播放器提供的媒体接口。设备分类及状态读取能力取决于 Windows 和驱动；不会记录鼠标、键盘的操作内容。

## 在线更新

自动检查发现更新后会在灵动岛提醒。下载显示真实进度，校验通过后确认安装，并保留设置与数据。更新支持 GitHub 与 Gitee 两个来源。

以 `LyricIsland` 开头的便携包及校验文件供旧更新器使用；手动使用独立便携版时，请选择以 `DotIsle` 开头的包。旧客户端无法连接更新源时，可从最新发行页手动下载。

## 技术与源码

主程序使用 Rust、Win32、Direct2D、DirectWrite 与 DirectComposition；控制中心使用 Rust / GPUI。随包提供 Noto Sans SC、Noto Sans JP 和 SQLite。

[运行与数据说明](NATIVE-README.md) · [构建与验证](docs/BUILDING.md) · [详细更新记录](CHANGELOG.md)

## 许可

项目许可见 [LICENSE](LICENSE)，字体许可见 [FONT-NOTICE.md](FONT-NOTICE.md)，依赖归属见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。内置提示音为原创音效。
