# 第三方组件与许可

点屿 3.1.1 使用以下第三方组件与移植来源。
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

## 4. Rust / Vello 界面栈

界面使用 Vello Hybrid 0.2、wgpu 29 与 Parley 0.5，不使用 GPUI。依赖的锁定版本见 Rust/Cargo.lock，所选许可全文汇总于 licenses/Rust-Dependencies-Licenses.txt。

## 5. 通知监听与旧版 SQLite 数据库兼容代码（随包捆绑 e_sqlite3.dll）

通知监听（只读 Windows 通知数据库 wpndatabase.db）与旧 database/song_maps.db 的可选只读导入共用随包捆绑的 `e_sqlite3.dll`（win-x64，来自 SQLitePCLRaw 的原生 SQLite 构建，以 Apache License 2.0 分发；SQLite 本身处于公有领域：https://www.sqlite.org/copyright.html ）。该 DLL 只以只读方式打开用户数据库，绝不写入或上传内容。SQLitePCLRaw 许可详见 licenses/e_sqlite3-APACHE-2.0.txt 与 https://github.com/ericsink/SQLitePCL.raw 。

## 6. 字体

西文与数字使用 Google Sans Flex，中文使用 Noto Sans SC，日文使用 Noto Sans JP；均采用 SIL Open Font License 1.1。

Google Sans Flex：Copyright 2025 Google LLC；Noto 字体：Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.

- Google Sans Flex 来源：https://fonts.google.com/specimen/Google+Sans+Flex （2025 年 11 月起以 SIL OFL 发布）
- Noto Sans SC 来源：https://github.com/google/fonts/tree/main/ofl/notosanssc
- Noto Sans JP 来源：https://github.com/google/fonts/tree/main/ofl/notosansjp
- 完整许可：`licenses/GoogleSansFlex-OFL.txt`、`licenses/NotoSansSC-OFL.txt`、`licenses/NotoSansJP-OFL.txt`
- 资源位置：`Assets/Fonts/GoogleSansFlex-*.ttf`、`Assets/Fonts/NotoSansSC.ttf`、`Assets/Fonts/NotoSansJP.ttf`

具体使用说明见 `FONT-NOTICE.md`。

## roxmltree（通知 XML 解析）

https://github.com/RazrFalcon/roxmltree · 0.20.0 · MIT OR Apache-2.0。采用 MIT 许可副本，见 licenses/roxmltree-MIT.txt。

## material-colors（动态色彩与封面取色）

https://github.com/Aiving/material-colors · 0.4.2 · MIT OR Apache-2.0。它是 Google Material Color Utilities（https://github.com/material-foundation/material-color-utilities，Apache-2.0）的社区 Rust 移植，不是 Google 官方实现。源码随附于 `Rust/vendor/material-colors-0.4.2`，采用 MIT 许可副本，见 licenses/material-colors-MIT.txt。

## Inno Setup（安装向导）

安装器由 Inno Setup 6.7.3 编译，许可见 licenses/Inno-Setup-License.txt。

## Material Symbols Rounded

Settings and device-status icons use official Material Symbols Rounded by Google, distributed under Apache License 2.0. Source: https://github.com/google/material-design-icons . The exact revision is recorded in Assets/MaterialSymbols/REVISION.txt. The license is retained in licenses/MaterialSymbols-Apache-2.0.txt. Music transport glyphs use the same icon set; the state-dependent button shape is application geometry.

## 本地混合渲染补丁

本轮修改 Vello Hybrid 0.2.0 的数据纹理布局，以及 wgpu-hal 29.0.4 的 DX12 字节码缓存。完整修改源码和原许可随源码包的 `Rust/vendor/` 提供，原项目许可均保留。便携包附带的 Hybrid29 DXBC 由这些源码生成。旧 compute 版本的 Vello/wgpu26 本地补丁不参与本混合版构建。

新增依赖包括 Vello Hybrid/vello_common/vello_sparse_shaders 0.2.0、glifo 0.3.0、Peniko 0.6.1、Kurbo 0.13.1、wgpu 29.0.3 和 core/hal/types 29.0.4。补充许可见 `licenses/Hybrid-Rust-Licenses.txt`，实际锁定依赖见 `Rust/Cargo.lock`。

## AndroidX Material 3 Motion

本版弹簧参数使用 AndroidX Material 3 的 ExpressiveMotionTokens / StandardMotionTokens v0_14_0。Copyright 2024 The Android Open Source Project，Apache License 2.0。求解器为本项目原生 Rust 实现。原始参数表及许可见 licenses/AndroidX-Motion/。

https://github.com/androidx/androidx/tree/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens
