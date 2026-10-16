# 第三方组件与许可

歌词岛 1.2 使用以下第三方组件与移植来源。
本文件列出项目来源、移植参考与主要随包组件。原作者版权及许可声明保持有效；许可全文见根目录 `LICENSE`、本文件与 `licenses/`。其他依赖的具体版本见 `Rust/Cargo.lock`，其许可及 NOTICE 以对应上游包所附文件为准。

## 1. Lyric Island / TaskbarLyrics（本项目与其原始项目）

来源：[ANYNC/TaskbarLyrics](https://github.com/ANYNC/TaskbarLyrics)。

MIT License，Copyright (c) 2026 anync。后续修改：Copyright (c) 2026 Heiamer。完整文本见根目录 `LICENSE`。

本项目在原作者许可下继续开发；原始 MIT 声明与版权行必须保留。

## 2. 网易云进度适配器来源

AF-Media-Bar: https://github.com/Fervent-Tempo/AF-Media-Bar
NetEase-Cloud-Music-DiscordRPC: https://github.com/Kxnrl/NetEase-Cloud-Music-DiscordRPC

MIT License

Copyright (c) 2026 AmorFate
Copyright (c) 2018 Kyle

各自的独立许可副本见 `licenses/AF-Media-Bar-MIT.txt` 与 `licenses/NetEase-Cloud-Music-DiscordRPC-MIT.txt`。

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## 3. Lyricify-Lyrics-Helper（协议移植参考）

https://github.com/WXRIW/Lyricify-Lyrics-Helper

Apache License 2.0，Copyright 2023 XY Wang, WXRIW。

Rust 后台把 QQ 音乐、网易云音乐、酷狗与 LRCLIB 的歌词协议处理移植到了
`Rust/crates/lyric-backend/src/providers.rs`。移植是**参考该项目的协议实现**重写的 Rust 代码，
在此保留来源、归属与 Apache-2.0 许可。

- 完整许可文本：`licenses/Lyricify-Lyrics-Helper-Apache-2.0.txt`
- 协议阅读所用的本地检出：`e875781e2e889d97c3c518fe388decd14805a56f`（2026-09-30）
- 历史上随 C# 版本引用的 NuGet 包 `Lyricify.Lyrics.Helper` 0.2.0，其 nuspec 记录的提交为
  `a139e385b032b9abfca4c060627776cf9c3147ad`；两者可能有差异。

## 4. GPUI（设置界面）

https://github.com/zed-industries/zed · gpui 0.2.2，Apache License 2.0，Copyright 2022 - 2025 Zed Industries, Inc.

设置界面使用 gpui 0.2.2 的 DirectX 渲染后端。

- 完整许可文本：`licenses/GPUI-Apache-2.0.txt`

## 5. SQLite（歌曲映射数据库）

发行目录中的 `e_sqlite3.dll` 是 SQLite 的原生库，取自 NuGet 包
`SQLitePCLRaw.lib.e_sqlite3` 2.1.6 的 `runtimes/win-x64/native` 资产。
（该文件与 0.7.8.3 随包发布的那份逐字节相同，SHA256
`DCCBABB2BC7E7D4302C44D9CE41B70721A7D0914FA4D289E2F340D39766AD102`。）

- SQLite 本身处于公有领域（public domain）：https://www.sqlite.org/copyright.html
- SQLitePCLRaw 采用 Apache License 2.0：https://github.com/ericsink/SQLitePCL.raw

Rust 后台通过它只读访问用户数据目录下的 `database/song_maps.db`，用于歌曲别名映射。

## 6. 字体

中文使用 Noto Sans SC，日文使用 Noto Sans JP；两套均为未修改的完整变量字体，采用 SIL Open Font License 1.1。

Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.

- Noto Sans SC 来源：https://github.com/google/fonts/tree/main/ofl/notosanssc
- Noto Sans JP 来源：https://github.com/google/fonts/tree/main/ofl/notosansjp
- 完整许可：`licenses/NotoSansSC-OFL.txt`、`licenses/NotoSansJP-OFL.txt`
- 资源位置：`Assets/Fonts/NotoSansSC.ttf`、`Assets/Fonts/NotoSansJP.ttf`

具体使用说明见 `FONT-NOTICE.md`。

## roxmltree（通知 XML 解析）

https://github.com/RazrFalcon/roxmltree · 0.20.0 · MIT OR Apache-2.0。采用 MIT 许可副本，见 licenses/roxmltree-MIT.txt。
