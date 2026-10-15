using System.Runtime.InteropServices;
using System.Text.Json;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Utilities;

namespace TaskbarLyrics.Core;

/// <summary>
/// P/Invoke bridge to the Rust lyric engine (<c>Rust/crates/lyric-ffi</c>).
/// </summary>
/// <remarks>
/// <para>
/// The Rust engine is a drop-in re-implementation of the lyric core: parsing (LRC / QRC /
/// KRC / YRC / TTML), QRC + KRC decryption, single/batch identity matching and the local
/// file provider. When <see cref="IsActive"/> is true the lyric pipeline calls into Rust
/// first and only falls back to the C# implementation if the native call fails.
/// </para>
/// <para>
/// Native library resolution order:
/// <list type="number">
///   <item><description>the application directory (where <c>dotnet build/publish</c> copies it),</description></item>
///   <item><description><c>%LOCALAPPDATA%\LyricIsland\Runtime\&lt;version&gt;</c> (the share-version extraction folder),</description></item>
///   <item><description><c>Rust\artifacts</c> / <c>Rust\target\release</c> next to the source tree (developer runs).</description></item>
/// </list>
/// </para>
/// <para>
/// Set the environment variable <c>LYRICISLAND_LYRIC_ENGINE=csharp</c> to force the
/// original C# implementation (useful for A/B comparison and for bug triage).
/// </para>
/// </remarks>
public static class RustLyricEngine
{
    private const string NativeLibraryName = "lyric_ffi";
    private const string EngineEnvironmentVariable = "LYRICISLAND_LYRIC_ENGINE";
    private const string FallbackDirectoryName = "Runtime";

    private static readonly Lazy<EngineState> State = new(Probe, isThreadSafe: true);
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNameCaseInsensitive = true
    };

    /// <summary>Library loaded and usable.</summary>
    public static bool IsActive => State.Value.IsUsable;

    /// <summary>Release name reported by the Rust engine (for example <c>0.7.8.1</c>).</summary>
    public static string? Version => State.Value.Version;

    /// <summary>Human readable state, for logs and for the settings "about" page.</summary>
    public static string Describe()
    {
        var state = State.Value;
        if (state.IsUsable)
        {
            return $"rust {state.Version} ({state.LoadedFrom})";
        }

        return state.Detail is null
            ? "csharp (rust engine disabled)"
            : $"csharp (rust engine unavailable: {state.Detail})";
    }

    /// <summary>
    /// <c>LyricMatcher.Score</c> through the Rust engine. Returns <c>null</c> when the engine
    /// is inactive so callers keep the original implementation.
    /// </summary>
    public static int? Score(
        TrackInfo target,
        string resultTitle,
        string resultArtist,
        int resultDurationInSeconds,
        string? resultAlbum)
    {
        if (!IsActive)
        {
            return null;
        }

        try
        {
            return NativeMethods.li_match_score(
                target.Title ?? string.Empty,
                target.Artist ?? string.Empty,
                target.Album ?? string.Empty,
                (int)Math.Round(target.Duration.TotalSeconds),
                resultTitle ?? string.Empty,
                resultArtist ?? string.Empty,
                resultDurationInSeconds,
                resultAlbum ?? string.Empty);
        }
        catch (Exception exception) when (exception is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException)
        {
            return null;
        }
    }

    /// <summary>
    /// Parses (and, when needed, decrypts) a payload with the Rust engine.
    /// Returns <c>null</c> when the engine is inactive or the payload is rejected.
    /// </summary>
    public static ParsedLyrics? TryParse(DecodedLyricPayload payload)
    {
        if (!IsActive || payload is null)
        {
            return null;
        }

        var json = InvokeUtf8(() => NativeMethods.li_parse_payload(
            payload.Format.ToString(),
            payload.OriginalLyrics ?? string.Empty,
            payload.TranslationLyrics ?? string.Empty,
            payload.IsPureMusic ? 1 : 0,
            SerializeDiagnostics(payload.Diagnostics)));

        if (json is null)
        {
            return null;
        }

        var envelope = ParseEnvelope<ParsedLyricsPayload>(json);
        return envelope is null ? null : ToParsedLyrics(envelope);
    }

    /// <summary>Decrypts an encrypted QRC/KRC payload with the Rust engine.</summary>
    public static string? TryDecrypt(LyricPayloadFormat format, string encryptedLyrics)
    {
        if (!IsActive || string.IsNullOrWhiteSpace(encryptedLyrics))
        {
            return null;
        }

        if (format is not (LyricPayloadFormat.Qrc or LyricPayloadFormat.Krc))
        {
            return null;
        }

        var json = InvokeUtf8(() => NativeMethods.li_decrypt(format.ToString(), encryptedLyrics));
        if (json is null)
        {
            return null;
        }

        // li_decrypt answers with {"ok":"<plain lyrics>"}, so the payload is a plain string.
        return ParseEnvelope<string>(json);
    }

    /// <summary>
    /// Builds the search plan (query variants) with the Rust planner
    /// (<c>LyricSearchPlanner.CreatePlan</c>). Returns <c>null</c> when the engine is
    /// inactive or the plan was rejected, so the caller keeps the C# implementation.
    /// </summary>
    public static LyricSearchPlan? TryPlanSearch(TrackIdentity identity)
    {
        if (!IsActive || identity is null)
        {
            return null;
        }

        var json = InvokeUtf8(() => NativeMethods.li_plan_search(SerializeIdentity(identity)));
        if (json is null)
        {
            return null;
        }

        var plan = ParseEnvelope<PlanPayload>(json);
        if (plan?.OriginalTrack is null || plan.Variants is null)
        {
            return null;
        }

        try
        {
            var variants = new List<SearchQueryVariant>(plan.Variants.Count);
            foreach (var variant in plan.Variants)
            {
                variants.Add(new SearchQueryVariant(
                    variant.Id ?? string.Empty,
                    variant.Title ?? string.Empty,
                    variant.Artists ?? [],
                    variant.Album ?? string.Empty,
                    TimeSpan.FromMilliseconds(variant.DurationMs),
                    variant.RelaxationReasons ?? []));
            }

            // The original planner returns the caller's own TrackIdentity instance, and the
            // shipped test suite asserts on that reference identity, so reuse the input when
            // the round-tripped copy is field-equal to it.
            var originalTrack = ToIdentity(plan.OriginalTrack);
            return new LyricSearchPlan(
                IdentitiesEqual(originalTrack, identity) ? identity : originalTrack,
                variants);
        }
        catch (ArgumentException exception)
        {
            Log.Warn($"Rust search plan rejected: {exception.Message}");
            return null;
        }
    }

    private static bool IdentitiesEqual(TrackIdentity left, TrackIdentity right) =>
        string.Equals(left.TrackId, right.TrackId, StringComparison.Ordinal) &&
        string.Equals(left.Title, right.Title, StringComparison.Ordinal) &&
        string.Equals(left.Album, right.Album, StringComparison.Ordinal) &&
        string.Equals(left.SourceApp, right.SourceApp, StringComparison.Ordinal) &&
        string.Equals(left.SongId, right.SongId, StringComparison.Ordinal) &&
        left.Duration == right.Duration &&
        left.Artists.SequenceEqual(right.Artists, StringComparer.Ordinal) &&
        left.VersionMarkers.SequenceEqual(right.VersionMarkers, StringComparer.Ordinal);

    /// <summary>
    /// Scores and admits a candidate with the Rust evaluator
    /// (<c>LyricIdentityEvaluator.Evaluate</c>).
    /// </summary>
    public static LyricCandidateEvaluation? TryEvaluateCandidate(
        TrackIdentity identity,
        SourceTrackCandidate candidate)
    {
        if (!IsActive || identity is null || candidate is null)
        {
            return null;
        }

        var candidateJson = JsonSerializer.Serialize(new
        {
            providerId = candidate.ProviderId.Value,
            candidateId = candidate.CandidateId,
            title = candidate.Title,
            artists = candidate.Artists?.ToArray() ?? [],
            album = candidate.Album,
            durationMs = (long)Math.Round(candidate.Duration.TotalMilliseconds),
            queryVariantId = candidate.QueryVariantId
        });

        var json = InvokeUtf8(() => NativeMethods.li_evaluate_candidate(
            SerializeIdentity(identity),
            candidateJson));

        if (json is null)
        {
            return null;
        }

        var evaluation = ParseEnvelope<EvaluationPayload>(json);
        return evaluation is null
            ? null
            : new LyricCandidateEvaluation(
                evaluation.Score,
                evaluation.IsAdmitted,
                evaluation.RejectionReasons ?? []);
    }

    /// <summary>
    /// Applies the resolution selection rule (<c>LyricResolutionCoordinator.SelectBestOutcome</c>)
    /// to already scored provider results. Returns the chosen provider and candidate id.
    /// </summary>
    public static (string ProviderId, string CandidateId)? TrySelectBest(
        IReadOnlyList<(string ProviderId, string CandidateId, int IdentityScore)> outcomes,
        IReadOnlyList<string> trustOrder)
    {
        if (!IsActive || outcomes.Count == 0)
        {
            return null;
        }

        string outcomesJson;
        string trustOrderJson;
        try
        {
            outcomesJson = JsonSerializer.Serialize(
                outcomes.Select(outcome => new
                {
                    providerId = outcome.ProviderId,
                    candidateId = outcome.CandidateId,
                    identityScore = outcome.IdentityScore
                }),
                SerializerOptions);
            trustOrderJson = JsonSerializer.Serialize(trustOrder, SerializerOptions);
        }
        catch (JsonException)
        {
            return null;
        }

        var json = InvokeUtf8(() => NativeMethods.li_select_best(outcomesJson, trustOrderJson));
        if (json is null)
        {
            return null;
        }

        var selection = ParseEnvelope<SelectionPayload>(json);
        if (selection is null || !selection.Found ||
            string.IsNullOrEmpty(selection.ProviderId) || selection.CandidateId is null)
        {
            return null;
        }

        return (selection.ProviderId, selection.CandidateId);
    }

    private static string SerializeIdentity(TrackIdentity identity) =>
        JsonSerializer.Serialize(new
        {
            trackId = identity.TrackId,
            title = identity.Title,
            artists = identity.Artists?.ToArray() ?? [],
            album = identity.Album,
            durationMs = (long)Math.Round(identity.Duration.TotalMilliseconds),
            sourceApp = identity.SourceApp,
            songId = identity.SongId,
            versionMarkers = identity.VersionMarkers?.ToArray() ?? []
        });

    private static TrackIdentity ToIdentity(IdentityPayload payload) =>
        new(
            payload.TrackId ?? string.Empty,
            payload.Title ?? string.Empty,
            payload.Artists ?? [],
            payload.Album ?? string.Empty,
            TimeSpan.FromMilliseconds(payload.DurationMs),
            payload.SourceApp ?? string.Empty,
            payload.SongId,
            payload.VersionMarkers ?? []);

    /// <summary>
    /// Scans the local lyric folders on a background task so the first lookup does not pay for
    /// the directory walk. Mirrors the original <c>LocalMediaIndex</c> background warm-up.
    /// </summary>
    public static void WarmLocalIndexInBackground(IReadOnlyList<string> folders)
    {
        if (!IsActive || folders.Count == 0)
        {
            return;
        }

        var payload = SerializeFolders(folders);
        Task.Run(() =>
        {
            try
            {
                var json = InvokeUtf8(() => NativeMethods.li_warm_local_index(payload));
                if (json is not null)
                {
                    Log.Debug($"Rust lyric engine local index warmed: {json}");
                }
            }
            catch (Exception exception)
            {
                Log.Warn($"Rust lyric engine local index warm-up failed: {exception.Message}");
            }
        });
    }

    /// <summary>Drops the cached local index so the next lookup rescans the folders.</summary>
    public static void InvalidateLocalIndex(IReadOnlyList<string>? folders = null)
    {
        if (!IsActive)
        {
            return;
        }

        var payload = folders is { Count: > 0 } ? SerializeFolders(folders) : string.Empty;
        _ = InvokeUtf8(() => NativeMethods.li_invalidate_local_index(payload));
    }

    /// <summary>
    /// Local sidecar lyrics from the Rust provider.
    /// </summary>
    /// <returns>
    /// <c>null</c> when the call failed technically (the caller should use the C# provider);
    /// otherwise a result whose <see cref="RustLocalLyricResult.Document"/> is <c>null</c> when
    /// Rust authoritatively found nothing.
    /// </returns>
    public static RustLocalLyricResult? TryGetLocalDocument(TrackInfo track, IReadOnlyList<string> folders)
    {
        if (!IsActive || track is null || folders.Count == 0)
        {
            return null;
        }

        var json = InvokeUtf8(() => NativeMethods.li_local_document(
            SerializeFolders(folders),
            track.Title ?? string.Empty,
            track.Artist ?? string.Empty,
            track.Album ?? string.Empty,
            (int)Math.Round(track.Duration.TotalSeconds)));

        if (json is null)
        {
            return null;
        }

        var envelope = ParseEnvelope<LocalDocumentEnvelope>(json);
        if (envelope is null)
        {
            return null;
        }

        return new RustLocalLyricResult(
            ToLyricDocument(envelope.Document),
            ParseEnum(envelope.Acquisition, LyricAcquisitionKind.NotFound),
            envelope.ElapsedMillis);
    }

    private static string SerializeFolders(IReadOnlyList<string> folders)
    {
        try
        {
            return JsonSerializer.Serialize(folders, SerializerOptions);
        }
        catch (JsonException)
        {
            return "[]";
        }
    }

    private static LyricDocument? ToLyricDocument(DocumentPayload? payload)
    {
        if (payload?.Lines is null || payload.Lines.Count == 0)
        {
            return null;
        }

        var lines = new List<LyricLine>(payload.Lines.Count);
        foreach (var line in payload.Lines)
        {
            List<LyricSyllable>? syllables = null;
            if (line.Syllables is { Count: > 0 })
            {
                syllables = new List<LyricSyllable>(line.Syllables.Count);
                foreach (var syllable in line.Syllables)
                {
                    syllables.Add(new LyricSyllable(
                        TimeSpan.FromMilliseconds(syllable.RelativeOffsetMs),
                        TimeSpan.FromMilliseconds(syllable.DurationMs),
                        syllable.Text));
                }
            }

            lines.Add(new LyricLine(
                TimeSpan.FromMilliseconds(line.TimestampMs),
                line.Text,
                line.Translation,
                syllables));
        }

        return new LyricDocument(lines, payload.BestScore, payload.IsPureMusic);
    }

    private static EngineState Probe()
    {
        var requested = Environment.GetEnvironmentVariable(EngineEnvironmentVariable);
        if (!string.IsNullOrWhiteSpace(requested) &&
            (requested.Equals("csharp", StringComparison.OrdinalIgnoreCase) ||
             requested.Equals("dotnet", StringComparison.OrdinalIgnoreCase) ||
             requested.Equals("off", StringComparison.OrdinalIgnoreCase) ||
             requested.Equals("0", StringComparison.OrdinalIgnoreCase)))
        {
            return new EngineState(false, null, null, $"{EngineEnvironmentVariable}={requested}");
        }

        var loadedFrom = string.Empty;
        try
        {
            NativeLibrary.SetDllImportResolver(
                typeof(RustLyricEngine).Assembly,
                (libraryName, assembly, searchPath) =>
                {
                    if (!string.Equals(libraryName, NativeLibraryName, StringComparison.OrdinalIgnoreCase))
                    {
                        return IntPtr.Zero;
                    }

                    foreach (var candidate in CandidatePaths())
                    {
                        if (File.Exists(candidate) &&
                            NativeLibrary.TryLoad(candidate, out var handle))
                        {
                            loadedFrom = candidate;
                            return handle;
                        }
                    }

                    return IntPtr.Zero;
                });

            var infoJson = InvokeUtf8(() => NativeMethods.li_build_info());
            var info = infoJson is null ? null : JsonSerializer.Deserialize<BuildInfo>(infoJson, SerializerOptions);
            if (info is null || string.IsNullOrWhiteSpace(info.Version))
            {
                return new EngineState(false, null, null, "li_build_info returned no version");
            }

            return new EngineState(true, info.Version, string.IsNullOrEmpty(loadedFrom) ? "default probe" : loadedFrom, null);
        }
        catch (Exception exception)
        {
            return new EngineState(false, null, null, $"{exception.GetType().Name}: {exception.Message}");
        }
    }

    private static IEnumerable<string> CandidatePaths()
    {
        var fileName = $"{NativeLibraryName}.dll";

        yield return Path.Combine(AppContext.BaseDirectory, fileName);

        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        if (!string.IsNullOrWhiteSpace(localAppData))
        {
            yield return Path.Combine(localAppData, "LyricIsland", FallbackDirectoryName, fileName);
        }

        // Developer runs: walk up from the binary looking for Rust\artifacts\lyric_ffi.dll.
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        for (var depth = 0; directory is not null && depth < 6; depth++, directory = directory.Parent)
        {
            yield return Path.Combine(directory.FullName, "Rust", "artifacts", fileName);
            yield return Path.Combine(directory.FullName, "Rust", "target", "release", fileName);
        }
    }

    /// <summary>Bridge helpers for <see cref="RustLyricSync"/> (same file, same rules).</summary>
    internal static string? InvokeUtf8ForHandle(Func<IntPtr> invoke) => InvokeUtf8(invoke);

    internal static T? ParseEnvelopeForHandle<T>(string json) => ParseEnvelope<T>(json);

    private static string? InvokeUtf8(Func<IntPtr> invoke)
    {
        var pointer = IntPtr.Zero;
        try
        {
            pointer = invoke();
            return pointer == IntPtr.Zero ? null : Marshal.PtrToStringUTF8(pointer);
        }
        catch (Exception exception) when (exception is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException)
        {
            return null;
        }
        finally
        {
            if (pointer != IntPtr.Zero)
            {
                // The engine owns the buffer; release it here.
                try
                {
                    NativeMethods.li_free_string(pointer);
                }
                catch (Exception exception) when (exception is DllNotFoundException or EntryPointNotFoundException)
                {
                    // Nothing to free when the library vanished mid-call.
                }
            }
        }
    }

    private static string SerializeDiagnostics(IReadOnlyDictionary<string, string>? diagnostics)
    {
        if (diagnostics is null || diagnostics.Count == 0)
        {
            return "{}";
        }

        try
        {
            return JsonSerializer.Serialize(diagnostics, SerializerOptions);
        }
        catch (JsonException)
        {
            return "{}";
        }
    }

    private static T? ParseEnvelope<T>(string json)    {
        try
        {
            using var document = JsonDocument.Parse(json);
            if (document.RootElement.TryGetProperty("error", out _))
            {
                return default;
            }

            if (!document.RootElement.TryGetProperty("ok", out var ok))
            {
                return default;
            }

            return ok.Deserialize<T>(SerializerOptions);
        }
        catch (JsonException)
        {
            return default;
        }
    }

    private static ParsedLyrics ToParsedLyrics(ParsedLyricsPayload payload)
    {
        var lines = new List<ParsedLyricLine>();
        foreach (var line in payload.Lines ?? [])
        {
            var segments = new List<ParsedLyricSegment>();
            foreach (var segment in line.Segments ?? [])
            {
                segments.Add(new ParsedLyricSegment(
                    TimeSpan.FromMilliseconds(segment.StartMs),
                    TimeSpan.FromMilliseconds(segment.EndMs),
                    segment.Text));
            }

            lines.Add(new ParsedLyricLine(
                TimeSpan.FromMilliseconds(line.StartMs),
                line.EndMs is { } end ? TimeSpan.FromMilliseconds(end) : null,
                line.Text,
                line.Translation,
                segments,
                line.IsInformationLine));
        }

        return new ParsedLyrics(
            lines,
            ParseEnum(payload.TimingKind, LyricTimingKind.LineTimed),
            ParseEnum(payload.TimingProvenance, LyricTimingProvenance.Unknown),
            ParseEnum(payload.Format, LyricPayloadFormat.Unknown),
            payload.IsPureMusic);
    }

    private static TEnum ParseEnum<TEnum>(string? value, TEnum fallback)
        where TEnum : struct, Enum
        => Enum.TryParse<TEnum>(value, ignoreCase: true, out var parsed) ? parsed : fallback;

    private sealed record EngineState(bool IsUsable, string? Version, string? LoadedFrom, string? Detail);

    // -- wire format (camelCase, milliseconds, enum names) --------------------

    private sealed class IdentityPayload
    {
        public string? TrackId { get; set; }
        public string? Title { get; set; }
        public List<string>? Artists { get; set; }
        public string? Album { get; set; }
        public long DurationMs { get; set; }
        public string? SourceApp { get; set; }
        public string? SongId { get; set; }
        public List<string>? VersionMarkers { get; set; }
    }

    private sealed class VariantPayload
    {
        public string? Id { get; set; }
        public string? Title { get; set; }
        public List<string>? Artists { get; set; }
        public string? Album { get; set; }
        public long DurationMs { get; set; }
        public List<string>? RelaxationReasons { get; set; }
    }

    private sealed class PlanPayload
    {
        public IdentityPayload? OriginalTrack { get; set; }
        public List<VariantPayload>? Variants { get; set; }
    }

    private sealed class EvaluationPayload
    {
        public int Score { get; set; }
        public bool IsAdmitted { get; set; }
        public List<string>? RejectionReasons { get; set; }
    }

    /// <summary>Display-frame wire format; shared with <see cref="RustLyricSync"/>.</summary>
    internal sealed class FramePayload
    {
        public string? CurrentLine { get; set; }
        public string? NextLine { get; set; }
        public string? Title { get; set; }
        public double LineProgress { get; set; }
        public int CurrentLineIndex { get; set; } = -1;
        public bool IsPureMusic { get; set; }
        public double? WordScanProgress { get; set; }
        public string? CurrentTranslation { get; set; }
        public string? NextTranslation { get; set; }
        public bool HasTrackTranslation { get; set; }
    }

    private sealed class SelectionPayload
    {
        public bool Found { get; set; }
        public string? ProviderId { get; set; }
        public string? CandidateId { get; set; }
    }

    private sealed class LocalDocumentEnvelope
    {
        public bool Found { get; set; }
        public DocumentPayload? Document { get; set; }
        public int Score { get; set; }
        public string? Acquisition { get; set; }
        public long ElapsedMillis { get; set; }
    }

    private sealed class DocumentPayload
    {
        public List<DocumentLinePayload>? Lines { get; set; }
        public int BestScore { get; set; }
        public bool IsPureMusic { get; set; }
    }

    private sealed class DocumentLinePayload
    {
        public long TimestampMs { get; set; }
        public string Text { get; set; } = string.Empty;
        public string? Translation { get; set; }
        public List<DocumentSyllablePayload>? Syllables { get; set; }
    }

    private sealed class DocumentSyllablePayload
    {
        public long RelativeOffsetMs { get; set; }
        public long DurationMs { get; set; }
        public string Text { get; set; } = string.Empty;
    }

    // -- wire format (camelCase, milliseconds, enum names) --------------------

    private sealed class BuildInfo
    {
        public string? Version { get; set; }
        public string? CrateVersion { get; set; }
        public string? Compiler { get; set; }
        public int Abi { get; set; }
    }

    private sealed class ParsedLyricsPayload
    {
        public List<LinePayload>? Lines { get; set; }
        public string? TimingKind { get; set; }
        public string? TimingProvenance { get; set; }
        public string? Format { get; set; }
        public bool IsPureMusic { get; set; }
    }

    private sealed class LinePayload
    {
        public long StartMs { get; set; }
        public long? EndMs { get; set; }
        public string Text { get; set; } = string.Empty;
        public string? Translation { get; set; }
        public List<SegmentPayload>? Segments { get; set; }
        public bool IsInformationLine { get; set; }
    }

    private sealed class SegmentPayload
    {
        public long StartMs { get; set; }
        public long EndMs { get; set; }
        public string Text { get; set; } = string.Empty;
    }

    /// <summary>Raw P/Invoke surface; exposed to <see cref="RustLyricSync"/>.</summary>
    internal static class NativeMethods
    {
        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int li_scan_rip([MarshalAs(UnmanagedType.LPUTF8Str)] string path, ulong offset, ulong length, long sectionRva, [MarshalAs(UnmanagedType.LPUTF8Str)] string signature, uint displacement, out long target);
        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int li_sync_frame_v2(ulong handle, long positionMs, out NativeDisplayFrame frame);
        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_version();

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_build_info();

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void li_free_string(IntPtr pointer);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int li_match_score(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string title,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string artist,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string album,
            int durationSeconds,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateTitle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateArtist,
            int candidateDurationSeconds,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateAlbum);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_parse_payload(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string format,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string original,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string translation,
            int isPureMusic,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string diagnosticsJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_decrypt(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string format,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string encrypted);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_plan_search(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string identityJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_evaluate_candidate(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string identityJson,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_select_best(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string outcomesJson,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string trustOrderJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_warm_local_index(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string dirsJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_invalidate_local_index(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string dirsJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_local_document(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string dirsJson,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string title,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string artist,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string album,
            int durationSeconds);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong li_sync_create();

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void li_sync_destroy(ulong handle);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_sync_begin_track(
            ulong handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string title,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string artist,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string album,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sourceApp,
            int durationSeconds,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string songId);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_sync_set_document(
            ulong handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string documentJson);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_sync_mark_not_found(ulong handle);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_sync_reset(ulong handle);

        [DllImport(NativeLibraryName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_sync_frame(ulong handle, long positionMs);
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeDisplayFrame
    {
        public double Progress, WordProgress;
        public int Index, PureMusic, HasWordProgress, HasTranslation;
        public ulong TextVersion;
        public IntPtr Current, Next, Title, Translation, NextTranslation;
    }
    public static long? ScanPlayerSignature(string path, long offset, int length, int sectionRva, string pattern, int displacement)
    {
        if (!IsActive) return null;
        try { return NativeMethods.li_scan_rip(path, (ulong)offset, (ulong)length, sectionRva, pattern, (uint)displacement, out var target) == 1 ? target : null; }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException) { return null; }
    }
}

/// <summary>
/// Handle-based bridge for the display-frame computation: the document and the loaded-at
/// timestamp stay in Rust, so a frame tick only carries the playback position.
/// </summary>
/// <remarks>
/// Mirrors the state the C# <c>LyricSyncService</c> keeps: call <see cref="BeginTrack"/> when
/// the visible track changes, <see cref="SetDocument"/> when lyrics arrive (or
/// <see cref="MarkNotFound"/>), then <see cref="Frame"/> once per frame. Every call returns
/// <c>null</c>/does nothing when the handle is gone, so the caller can fall back.
/// </remarks>
public sealed class RustLyricSync : IDisposable
{
    private ulong _handle;
    private bool _disposed;
    private readonly object _frameGate = new();
    private bool _binaryFrame = true;
    private ulong _textVersion;
    private string _current = string.Empty, _next = string.Empty, _title = string.Empty;
    private string? _translation, _nextTranslation;

    private RustLyricSync(ulong handle)
    {
        _handle = handle;
    }

    /// <summary>Creates a handle, or returns <c>null</c> when the engine is inactive.</summary>
    public static RustLyricSync? TryCreate()
    {
        if (!RustLyricEngine.IsActive)
        {
            return null;
        }

        try
        {
            var handle = RustLyricEngine.NativeMethods.li_sync_create();
            return handle == 0 ? null : new RustLyricSync(handle);
        }
        catch (Exception exception) when (exception is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException)
        {
            return null;
        }
    }

    public bool IsValid => !_disposed && _handle != 0;

    /// <summary>Registers the playing track; resets the internal document state on change.</summary>
    public bool BeginTrack(TrackInfo track)
    {
        if (!IsValid)
        {
            return false;
        }

        var json = RustLyricEngine.InvokeUtf8ForHandle(() => RustLyricEngine.NativeMethods.li_sync_begin_track(
            _handle,
            track.Title ?? string.Empty,
            track.Artist ?? string.Empty,
            track.Album ?? string.Empty,
            track.SourceApp ?? string.Empty,
            (int)Math.Round(track.Duration.TotalSeconds),
            track.SongId ?? string.Empty));

        return json is not null;
    }

    /// <summary>Installs the lyrics document; returns <c>true</c> when Rust accepted it.</summary>
    public bool SetDocument(LyricDocument document)
    {
        if (!IsValid || document.Lines.Count == 0)
        {
            return false;
        }

        string payload;
        try
        {
            payload = JsonSerializer.Serialize(new
            {
                lines = document.Lines.Select(line => new
                {
                    timestampMs = (long)Math.Round(line.Timestamp.TotalMilliseconds),
                    text = line.Text,
                    translation = line.Translation,
                    syllables = line.Syllables?.Select(syllable => new
                    {
                        relativeOffsetMs = (long)Math.Round(syllable.RelativeOffset.TotalMilliseconds),
                        durationMs = (long)Math.Round(syllable.Duration.TotalMilliseconds),
                        text = syllable.Text
                    }).ToArray()
                }).ToArray(),
                isPureMusic = document.IsPureMusic
            });
        }
        catch (JsonException)
        {
            return false;
        }

        var json = RustLyricEngine.InvokeUtf8ForHandle(
            () => RustLyricEngine.NativeMethods.li_sync_set_document(_handle, payload));
        return json is not null && json.Contains("\"applied\":true", StringComparison.Ordinal);
    }

    /// <summary>Marks the current track as "no lyrics found".</summary>
    public void MarkNotFound()
    {
        if (!IsValid)
        {
            return;
        }

        _ = RustLyricEngine.InvokeUtf8ForHandle(() => RustLyricEngine.NativeMethods.li_sync_mark_not_found(_handle));
    }

    /// <summary>Clears the handle (playback stopped / snapshot has no track).</summary>
    public void Reset()
    {
        if (!IsValid)
        {
            return;
        }

        _ = RustLyricEngine.InvokeUtf8ForHandle(() => RustLyricEngine.NativeMethods.li_sync_reset(_handle));
    }

    /// <summary>
    /// Computes the display frame at <paramref name="position"/> (already offset by the
    /// player and track lead times), or <c>null</c> when the call failed.
    /// </summary>
    public LyricDisplayFrame? Frame(TimeSpan position)
    {
        lock (_frameGate) return FrameCore(position);
    }
    private LyricDisplayFrame? FrameCore(TimeSpan position)
    {
        if (!IsValid)
        {
            return null;
        }

        if (_binaryFrame)
        {
            try
            {
                if (RustLyricEngine.NativeMethods.li_sync_frame_v2(_handle, (long)Math.Round(position.TotalMilliseconds), out var fast) == 1)
                {
                    if (_textVersion != fast.TextVersion)
                    {
                        _current = Marshal.PtrToStringUTF8(fast.Current) ?? string.Empty;
                        _next = Marshal.PtrToStringUTF8(fast.Next) ?? string.Empty;
                        _title = Marshal.PtrToStringUTF8(fast.Title) ?? string.Empty;
                        _translation = Marshal.PtrToStringUTF8(fast.Translation);
                        _nextTranslation = Marshal.PtrToStringUTF8(fast.NextTranslation);
                        _textVersion = fast.TextVersion;
                    }
                    return new LyricDisplayFrame(_current, _next, _title, fast.Progress, fast.Index, fast.PureMusic != 0,
                        fast.HasWordProgress != 0 ? fast.WordProgress : null, _translation, _nextTranslation, fast.HasTranslation != 0);
                }
            }
            catch (EntryPointNotFoundException) { _binaryFrame = false; }
        }
        var json = RustLyricEngine.InvokeUtf8ForHandle(() => RustLyricEngine.NativeMethods.li_sync_frame(
            _handle,
            (long)Math.Round(position.TotalMilliseconds)));

        if (json is null)
        {
            return null;
        }

        var frame = RustLyricEngine.ParseEnvelopeForHandle<RustLyricEngine.FramePayload>(json);
        return frame is null
            ? null
            : new LyricDisplayFrame(
                frame.CurrentLine ?? string.Empty,
                frame.NextLine ?? string.Empty,
                frame.Title ?? string.Empty,
                frame.LineProgress,
                frame.CurrentLineIndex,
                frame.IsPureMusic,
                frame.WordScanProgress,
                frame.CurrentTranslation,
                frame.NextTranslation,
                frame.HasTrackTranslation);
    }

    public void Dispose()
    {
        lock (_frameGate) DisposeCore();
    }
    private void DisposeCore()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        if (_handle == 0)
        {
            return;
        }

        try
        {
            RustLyricEngine.NativeMethods.li_sync_destroy(_handle);
        }
        catch (Exception exception) when (exception is DllNotFoundException or EntryPointNotFoundException)
        {
            // Nothing to release when the library vanished.
        }

        _handle = 0;
    }
}

/// <summary>Result of a local sidecar-lyrics lookup performed by the Rust engine.</summary>
/// <param name="Document">Resolved document, or <c>null</c> when nothing matched.</param>
/// <param name="Acquisition">Acquisition kind reported by the engine.</param>
/// <param name="ElapsedMilliseconds">Time the engine spent on the lookup.</param>
public sealed record RustLocalLyricResult(
    LyricDocument? Document,
    LyricAcquisitionKind Acquisition,
    long ElapsedMilliseconds);
