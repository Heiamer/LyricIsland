using TaskbarLyrics.Core;
using TaskbarLyrics.Core.Abstractions;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Services;
using TaskbarLyrics.Core.Utilities;

// Proves that the C# lyric core is actually calling the Rust engine.
// Exit code 0 = the Rust engine answered; 1 = it did not (the app would silently fall back).
Console.OutputEncoding = System.Text.Encoding.UTF8;

// `--local-sweep` / `--frame-sweep` print one line per case so the Rust and C# engines can be
// diffed (`dotnet run ... --frame-sweep` vs `LYRICISLAND_LYRIC_ENGINE=csharp ... --frame-sweep`).
if (args.Contains("--local-sweep"))
{
    return await LocalSweepAsync();
}

if (args.Contains("--frame-sweep"))
{
    return await FrameSweepAsync();
}

var failures = 0;

void Check(string label, bool ok, string detail)
{
    Console.WriteLine($"{(ok ? "[ok]  " : "[FAIL]")} {label}: {detail}");
    if (!ok)
    {
        failures++;
    }
}

Console.WriteLine($"engine      : {RustLyricEngine.Describe()}");
if (string.Equals(
        Environment.GetEnvironmentVariable("LYRICISLAND_LYRIC_ENGINE"),
        "csharp",
        StringComparison.OrdinalIgnoreCase))
{
    Console.WriteLine("[skip] native library loaded (engine disabled on purpose for the C# A/B run)");
}
else
{
    Check("native library loaded", RustLyricEngine.IsActive, RustLyricEngine.Version ?? "<none>");
}
Console.WriteLine();

// 1) Identity scoring ------------------------------------------------------
var track = new TrackInfo(
    "smoke-track",
    "Midnight City",
    "M83",
    "Hurry Up, We're Dreaming",
    "Spotify",
    TimeSpan.FromSeconds(244));

var conflict = LyricMatcher.Score(track, "Midnight City (Live)", "M83", 244);
Check("version conflict rejected", conflict == 0, $"score={conflict}");

var exact = LyricMatcher.Score(track, "Midnight City", "M83", 244);
Check("exact match accepted", exact >= 95, $"score={exact}");

var crossScript = LyricMatcher.Score(
    new TrackInfo("t", "新宝島", "魚韻", "834.194", "Spotify", TimeSpan.FromSeconds(305)),
    "新宝島",
    "sakanaction",
    306,
    "新宝島");
Check("cross-script artist capped below 95", crossScript is >= 80 and < 95, $"score={crossScript}");
Console.WriteLine();

// 2) Payload parsing (QRC with word timing) --------------------------------
var parser = new LyricifyPayloadParser();
var qrc = new DecodedLyricPayload(
    KnownLyricProviders.QQMusic,
    "smoke-qrc",
    LyricPayloadFormat.Qrc,
    "[1000,1800]qrc(1000,300)zero(1300,0)word(1300,1500)",
    null,
    false,
    new Dictionary<string, string>());

var parsed = await parser.ParseAsync(qrc);
var line = parsed.Lines.FirstOrDefault(candidate => candidate.StartTime == TimeSpan.FromSeconds(1));
Check(
    "qrc line timing parsed",
    line is not null && line.EndTime == TimeSpan.FromMilliseconds(2800),
    line is null ? "<no line>" : $"{line.StartTime}..{line.EndTime}");
Check(
    "qrc syllable segments parsed",
    line?.Segments.Count == 2 && line.Segments[1].EndTime == TimeSpan.FromMilliseconds(2800),
    line is null ? "<no line>" : $"{line.Segments.Count} segment(s)");
Check("qrc timing kind", parsed.TimingKind == LyricTimingKind.WordTimed, parsed.TimingKind.ToString());
Console.WriteLine();

// 3) LRC translation alignment + information lines -------------------------
var lrc = new DecodedLyricPayload(
    KnownLyricProviders.LrcLib,
    "smoke-lrc",
    LyricPayloadFormat.Lrc,
    "[offset:+250]\n[00:01.00]alpha line\n[00:04.00]beta line",
    "[offset:+250]\n[00:01.00]第一行\n[00:04.00]//",
    false,
    new Dictionary<string, string>());

var parsedLrc = await parser.ParseAsync(lrc);
Check("lrc offset applied", parsedLrc.Lines[0].StartTime == TimeSpan.FromMilliseconds(1250), parsedLrc.Lines[0].StartTime.ToString());
Check("lrc translation aligned", parsedLrc.Lines[0].Translation == "第一行", parsedLrc.Lines[0].Translation ?? "<null>");
Check("lrc '//' becomes no translation", parsedLrc.Lines[1].Translation is null, parsedLrc.Lines[1].Translation ?? "<null>");
Console.WriteLine();

// 4) Decryption ------------------------------------------------------------
var krc = "[1000,1800]<0,300,0>krc<300,0,0>zero<300,1500,0>word";
if (RustLyricEngine.IsActive)
{
    var encrypted = EncryptForSmoke(krc);
    var decrypted = RustLyricEngine.TryDecrypt(LyricPayloadFormat.Krc, encrypted);
    Check("krc decrypted by rust", decrypted == krc, decrypted is null ? "<null>" : $"{decrypted.Length} chars");
}
else
{
    Console.WriteLine("[skip] krc decrypted by rust (engine disabled: the C# path decrypts internally)");
}

Console.WriteLine();

// 5) Local file provider ---------------------------------------------------
// NOTE: the local provider is still the C# implementation (it is not wired to the Rust
// engine yet); this check only proves the untouched path keeps working alongside it.
var samples = Path.Combine(AppContext.BaseDirectory, "samples");
Directory.CreateDirectory(samples);
await File.WriteAllTextAsync(
    Path.Combine(samples, "M83 - Midnight City.lrc"),
    "[00:01.00]first line\n[00:03.00]second line");

var provider = new LocalLyricProvider(new[] { samples });
var local = await provider.GetLyricsWithDiagnosticsAsync(track);

// The original LocalMediaIndex warms up on a background task, so the first snapshot can
// still be empty; give it a moment exactly like a real playback session does.
for (var attempt = 0; attempt < 20 && local.Document is null; attempt++)
{
    await Task.Delay(100);
    local = await provider.GetLyricsWithDiagnosticsAsync(track);
}

Check(
    "local sidecar resolved (csharp provider path)",
    local.Document is { Lines.Count: >= 2 },
    $"{local.Acquisition}, {local.Document?.Lines.Count ?? 0} line(s)");
Console.WriteLine();

Console.WriteLine(failures == 0
    ? "ALL CHECKS PASSED"
    : $"{failures} CHECK(S) FAILED");
return failures == 0 ? 0 : 1;

// The smoke test needs a KRC container; reuse the Rust encryptor through the CLI's library
// is not exposed, so this mirrors the documented KRC container layout in the test only.
static string EncryptForSmoke(string plain){
    var key = new byte[]
    {
        0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e, 0x69
    };

    var payload = new List<byte> { 0xEF, 0xBB, 0xBF };
    payload.AddRange(System.Text.Encoding.UTF8.GetBytes(plain));

    byte[] deflated;
    using (var output = new MemoryStream())
    {
        using (var deflate = new System.IO.Compression.ZLibStream(output, System.IO.Compression.CompressionLevel.Optimal, leaveOpen: true))
        {
            deflate.Write(payload.ToArray());
        }

        deflated = output.ToArray();
    }

    var container = new List<byte> { 0, 0, 0, 0 };
    for (var i = 0; i < deflated.Length; i++)
    {
        container.Add((byte)(deflated[i] ^ key[i % key.Length]));
    }

    return Convert.ToBase64String(container.ToArray());
}

// Compares the Rust and C# local-provider engines over a fixture set. Run twice:
//   dotnet run ... --local-sweep                                  (Rust)
//   LYRICISLAND_LYRIC_ENGINE=csharp dotnet run ... --local-sweep  (C#)
static async Task<int> LocalSweepAsync()
{
    var root = Path.Combine(Path.GetTempPath(), "lyric-island-local-sweep-" + Environment.ProcessId);
    if (Directory.Exists(root))
    {
        Directory.Delete(root, true);
    }

    Directory.CreateDirectory(root);

    var cases = new (string File, string Title, string Artist, string Content)[]
    {
        ("M83 - Midnight City.lrc", "Midnight City", "M83", "[00:01.00]alpha\n[00:03.00]beta"),
        ("[Toe] Goodbye.lrc", "Goodbye", "Toe", "[00:01.00]toe line"),
        ("07. Sigur Rós - Hoppípolla.lrc", "Hoppípolla", "Sigur Rós", "[00:02.00]hop line"),
        ("Aimer - 残響散歌.qrc", "残響散歌", "Aimer", "[1000,1800]残響(1000,300)散歌(1300,500)"),
        ("Kugou Test - KRC Song.krc", "KRC Song", "Kugou Test", "[1000,1800]<0,300,0>krc<300,1500,0>song"),
        ("Unknown Artist - Some Song.lrc", "Some Song", "Completely Different", "[00:01.00]some line"),
        ("Not Related Song.lrc", "Totally Different Song", "Nobody At All", "[00:01.00]should not match"),
    };

    foreach (var testCase in cases)
    {
        await File.WriteAllTextAsync(Path.Combine(root, testCase.File), testCase.Content);
    }

    var folders = new[] { root };
    var provider = new LocalLyricProvider(folders);

    Console.WriteLine($"engine: {RustLyricEngine.Describe()}");

    // The original provider (and the Rust one) warm their index asynchronously.
    for (var attempt = 0; attempt < 25; attempt++)
    {
        await Task.Delay(100);
    }

    foreach (var testCase in cases)
    {
        var track = new TrackInfo("sweep", testCase.Title, testCase.Artist, "Album", "Spotify", TimeSpan.FromMinutes(3));
        var result = await provider.GetLyricsWithDiagnosticsAsync(track);
        var first = result.Document?.Lines.Count > 0 ? result.Document.Lines[0].Text : "<none>";
        var syllables = result.Document?.Lines.Count > 0 ? result.Document.Lines[0].Syllables?.Count ?? 0 : 0;

        // BestScore is a compatibility-only serialized field, hence the suppression.
#pragma warning disable CS0618
        var score = result.Document?.BestScore ?? 0;
#pragma warning restore CS0618

        Console.WriteLine(
            $"{testCase.File}|found={result.Document is not null}|lines={result.Document?.Lines.Count ?? 0}" +
            $"|score={score}|acq={result.Acquisition}|syllables={syllables}|first={first}");
    }

    Directory.Delete(root, true);
    return 0;
}

// Compares the Rust and C# display-frame math over a set of playback positions.
static async Task<int> FrameSweepAsync()
{
    var track = new TrackInfo("sweep", "Frame Song", "Frame Artist", "Album", "Spotify", TimeSpan.FromSeconds(200));

    var lines = new List<LyricLine>
    {
        new(TimeSpan.FromSeconds(1), "first line", "第一行", new List<LyricSyllable>
        {
            new(TimeSpan.Zero, TimeSpan.FromMilliseconds(500), "first "),
            new(TimeSpan.FromMilliseconds(500), TimeSpan.FromMilliseconds(500), "line"),
        }),
        new(TimeSpan.FromSeconds(3), "second line"),
        new(TimeSpan.FromSeconds(5), "third line", "第三行"),
    };

    var document = new LyricDocument(lines, 0, false);

    // The C# semantic projector is internal, so build the pipeline model directly (same rules:
    // a line ends at the next timestamp, word timing makes the set Mixed).
    var parsedLines = new List<ParsedLyricLine>();
    for (var index = 0; index < document.Lines.Count; index++)
    {
        var line = document.Lines[index];
        var end = index + 1 < document.Lines.Count ? document.Lines[index + 1].Timestamp : (TimeSpan?)null;
        var segments = (line.Syllables ?? new List<LyricSyllable>())
            .Select(syllable => new ParsedLyricSegment(
                line.Timestamp + syllable.RelativeOffset,
                line.Timestamp + syllable.RelativeOffset + syllable.Duration,
                syllable.Text))
            .ToList();
        parsedLines.Add(new ParsedLyricLine(line.Timestamp, end, line.Text, line.Translation, segments));
    }

    var parsed = new ParsedLyrics(
        parsedLines,
        parsedLines.Any(l => l.Segments.Count > 0) ? LyricTimingKind.Mixed : LyricTimingKind.LineTimed,
        LyricTimingProvenance.Unknown,
        LyricPayloadFormat.Lrc,
        document.IsPureMusic);

    var resolved = new ResolvedLyrics(
        parsed,
        KnownLyricProviders.Local,
        "sweep-document",
        LyricAcquisitionKind.LocalFile,
        new Dictionary<string, string>());

    var service = new LyricSyncService(new NoLyricsCoordinator());
    await service.GetDisplayFrameAsync(new PlaybackSnapshot(true, TimeSpan.Zero, track));
    service.TryApplyResolvedLyrics(track, resolved);

    Console.WriteLine($"engine: {RustLyricEngine.Describe()}");

    var positions = new[] { 0.0, 0.1, 0.9, 1.0, 1.25, 2.0, 2.9, 3.0, 4.0, 5.5, 6.0, 9.0 };
    foreach (var seconds in positions)
    {
        var frame = await service.GetDisplayFrameAsync(new PlaybackSnapshot(true, TimeSpan.FromSeconds(seconds), track));
        Console.WriteLine(
            $"{seconds:0.00}|{frame.CurrentLine}|{frame.NextLine}|idx={frame.CurrentLineIndex}" +
            $"|p={frame.LineProgress:0.###}|w={(frame.WordScanProgress.HasValue ? frame.WordScanProgress.Value.ToString("0.###") : "-")}" +
            $"|t={frame.CurrentTranslation ?? "-"}|nt={frame.NextTranslation ?? "-"}|has={frame.HasTrackTranslation}|pure={frame.IsPureMusic}");
    }

    // Hot-path cost: the island asks for a frame on every UI tick (60-240 Hz).
    const int iterations = 200000;
    for (var warm = 0; warm < 2000; warm++) { _ = await service.GetDisplayFrameAsync(new PlaybackSnapshot(true, TimeSpan.FromMilliseconds(1500 + warm % 4000), track)); }
    var allocatedBefore = GC.GetTotalAllocatedBytes(true);
    var clock = System.Diagnostics.Stopwatch.StartNew();
    for (var index = 0; index < iterations; index++)
    {
        _ = await service.GetDisplayFrameAsync(new PlaybackSnapshot(true, TimeSpan.FromMilliseconds(1500 + index % 4000), track));
    }

    clock.Stop();
    Console.WriteLine($"allocated-bytes-per-frame={(GC.GetTotalAllocatedBytes(true) - allocatedBefore) / (double)iterations:0.0}");
    Console.WriteLine(
        $"frames/s={iterations / clock.Elapsed.TotalSeconds:0} per-frame={clock.Elapsed.TotalMilliseconds * 1000 / iterations:0.0}us");

    service.Dispose();
    return 0;
}

// Minimal coordinator: the frame sweep feeds lyrics directly, so the search returns nothing.
file sealed class NoLyricsCoordinator : ILyricResolutionCoordinator
{
    public Task<ResolvedLyrics?> ResolveAsync(TrackInfo track, CancellationToken cancellationToken = default)
        => Task.FromResult<ResolvedLyrics?>(null);

    public void Dispose()
    {
    }
}
