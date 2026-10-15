# LyricIsland Rust 重制版 · 0.7.8.2

把 LyricIsland 0.7.8 / TaskbarLyrics 1.7.4 里 **与 Windows 桌面栈无关的歌词内核** 用 Rust 重写。
外壳（WPF 灵动岛、WinUI 设置、SMTC、托盘、DWM 材质）不动；可以重制且值得重制的部分全部重制。

- 库：`lyric-core`（对应 `TaskbarLyrics.Core`）
- C ABI：`lyric-ffi` → `lyric_ffi.dll`（C# 应用通过 `RustLyricEngine` P/Invoke 调用）
- 命令行：`lyric-island`（解析 / 匹配 / 解析取词 / 显示帧）
- 状态：**126 项单元测试全部通过**（core 106 + ffi 14 + cli 6），`cargo build` 无警告
- 更新明细：见 [CHANGELOG.md](CHANGELOG.md)；逐文件映射与保真度说明：见 [PORTING-NOTES.md](PORTING-NOTES.md)

C# 侧接入情况（哪些路径已经走 Rust、怎么验收、怎么回退）见仓库根目录的
[`../RUST-PORT.md`](../RUST-PORT.md)。

## 现成的 exe

`dist/LyricIsland-Rust-0.7.8.2.exe` —— 单文件、静态链接（`+crt-static`，配置在项目内
`.cargo/config.toml`，不污染全局），**不需要 .NET / Windows App SDK / MinGW / Rust 运行库**。
**双击即进入交互菜单**；无参数且 stdin 是管道时只打印帮助，`--menu` 可强制进入菜单。
`dist/samples/` 放着原版测试夹具，可以直接试；用法见 `dist/README.md`。

注意：这是歌词内核的 CLI，不是原版那个灵动岛窗口程序。

## 构建与测试

```bash
cargo build --release
cargo test
```

Rust 版本要求 1.75+（开发使用 1.99.0，`x86_64-pc-windows-gnu`）。

## 命令行用法

```bash
# 解析歌词文件（走完整流水线：解密 → 解析 → 翻译对齐 → 粒度判定）
lyric-island parse fixtures/qrc-provider-supplied.qrc
lyric-island parse fixtures/baseline.lrc
lyric-island parse fixtures/ttml-provider-supplied.ttml --json
lyric-island parse fixtures/baseline.lrc --raw     # 底层格式解析结果

# 候选身份打分（0 表示版本冲突硬拒绝）
lyric-island match --title "Midnight City" --artist M83 --duration 244 \
  --candidate-title "Midnight City (Live)"
# identity score: 0  → admission: rejected

# 端到端解析（本地目录 + 缓存）
lyric-island resolve --title "Midnight City" --artist M83 --duration 244 \
  --dir ./my-music --data-dir ./cache

# 某一时刻的显示帧
lyric-island frame "./my-music/M83 - Midnight City.lrc" --at 3.6

lyric-island cache-clear
lyric-island version
```

## 目录结构

```
crates/lyric-core/src/
  models.rs       数据模型与不变量校验
  text.rs         简繁转换、去变音、标题归一化、Jaro-Winkler
  matching.rs     LyricMatcher 身份评分
  policy.rs       准入阈值、信任序、来源路由、瞬时失败判定
  formats.rs      LRC / QRC / KRC / YRC / TTML / 纯文本解析
  crypto.rs       QRC(3DES) / KRC(XOR+zlib) 解密 + 自实现 DES/3DES
  payload.rs      载荷解码 + 解析流水线（翻译对齐、信息行、粒度判定）
  planner.rs      检索变体计划与分阶段执行
  cache.rs        JSON 存储 / 两阶段流水线缓存 / 最终解析缓存
  local.rs        本地歌词源与媒体索引
  resolution.rs   解析协调器、别名映射、trace
  sync.rs         显示帧同步
  projector.rs    旧文档模型 ↔ 流水线模型投影
  lines.rs        独立歌手标签合并
  log.rs          轻量日志
crates/lyric-cli/src/main.rs   命令行前端
fixtures/                       原版测试夹具（LRC/QRC/KRC/YRC/TTML）
```

## 保真度

评分公式、阈值、归一化步骤、缓存键构造、选择规则都与 C# 原版逐条对齐，并用原版测试断言回归：

```bash
cargo test -p lyric-core --lib matching::      # 18 项 LyricMatcher 断言（含精确分值 98 / 100）
cargo test -p lyric-core --lib payload::       # 原版 fixtures 的行/片段时序断言
cargo test -p lyric-core --lib planner::       # 检索变体与准入断言
cargo test -p lyric-core --lib cache::         # 缓存键、迁移、版本失配回退
```

## 与原版的已知差异

1. **在线歌词源未实现**：QQ 音乐 / 网易云 / 酷狗 / LRCLIB 的真实 HTTP 请求不在本次范围内；
   `LyricSource` trait 已就位，补实现即可接入。
2. **异步 → 同步**：原协调器并发请求多个来源并按需取消低优先批次；本端口按信任序顺序评估，
   选择结果一致，墙钟行为不同。
3. **简繁转换**：原版走 OpenCC `T2cn` + Win32 `LCMapStringEx`；本端口使用内置繁体→简体映射表
   （覆盖元数据常见字，未收录的字符原样保留）。纯 Rust 的 OpenCC 实现依赖 `getrandom` /
   `windows-sys`，在当前工具链下无法链接。
4. **SQLite → 显式数据源**：别名映射的选择算法保留，数据行改为调用方传入。
5. **缓存文件**：读得懂 .NET 写的 JSON（字段 camelCase、枚举数字或名字都接受），
   写出的文件同样采用 camelCase。
