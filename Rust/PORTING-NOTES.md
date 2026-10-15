# 移植说明 · C# → Rust 逐文件映射

来源：`LyricIsland-0.7.8-Source`（TaskbarLyrics.Core 8249 行 / TaskbarLyrics.Core.Tests 5772 行 /
LyricIsland.App 5542 行 / LyricIsland.Settings 731 行）。

状态含义：

- **逐条重制**：算法与校验规则逐条对应，包含原测试断言的精确数值。
- **语义重制**：行为等价，但 API 形态按 Rust 惯例调整（异常 → `Result`、`async` → 同步、类 → 结构体/trait）。
- **未重制**：依赖 Windows 桌面栈或超出本次范围，已在 README / CHANGELOG 说明原因。

## TaskbarLyrics.Core

| C# 文件 | Rust 模块 | 状态 | 说明 |
| --- | --- | --- | --- |
| `Models.TrackInfo.cs` | `models::TrackInfo` | 逐条重制 | `Duration` 天然非负，原 `TimeSpan < 0` 钳制保留为显式方法 |
| `Models.LyricDocument.cs` | `models::LyricDocument` | 逐条重制 | 按时间戳排序 + `LooksLikePureMusic` 的信息行白名单 |
| `Models.LyricPipeline.cs` | `models` 内多个 struct | 逐条重制 | `LyricProviderId` 非空校验、`ParsedLyricSegment`/`ParsedLyricLine` 全部构造校验 |
| `Models.LyricResolveResult.cs` / `LyricResolutionResult.cs` / `LyricSource*.cs` / `LyricDisplayFrame.cs` / `PlaybackSnapshot.cs` | `models` | 逐条重制 | 枚举取值与序数（0..8 等）与原版一致，便于读 .NET 写出的 JSON |
| `Models.LyricPipelineCache.cs` / `Models.ResolvedLyricCache.cs` | `cache` 内的 envelope/entry | 语义重制 | 字段 camelCase；枚举接受「名字或数字」两种形态 |
| `Models.LyricResolutionTrace.cs` | `models` trace 记录 | 逐条重制 | 四个 trace 回调原样保留 |
| `Models.LyricDiagnosticReport.cs` | — | 未重制 | 诊断报告依赖 `LyricDiagnosticRunner` 的采集器，属外壳功能 |
| `Abstractions.*.cs` | `resolution` / `cache` 的 trait | 语义重制 | `ILyricSource` / `ILyricMappingResolver` / trace sink / cache 接口全部有对应 trait |
| `Utilities.LyricMatcher.cs` | `matching.rs` + `text.rs` | 逐条重制 | 18 项原断言在 `matching::tests` 中全部通过，含精确值 98 / 100 |
| `Utilities.LyricLineNormalizer.cs` | `lines.rs` | 逐条重制 | 1500ms 合并窗口、常见标签表、章节标签排除 |
| `Utilities.ChineseScriptConverter.cs` | `text::to_simplified` | 语义重制 | 内置映射表替代 OpenCC + `LCMapStringEx` |
| `Utilities.Log.cs` | `log.rs` | 语义重制 | 等级语义与 `Diagnostic(category, message)` 约定保留；改为 stderr + 可选文件 |
| `Services.LyricMatchingPolicy.cs` | `policy::LyricMatchingPolicy` | 逐条重制 | 80 / 95 / 94、20s 时长冲突、5s 超时 |
| `Services.LyricIdentityEvaluator.cs` | `policy::LyricIdentityEvaluator` | 逐条重制 | 含 `ProviderSongIdPolicy` |
| `Services.LyricProviderTrustPolicy.cs` | `policy::LyricProviderTrustPolicy` | 逐条重制 | 重复 / 未注册 / 遗漏三种拒绝原因 |
| `Services.LyricSourceRoutingPolicy.cs` | `policy::LyricSourceRoutingPolicy` | 逐条重制 | 官方 provider 映射 + 两阶段回退批次（原断言「QQMusic → Netease/Kugou/LRCLIB」已复现） |
| `Services.LyricTransientFailurePolicy.cs` | `policy::SourceFailure` + `LyricTransientFailurePolicy` | 语义重制 | 异常类型分支按 SocketError/HTTP 语义归类为枚举 |
| `Services.LyricSearchPlanner.cs` | `planner::LyricSearchPlanner` | 逐条重制 | 五个变体、去重规则、放松原因、`RelaxDelimitedVersionSuffix` 取最后一个分隔符 |
| `Services.LyricSearchStageExecutor.cs` | `planner::LyricSearchStageExecutor` | 逐条重制 | 首个含可准入候选的阶段即返回 |
| `Services.LyricifyPayloadDecoder.cs` | `payload::LyricifyPayloadDecoder` | 逐条重制 | QRC/KRC 解密分派；未加密时透传 |
| `Services.LyricifyPayloadParser.cs` | `payload::LyricifyPayloadParser` | 逐条重制 | 60ms 翻译容差、信息行诊断标记、粒度判定、纯音乐占位 |
| `Services.LyricMatchingPolicy.cs`（重复） | 同上 | — | 原仓库同名文件（两处内容相同） |
| `Services.LyricCacheStore.cs` | `cache::JsonLyricCacheStore` | 语义重制 | 键大小写不敏感、磁盘/内存命中区分、坏契约整体丢弃 |
| `Services.LyricPipelineCache.cs` | `cache::LyricPipelineCache` | 逐条重制 | 载荷指纹编码逐字符对齐（`-1:` / `长度:值`、`\u001f` 分隔） |
| `Services.ResolvedLyricCache.cs` | `cache::JsonResolvedLyricCache` | 逐条重制 | 键 = SHA-256(版本 + 长度前缀的标题/歌手/上下文)，含 v1 旧文件迁移 |
| `Services.LocalLyricProvider.cs` | `local::LocalLyricProvider` | 逐条重制 | 150ms 预算、88/82 兜底分、增强 LRC 边界校验、ID3/FLAC 内嵌歌词、GBK 回退 |
| `Services.LocalMediaIndex.cs` | `local::LocalMediaIndex` | 逐条重制 | 版本化快照、目录读取失败不影响已收集条目 |
| `Services.SongSearchMapResolver.cs` | `resolution::SongSearchMapResolver` | 语义重制 | `SelectBestMapping` 原样；SQLite 查询 → 显式行列表 |
| `Services.LyricDocumentSemanticProjector.cs` | `projector::LyricDocumentSemanticProjector` | 逐条重制 | 行结束时间取下一行时间戳；粒度判定规则一致 |
| `Services.ResolvedLyricsCompatibilityProjector.cs` | `projector::ResolvedLyricsCompatibilityProjector` | 逐条重制 | 音节相对偏移回算、信息行可选过滤 |
| `Services.LyricResolutionCoordinator.cs` | `resolution::LyricResolutionCoordinator` | 语义重制 | 全部分支与选择规则保留；并发批次 → 顺序评估（见「差异」） |
| `Services.LyricSyncService.cs` | `sync::LyricSyncService` | 语义重制 | 帧计算、稳定身份、时长修正、300ms 位置守卫原样；后台检索任务由调用方驱动 |
| `Services.LyricDiagnosticRunner.cs` | — | 未重制 | 诊断采集器，属外壳 |
| `Services.LyricifySources.cs` | — | 未重制 | 在线来源 HTTP 实现（trait 已留位） |
| `Database/*.cs`（EF Core SQLite） | `resolution::SongSearchMap` 等 | 部分重制 | 选择算法重制，持久层未重制 |
| `DirectNetwork.cs` | — | 未重制 | 直连 HttpClient 配置，随在线来源一并保留在 C# 侧 |

## TaskbarLyrics.Core.Tests（断言来源）

| C# 测试 | Rust 测试 | 备注 |
| --- | --- | --- |
| `LyricMatcherTests`（18 项） | `matching::tests`（18 项） | 含 `Assert.Equal(98)` / `Assert.Equal(100)` 精确值 |
| `LyricPipelineContractsTests` | `models::tests` | 校验与排序规则 |
| `LyricSearchPlannerTests` | `planner::tests`、`policy::tests` | Simlish / 括号元数据 / 专辑偏好 |
| `LyricSourceRoutingPolicyTests` | `policy::tests` | 官方 provider 与回退批次 |
| `LyricTransientFailurePolicyTests` | `policy::tests` | 瞬时/永久失败分类 |
| `LyricifyPayloadParserTests` | `payload::tests` | 使用原版同一批 fixtures |
| `LyricCacheStoreTests` / `LyricPipelineCacheTests` | `cache::tests` | 含跨实例回读与版本失配 |
| `ResolvedLyricCacheTests` | `cache::tests` | 键忽略专辑/时长/标识；v1 迁移「后写覆盖」 |
| `LyricResolutionCoordinatorTests` | `resolution::tests`、`policy::tests` | 信任序校验 + 端到端解析 |
| `LyricSyncServiceTests`（部分） | `sync::tests` | 帧计算、逐字进度、时长修正 |
| `SongSearchMapResolverTests` | `resolution::tests` | 精确专辑 / 无专辑兜底 / 歧义拒绝 |
| `LocalMediaIndexTests` | `local::tests`（等价覆盖） | 目录读取失败保护 |
| `DirectNetworkTests` / `LyricifyProxyRecoveryTests` / `LyricifySourceAdaptersTests` / `TrackLyricOffsetStoreTests` | — | 依赖未重制的在线来源与 SQLite |

## LyricIsland.App / LyricIsland.Settings

全部未重制（WPF / WinUI 3 / Win2D / DWM / SMTC / WinRT / 托盘 / 注册表自启 / 单文件打包）。
这些部分与 Windows 桌面栈强耦合，在 Rust 里重做的成本远高于收益，而且会丢掉原版的动效与材质表现。

## 与原版的差异（有意为之）

1. **异常 → `Result`**：`ArgumentOutOfRangeException` / `FormatException` 等改为
   `ModelError` / `PayloadError` / `DecryptError`，调用方显式处理；校验条件不变。
2. **异步 → 同步**：原协调器并发请求所有来源并在主来源达到立即接受分时取消批次；
   端口按信任序顺序评估并在同一条件下短路。选择结果一致，墙钟不同。`LyricSyncService`
   的后台检索任务改为 `begin_track` / `complete_search` 由调用方驱动。
3. **简繁转换表**：内置映射表（约 300 字对）替代 OpenCC `T2cn` + Win32 `LCMapStringEx`；
   未收录字符原样保留（与原版两个转换源都不可用时的行为一致）。
4. **缓存文件**：默认路径沿用 `%APPDATA%\LyricIsland\database\`（非 Windows 平台使用
   `$XDG_DATA_HOME/LyricIsland`）；字段命名 camelCase，枚举既可读名字也可读数字。
5. **超时实现**：原版用 `CancellationTokenSource.CancelAfter`；端口在逐候选循环中用
   `Instant` 检查已耗时（单线程下等价）。
6. **新增能力**：`formats::parse_ttml` 支持 `Ns` / `Nms` 时间写法；QRC/KRC 解密函数附带
   反向加密实现，仅用于测试与夹具生成。
