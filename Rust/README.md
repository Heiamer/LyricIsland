# 歌词岛 Rust 工作区

- `lyric-core`：歌词解析、匹配、搜索决策与同步计算。
- `lyric-backend`：媒体会话、在线歌词、配置、数据库与系统交互。
- `lyric-native`：胶囊、通知、动画与音乐控制窗口。
- `lyric-settings`：GPUI 设置界面。
- `lyric-archive`：ZIP 读写与 SHA256。
- `lyric-bootstrap`：安装、卸载、更新与启动器。
- `lyric-tools`：构建、打包与端到端验证。

七个包共同构成当前应用。构建和测试请使用 `lyric-tools build`；环境准备见 [构建说明](../docs/BUILDING.md)。
