# 第三方组件与许可

歌词岛 1.1 使用以下第三方组件与移植来源。
**开源移植不得因改写而删除版权与许可声明**，因此每一项都保留归属，完整许可文本放在发行目录的 `licenses/` 中。

## 1. Lyric Island / TaskbarLyrics（本项目与其原始项目）

MIT License，Copyright (c) 2026 anync。完整文本见发行目录的 `LICENSE`。

本项目在原作者许可下继续开发；原始 MIT 声明与版权行必须保留。

## 2. 网易云进度适配器来源

AF-Media-Bar: https://github.com/Fervent-Tempo/AF-Media-Bar
NetEase-Cloud-Music-DiscordRPC: https://github.com/Kxnrl/NetEase-Cloud-Music-DiscordRPC

MIT License

Copyright (c) 2026 AmorFate
Copyright (c) 2018 Kyle

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
并非逐行复制，但按 Apache-2.0 第 4 条仍在此保留归属与许可。

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

OPPO Sans 4.0 不纳入项目 MIT 许可，公开源码仓库不分发独立字体文件。来源与条款见 `FONT-NOTICE.md`。

## Noto Sans JP

Font source: https://github.com/google/fonts/tree/main/ofl/notosansjp
Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.
SIL Open Font License, Version 1.1. Unmodified variable font bundled as Assets/Fonts/NotoSansJP.ttf.
Full license: licenses/NotoSansJP-OFL.txt.
