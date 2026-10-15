## 0.7.8.2 · 2026-10-14（进一步 Rust 化）

- `lyric-core::resolution` 新增公开的 `ScoredCandidate` / `select_best_scored`，
  内部 `select_best_outcome` 改为复用它 —— 选择规则只有一份实现，宿主可以直接调用同一套规则。
- FFI 新增三个决策入口：`li_plan_search`（检索变体）、`li_evaluate_candidate`（准入评分）、
  `li_select_best`（最终选择）；C# 侧对应接到 `LyricSearchPlanner`、`LyricIdentityEvaluator`、
  `LyricResolutionCoordinator.SelectBestOutcome`。
- FFI 新增显示帧同步（句柄式）：`li_sync_create/destroy/begin_track/set_document/mark_not_found/
  reset/frame`。文档与加载时间戳留在 Rust，每帧只传位置；C# 的 `LyricSyncService` 在
  `_rustSyncReady` 时把帧计算委托给 Rust。帧对照（12 位置 × 9 字段）与 C# 完全一致，
  热路径实测 17.5 µs/帧（约 57,000 帧/秒）。
- 版本号 0.7.8.2（`lyric_core::VERSION` 与 workspace 版本同步）。
- Rust 侧测试：core 106（含 2 项选择规则）+ ffi 15（含 3 项决策接口与 1 项帧同步）+ cli 6 = **127 项通过**。

## 0.7.8.1 · 2026-10-14

LyricIsland 的 **Rust 重制版**，只重制「可以重制的地方」：把 C# 原版里与平台无关的歌词内核搬到 Rust，
WPF / WinUI 外壳、DWM 材质、托盘、SMTC 适配器等依赖 Windows 桌面栈的部分保持不动。

### 与 C# 应用的接入（2026-10-14 下午追加）

- 新增 **`crates/lyric-ffi`**（`lyric_ffi.dll`，C ABI + JSON）与 C# 侧桥接层
  `TaskbarLyrics.Core/RustLyricEngine.cs`。打包出来的 exe 现在会调用 Rust：
  - `LyricMatcher.Score` → `lyric-core::matching`
  - `LyricifyPayloadParser.ParseAsync` → `lyric-core::payload` + `formats`
  - `LyricifyPayloadDecoder.DecodeAsync` → `lyric-core::crypto`
  - `LocalLyricProvider` → `lyric-core::local`（新增 `warm_index` / `invalidate_index`，FFI 侧按目录集缓存索引；
    C# 构造 provider 时后台预热，对应原版 `LocalMediaIndex` 的后台扫描）
- 回退：DLL 缺失 / 入口点缺失 / 返回错误 → 自动落回原 C# 实现；`LYRICISLAND_LYRIC_ENGINE=csharp` 强制 C#。
- 导出函数：`li_version`、`li_build_info`、`li_match_score`、`li_parse_payload`（含 diagnostics 透传）、
  `li_decrypt`、`li_warm_local_index`、`li_invalidate_local_index`、`li_local_document`、`li_resolve_local`、
  `li_display_frame`、`li_free_string`；每个入口 `catch_unwind` 兜底，panic 不跨 FFI 边界。
- 验收：原版 `TaskbarLyrics.Core.Tests` 209 项在 Rust 引擎下全通过，强制 C# 同样全通过；
  `LyricIsland.Tests` 118 项通过；本地 provider 双引擎 7 用例 6 项一致（1 项为有意保留的 KRC 逐字时间差异）；
  应用与分享版发布产物 `--engine-info` 均报 `rust 0.7.8.2 / loaded`。

重制后是三个 Rust 包：`lyric-core`（库）、`lyric-ffi`（C ABI，供 C# 应用调用）、`lyric-island`（CLI），
共 120 项单元测试全部通过，`cargo build` 无警告。

### 本次重制的内容

**1. 数据模型（`models.rs`）** — 对应 `TaskbarLyrics.Core/Models.*.cs`

- `TrackInfo`、`TrackIdentity`、`SearchQueryVariant`、`LyricSearchPlan`、`SourceTrackCandidate`
- `RawLyricPayload`、`DecodedLyricPayload`、`LyricSearchPlan`、`LyricCandidateEvaluation`、`ResolvedLyrics`
- `LyricDocument` / `LyricLine` / `LyricSyllable`（本地 provider 与显示投影仍在用的旧模型）
- `LyricDisplayFrame`、`PlaybackSnapshot`、`LyricSourceSelection`、全部枚举与 trace 记录
- C# 用抛异常表达不变量（`ArgumentOutOfRangeException` 等），Rust 侧统一改成 `try_new` 返回
  `ModelError`，**校验规则逐条保留**：负起点、结束时间必须晚于起点、空文本、片段不得越出行区间、
  `"//"` / `"///"` / 空白视为「无翻译」。

**2. 文本处理与相似度（`text.rs`）** — 对应 `Utilities.ChineseScriptConverter.cs` + `F23.StringSimilarity`

- 自实现 Jaro-Winkler，参数与原库一致（boost 阈值 0.7、前缀缩放 0.1、前缀上限 4），
  另附 FIPS DES 测试向量校验底层实现。
- 标题/歌手归一化全流程：简繁转换 → 小写 → 去变音符号 → 噪声标签剔除 → 括号内容处理
  （发行版标记保留、普通括号丢弃）→ `feat.` 后缀剔除 → 非字母数字转空格 → 空白折叠 →
  单字符 token 折叠（`R I T U A L` → `ritual`）。
- 文字系统判定（拉丁 / CJK），用于跨语言文字的歌手别名处理。

**3. 身份匹配（`matching.rs`、`policy.rs`）** — 对应 `Utilities.LyricMatcher.cs`、`Services.LyricMatchingPolicy.cs`

- 完整评分公式：标题 0.50 / 歌手 0.30 / 时长 0.20 的加权组合，歌手缺失或跨文字系统时的降级路径，
  专辑 0.05 低权重证据，版本冲突关键词硬拒绝，跨文字歌手封顶 94 分（低于立即接受线 95）。
- `LyricIdentityEvaluator`、`ProviderSongIdPolicy`、`LyricProviderTrustPolicy`（校验信任序与已注册 provider 的置换关系）、
  `LyricSourceRoutingPolicy`（播放器→官方 provider 映射、两阶段回退批次）、
  `LyricTransientFailurePolicy`（网络类失败才可重试，TLS/编程错误不重试）。

**4. 歌词格式解析（`formats.rs`）** — 对应 `Lyricify.Lyrics.Parsers` 的 `LrcParser` / `QrcParser` / `KrcParser` / `YrcParser` / `TtmlParser`

- LRC：多时间戳展开、`[offset:]` 应用（**在解析器内部应用**，翻译载荷同样生效）、开篇制作人信息行过滤、
  增强型 LRC 逐字 `<mm:ss.xx>` 边界校验、同时间戳行合并为「原文 + 翻译」。
- QRC：`[start,duration]` 行头 + `(start,duration)text` 逐字时间戳；非时间戳括号归正文
  （QQ 音乐用括号表达重复乐句），行尾无时间戳文本并入末个音节。
- KRC：`<相对起始,时长,0>文本` 逐音节；行头时长写入行结束时间；内嵌 base64 `[language:...]` 翻译表解出。
- YRC：状态机式扫描 `[行时间](词时间,时长,0)文本`，同时解析首尾 `{...}` 署名信息行。
- TTML：`p` / `span` 时间属性、文本拼接、`information-line` 标记；支持 `hh:mm:ss.mmm`、`hh:mm:ss:ff`、`Ns` / `Nms`。

**5. QRC / KRC 解密（`crypto.rs`）** — 对应 `Lyricify.Lyrics.Decrypter.Qrc/Krc`

- KRC：base64 → 跳过 4 字节头 → 16 字节 XOR 密钥 → zlib 解压 → 去 BOM 字符。
- QRC：hex 文本 → 3DES-ECB 解密（密钥 `!@#)(*$%123ZXC!@!@#)(NHL`）→ zlib 解压 → 去 UTF-8 BOM。
- **Triple-DES / DES 为自实现**（IP/FP/E/P/PC1/PC2/S 盒完整表 + 子密钥调度），
  用经典 FIPS 向量 `133457799BBCDFF1 / 0123456789ABCDEF → 85E813540F0AB405` 校验。

**6. 载荷解码与解析流水线（`payload.rs`）** — 对应 `Services.LyricifyPayloadDecoder.cs`、`Services.LyricifyPayloadParser.cs`

- 纯音乐占位、空载荷拒绝、纯文本不同步行、翻译按 60ms 容差对齐、`informationLineStartTimesMs`
  诊断标记、语法粒度判定（逐行 / 逐词 / 逐字 / 混合）、片段裁剪到行区间。

**7. 检索计划（`planner.rs`）** — 对应 `Services.LyricSearchPlanner.cs`、`Services.LyricSearchStageExecutor.cs`

- 多候选查询变体：exact / normalized / primary-artist / relaxed-title / version-relaxed-title，
  含去重规则与放松原因标注；分阶段执行器在首个「已有可准入候选」的阶段停止。

**8. 缓存（`cache.rs`）** — 对应 `Services.LyricCacheStore.cs`、`Services.LyricPipelineCache.cs`、`Services.ResolvedLyricCache.cs`

- 通用 JSON 存储：键大小写不敏感、磁盘/内存命中区分、契约反序列化失败时整体丢弃而不抛错。
- 两阶段流水线缓存：raw / parsed 两层、SHA-256 载荷指纹（含 `-1:` / `长度:值` 的 null 编码）、
  解析器版本与归一化版本失配时保留 raw、不稳定身份不落盘。
- 最终解析缓存：标题+歌手归一化后 SHA-256 键、上下文作用域、内容结构校验（含中文失败原因）、
  v1 旧文件（`user-lyric-bindings-v1.json`）自动迁移并按「后写覆盖先写」。

**9. 本地歌词源（`local.rs`）** — 对应 `Services.LocalLyricProvider.cs`、`Services.LocalMediaIndex.cs`

- 目录递归索引（单个目录读取失败不影响已收集结果）、文件名解析（音轨号前缀、`[艺人] 标题`、` - ` 分隔）、
  150ms 匹配预算、标题/歌手包含关系兜底加分（88 / 82）、
  `.lrc` / `.qrc` / `.krc` 侧车文件解析、音频内嵌 Vorbis Comment（`LYRICS` / `SYNCEDLYRICS` / `UNSYNCEDLYRICS`）提取、
  UTF-8 严格解码失败后回退 GBK（对应原版 `Encoding.GetEncoding(936)`）、
  无逐字时间时按字符合成音节。

**10. 解析协调器（`resolution.rs`）** — 对应 `Services.LyricResolutionCoordinator.cs`

- 未知标题短路、缓存命中、别名映射、纯音乐映射、指定 provider 优先、本地优先、在线按信任序评估。
- 候选准入、逐候选取词与解析、诊断字段（requestId / queryVariant / identityScore / format / elapsedMs）。
- 选择规则：主 provider 达到立即接受分即短路；否则在高置信池内按「信任序 → 分数」排序取首；
  全失败时按「瞬时失败 / 失败 / 未找到」汇总状态。
- 源占用门（重复进入返回 `Disabled`）、源超时、trace sink 四个回调。

**11. 显示同步（`sync.rs`）** — 对应 `Services.LyricSyncService.cs`

- 稳定曲目身份（忽略 SongId 与时长波动）、时长实质性修正只触发一次、
  开机首 300ms 位置守卫、行索引查找（重复时间戳收敛到首个）、行内进度、逐字扫描进度、
  原文/翻译/下一行输出。

**12. 模型投影（`projector.rs`）** — 对应 `LyricDocumentSemanticProjector.cs`、`ResolvedLyricsCompatibilityProjector.cs`

**13. 命令行程序（`crates/lyric-cli`）**

- `parse <文件>`：走完整流水线解析并打印行/片段/翻译（`--raw` 输出底层格式解析结果，`--json` 输出 JSON）。
- `match`：打印候选身份得分与准入结论。
- `resolve`：本地目录 + 缓存的端到端解析。
- `frame <文件> --at <秒>`：打印某一时刻的显示帧。
- `cache-clear`、`version`。
- **交互菜单**：不带参数启动且 stdin 是控制台时进入菜单（双击 exe 不再一闪而过）；
  `--menu` / `-i` 可强制进入。菜单支持选 1/4 时把文件直接拖进窗口，动作结束后等待回车返回。
  管道调用（stdin 非终端）时仍然只打印帮助，不影响脚本。
- 断管容错：输出被管道提前关闭（如 `resolve ... | head -5`）时安静退出，不再抛 panic。
- 静态链接（`+crt-static`），产出单文件 exe，不依赖 MinGW 运行库；
  预编译产物见 `dist/LyricIsland-Rust-0.7.8.2.exe`（含 `SHA256SUMS.txt` 与 `samples/`）。

### 未重制的部分（保留在 C# 原版）

- `LyricIsland.App`：WPF 胶囊/卡片窗口、Win2D 动画、DWM 材质、置顶控制、托盘菜单、多显示器与 DPI 布局、
  WinUI 3 设置窗口、开机自启注册、图标/字体打包。
- `LyricIsland.App/Media`：SMTC / WinRT 媒体会话、网易云只读内存进度适配器、播放控制与跳转能力判定。
- 在线歌词源：`Services.LyricifySources.cs` 的 QQ / 网易云 / 酷狗 / LRCLIB 网络请求（端口设计上保留
  `LyricSource` trait，可直接补实现）。
- EF Core SQLite 的两个库（`SongSearchMap`、`TrackLyricOffset`）：检索映射的**选择算法**已重制并测试，
  数据来源改为显式传入的行列表，避免为一个只读查表需求引入数据库依赖。
- 单文件打包器 `LyricIsland.Bundle`。
