//! `lyric-island` — command line front-end for the Rust lyric core (LyricIsland 0.7.8.1).
//!
//! Available commands:
//!
//! ```text
//! lyric-island parse <file> [--format auto|lrc|qrc|krc|yrc|ttml|plain] [--json]
//! lyric-island match --title T --artist A [--duration S] [--candidate-title T] [...]
//! lyric-island resolve --title T --artist A [--dir D]... [--json]
//! lyric-island frame <file> --at SECONDS [--format auto] [--json]
//! lyric-island cache-clear [--data-dir DIR]
//! lyric-island version
//! ```

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Duration;

use lyric_core::cache::{JsonResolvedLyricCache, LyricPipelineCache};
use lyric_core::formats;
use lyric_core::local::LocalLyricProvider;
use lyric_core::matching::LyricMatcher;
use lyric_core::models::{LyricPayloadFormat, LyricProviderId, TrackInfo};
use lyric_core::payload::LyricifyPayloadParser;
use lyric_core::resolution::{LyricResolutionCoordinator, LyricResolutionCoordinatorBuilder};
use lyric_core::sync::LyricSyncService;
use lyric_core::{DatabasePaths, VERSION};

/// Broken-pipe tolerant stdout writer.
///
/// Rust panics on a closed stdout (e.g. `lyric-island resolve ... | head -5`). A CLI that
/// is routinely piped should exit quietly instead.
macro_rules! outln {
    ($($arg:tt)*) => {
        print_line(&format!($($arg)*))
    };
}

fn print_line(text: &str) {
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    if handle.write_all(text.as_bytes()).is_err() || handle.write_all(b"\n").is_err() {
        std::process::exit(0);
    }
}

/// Writes a prompt without the trailing newline and flushes it.
fn prompt(label: &str) -> String {
    use std::io::{BufRead, Write};

    let stdout = std::io::stdout();
    {
        let mut handle = stdout.lock();
        let _ = handle.write_all(format!("{label}: ").as_bytes());
        let _ = handle.flush();
    }

    let stdin = std::io::stdin();
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => String::new(),
        Ok(_) => line.trim().to_string(),
        Err(_) => String::new(),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // A console program that is *double clicked* runs with no arguments and closes as soon
    // as it exits, so the help text is never readable. When stdin is an interactive
    // console, fall into the menu instead; `--menu` forces it (useful in scripts).
    if args.is_empty() || args[0] == "--menu" || args[0] == "-i" || args[0] == "--interactive" {
        let forced = !args.is_empty();
        if forced || std::io::stdin().is_terminal() {
            return interactive_menu();
        }
        print_usage();
        return ExitCode::SUCCESS;
    }

    dispatch(&args)
}

fn dispatch(args: &[String]) -> ExitCode {
    let command = args[0].as_str();
    let rest = &args[1..];

    let result = match command {
        "parse" => command_parse(rest),
        "match" => command_match(rest),
        "resolve" => command_resolve(rest),
        "frame" => command_frame(rest),
        "cache-clear" => command_cache_clear(rest),
        "version" | "--version" | "-V" => {
            outln!("LyricIsland Rust core {VERSION}");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        "menu" => Ok(()),
        other => Err(format!("unknown command '{other}'")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Menu shown when the program is started without arguments from a console.
fn interactive_menu() -> ExitCode {
    loop {
        outln!("");
        outln!("LyricIsland {VERSION} — Rust 歌词内核（重制版）");
        outln!("==========================================");
        outln!("  1) 解析歌词文件（LRC / QRC / KRC / YRC / TTML）");
        outln!("  2) 候选身份打分（判断某条结果是否可信）");
        outln!("  3) 从本地目录解析歌词");
        outln!("  4) 查看某一时刻的显示帧");
        outln!("  5) 清空缓存");
        outln!("  6) 命令行用法");
        outln!("  0) 退出");
        outln!("");

        let choice = prompt("请选择 (0-6)");
        outln!("");

        match choice.as_str() {
            "1" => {
                let path = prompt("歌词文件路径（可直接把文件拖进窗口）");
                let path = path.trim_matches('"').trim();
                if !path.is_empty() {
                    run_action(command_parse(&[path.to_string()]));
                }
            }
            "2" => {
                let title = prompt("歌名");
                let artist = prompt("歌手");
                let duration = prompt("时长（秒，可留空）");
                let candidate_title = prompt("候选歌名（留空 = 与歌名相同）");
                let candidate_artist = prompt("候选歌手（留空 = 与歌手相同）");
                let candidate_duration = prompt("候选时长（秒，可留空）");

                if !title.trim().is_empty() {
                    let mut args = vec!["--title".to_string(), title.clone()];
                    if !artist.trim().is_empty() {
                        args.push("--artist".to_string());
                        args.push(artist.clone());
                    }
                    if !duration.trim().is_empty() {
                        args.push("--duration".to_string());
                        args.push(duration.clone());
                    }
                    if !candidate_title.trim().is_empty() {
                        args.push("--candidate-title".to_string());
                        args.push(candidate_title.clone());
                    }
                    if !candidate_artist.trim().is_empty() {
                        args.push("--candidate-artist".to_string());
                        args.push(candidate_artist.clone());
                    }
                    if !candidate_duration.trim().is_empty() {
                        args.push("--candidate-duration".to_string());
                        args.push(candidate_duration.clone());
                    }
                    run_action(command_match(&args));
                }
            }
            "3" => {
                let title = prompt("歌名");
                let artist = prompt("歌手");
                let duration = prompt("时长（秒，可留空）");
                let directory = prompt("歌词目录（可直接把文件夹拖进窗口）");
                let directory = directory.trim_matches('"').trim();

                if !title.trim().is_empty() && !directory.is_empty() {
                    let mut args = vec!["--title".to_string(), title.clone(), "--dir".to_string(), directory.to_string()];
                    if !artist.trim().is_empty() {
                        args.push("--artist".to_string());
                        args.push(artist.clone());
                    }
                    if !duration.trim().is_empty() {
                        args.push("--duration".to_string());
                        args.push(duration.clone());
                    }
                    run_action(command_resolve(&args));
                }
            }
            "4" => {
                let path = prompt("歌词文件路径（可直接把文件拖进窗口）");
                let path = path.trim_matches('"').trim();
                let at = prompt("时刻（秒，例如 3.6）");
                if !path.is_empty() && !at.trim().is_empty() {
                    run_action(command_frame(&[path.to_string(), "--at".to_string(), at.clone()]));
                }
            }
            "5" => {
                run_action(command_cache_clear(&[]));
            }
            "6" => print_usage(),
            "0" | "q" | "quit" | "exit" => return ExitCode::SUCCESS,
            "" => return ExitCode::SUCCESS,
            other => outln!("无效选择：{other}"),
        }

        if prompt("按回车键返回菜单").is_empty() && !std::io::stdin().is_terminal() {
            return ExitCode::SUCCESS;
        }
    }
}

fn run_action(result: Result<(), String>) {
    if let Err(message) = result {
        outln!("");
        outln!("[失败] {message}");
    }
}

fn print_usage() {
    outln!("LyricIsland {VERSION} — Rust lyric core");
    outln!("");
    outln!("不带参数启动（或双击 exe）会进入交互菜单；--menu 可强制进入。");
    outln!("");
    outln!("usage:");
    outln!("  lyric-island parse <file> [--format auto|lrc|qrc|krc|yrc|ttml|plain] [--raw] [--json]");
    outln!("  lyric-island match --title T --artist A [--duration S] [--candidate-title T]");
    outln!("                     [--candidate-artist A] [--candidate-duration S] [--candidate-album AL]");
    outln!("  lyric-island resolve --title T --artist A [--album AL] [--duration S]");
    outln!("                       [--track-id ID] [--source-app APP] [--dir DIR]... [--no-cache] [--json]");
    outln!("  lyric-island frame <file> --at SECONDS [--format auto] [--json]");
    outln!("  lyric-island cache-clear [--data-dir DIR]");
    outln!("  lyric-island version");
    outln!("  lyric-island --menu            # 交互菜单（双击 exe 时自动进入）");
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

struct Args {
    positional: Vec<String>,
    flags: BTreeMap<String, Vec<String>>,
}

impl Args {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut positional = Vec::new();
        let mut flags: BTreeMap<String, Vec<String>> = BTreeMap::new();

        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            if let Some(name) = arg.strip_prefix("--") {
                let value = args.get(index + 1);
                match value {
                    Some(value) if !value.starts_with("--") => {
                        flags.entry(name.to_string()).or_default().push(value.clone());
                        index += 2;
                    }
                    _ => {
                        flags.entry(name.to_string()).or_default();
                        index += 1;
                    }
                }
            } else if arg.starts_with('-') && arg.len() > 1 {
                flags.entry(arg.trim_start_matches('-').to_string()).or_default();
                index += 1;
            } else {
                positional.push(arg.clone());
                index += 1;
            }
        }

        Ok(Self { positional, flags })
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|values| values.last()).map(String::as_str)
    }

    fn many(&self, name: &str) -> Vec<String> {
        self.flags.get(name).cloned().unwrap_or_default()
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn required(&self, name: &str) -> Result<&str, String> {
        self.flag(name)
            .ok_or_else(|| format!("missing required --{name}"))
    }

    fn json(&self) -> bool {
        self.has("json")
    }
}

fn parse_format(value: Option<&str>, path: Option<&str>) -> LyricPayloadFormat {
    match value.map(|value| value.to_lowercase()).as_deref() {
        Some("lrc") => LyricPayloadFormat::Lrc,
        Some("qrc") => LyricPayloadFormat::Qrc,
        Some("krc") => LyricPayloadFormat::Krc,
        Some("yrc") => LyricPayloadFormat::Yrc,
        Some("ttml") => LyricPayloadFormat::Ttml,
        Some("plain") | Some("plaintext") => LyricPayloadFormat::PlainText,
        _ => detect_format(path.map(|path| path.to_lowercase()).as_deref().unwrap_or("")),
    }
}

fn detect_format(path: &str) -> LyricPayloadFormat {
    if path.ends_with(".qrc") {
        LyricPayloadFormat::Qrc
    } else if path.ends_with(".krc") {
        LyricPayloadFormat::Krc
    } else if path.ends_with(".yrc") {
        LyricPayloadFormat::Yrc
    } else if path.ends_with(".ttml") {
        LyricPayloadFormat::Ttml
    } else {
        LyricPayloadFormat::Lrc
    }
}

fn decode_payload(format: LyricPayloadFormat, content: &str) -> String {
    match format {
        LyricPayloadFormat::Qrc if !content.trim_start().starts_with(['[', '{']) => {
            lyric_core::crypto::decrypt_qrc(content).unwrap_or_else(|_| content.to_string())
        }
        LyricPayloadFormat::Krc if !content.trim_start().starts_with('[') => {
            lyric_core::crypto::decrypt_krc(content).unwrap_or_else(|_| content.to_string())
        }
        _ => content.to_string(),
    }
}

// ---------------------------------------------------------------------------
// parse
// ---------------------------------------------------------------------------

fn command_parse(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let path = args
        .positional
        .first()
        .ok_or_else(|| "parse requires a file path".to_string())?;

    let raw = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let format = parse_format(args.flag("format"), Some(path));
    let content = decode_payload(format, &raw);

    // `--raw` shows the low level format parser output; the default goes through the
    // same pipeline parser the application uses.
    if args.has("raw") {
        let infos = match format {
            LyricPayloadFormat::Qrc => formats::parse_qrc(&content),
            LyricPayloadFormat::Krc => formats::parse_krc(&content),
            LyricPayloadFormat::Yrc => formats::parse_yrc(&content),
            LyricPayloadFormat::Ttml => formats::parse_ttml(&content),
            LyricPayloadFormat::PlainText => formats::parse_plain_text(&content),
            _ => formats::parse_lrc(&content),
        };

        if args.json() {
            outln!(
                "{}",
                serde_json::to_string_pretty(&infos).unwrap_or_else(|_| "[]".to_string())
            );
            return Ok(());
        }

        outln!("{}: {} raw line(s), format {format:?}", path, infos.len());
        for info in &infos {
            outln!(
                "  {:>8}  {}{}{}",
                info.start_time
                    .map(|value| format!("{:8.3}", value as f64 / 1000.0))
                    .unwrap_or_else(|| "    ---".to_string()),
                if info.is_information_line { "[info] " } else { "" },
                info.text,
                if info.syllables.is_empty() {
                    String::new()
                } else {
                    format!("   ({} syllable(s))", info.syllables.len())
                }
            );
        }
        return Ok(());
    }

    let payload = lyric_core::models::DecodedLyricPayload {
        provider_id: LyricProviderId::try_new("Local").expect("constant provider id"),
        candidate_id: format!("cli-{}", format.as_str()),
        format,
        original_lyrics: Some(content),
        translation_lyrics: None,
        is_pure_music: false,
        diagnostics: Default::default(),
    };

    let parsed = LyricifyPayloadParser::parse(&payload).map_err(|error| error.to_string())?;

    if args.json() {
        outln!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| "{}".to_string())
        );
        return Ok(());
    }

    outln!(
        "{}: {} line(s), format {:?}, timing {:?} / {:?}{}",
        path,
        parsed.lines.len(),
        parsed.format,
        parsed.timing_kind,
        parsed.timing_provenance,
        if parsed.is_pure_music { ", pure music" } else { "" }
    );
    for line in &parsed.lines {
        outln!(
            "  {:>8.3}  {}{}{}",
            line.start_time.as_secs_f64(),
            if line.is_information_line { "[info] " } else { "" },
            line.text,
            line.translation
                .as_ref()
                .map(|translation| format!("   / {translation}"))
                .unwrap_or_default()
        );
        for segment in &line.segments {
            outln!(
                "            {:.3} → {:.3}  {}",
                segment.start_time.as_secs_f64(),
                segment.end_time.as_secs_f64(),
                segment.text
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// match
// ---------------------------------------------------------------------------

fn command_match(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let title = args.required("title")?;
    let artist = args.flag("artist").unwrap_or("");
    let duration: u64 = args.flag("duration").unwrap_or("0").parse().unwrap_or(0);

    let track = TrackInfo::new(
        "cli-track",
        title,
        artist,
        args.flag("album").unwrap_or(""),
        args.flag("source-app").unwrap_or("Spotify"),
        Duration::from_secs(duration),
    );

    let candidate_title = args.flag("candidate-title").unwrap_or(title);
    let candidate_artist = args.flag("candidate-artist").unwrap_or(artist);
    let candidate_duration: i32 = args
        .flag("candidate-duration")
        .map(|value| value.parse().unwrap_or(duration as i32))
        .unwrap_or(duration as i32);
    let candidate_album = args.flag("candidate-album");

    let score = LyricMatcher::score(
        &track,
        candidate_title,
        candidate_artist,
        candidate_duration,
        candidate_album,
    );

    outln!("identity score: {score}");
    outln!(
        "  admission: {}",
        if score >= lyric_core::policy::LyricMatchingPolicy::MINIMUM_ACCEPTED_MATCH_SCORE {
            "admitted"
        } else {
            "rejected"
        }
    );
    outln!(
        "  immediate acceptance: {}",
        score >= lyric_core::policy::LyricMatchingPolicy::IMMEDIATE_ACCEPTANCE_SCORE
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

fn command_resolve(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let title = args.required("title")?.to_string();
    let artist = args.flag("artist").unwrap_or("").to_string();
    let album = args.flag("album").unwrap_or("").to_string();
    let duration: u64 = args.flag("duration").unwrap_or("0").parse().unwrap_or(0);
    let source_app = args.flag("source-app").unwrap_or("Spotify").to_string();

    let mut track = TrackInfo::new(
        args.flag("track-id").unwrap_or("cli-track"),
        title,
        artist,
        album,
        source_app,
        Duration::from_secs(duration),
    );
    if let Some(song_id) = args.flag("song-id") {
        track = track.with_song_id(song_id);
    }

    let directories = args.many("dir");
    let local_provider = if directories.is_empty() {
        None
    } else {
        Some(LocalLyricProvider::new(directories))
    };

    let data_directory = args
        .flag("data-dir")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| lyric_core::default_data_directory().join("database"));
    let paths = DatabasePaths {
        raw: data_directory.join("lyric-pipeline-raw-v1.json"),
        parsed: data_directory.join("lyric-pipeline-parsed-v1.json"),
        resolved: data_directory.join(JsonResolvedLyricCache::DEFAULT_FILE_NAME),
    };

    let use_cache = !args.has("no-cache");
    // This build ships no online provider implementations (see PORTING-NOTES.md), so the
    // trust order is empty and resolution relies on the local file provider.
    let trust_policy = lyric_core::policy::LyricProviderTrustPolicy::new(Vec::new(), Vec::new())
        .map_err(|error| error.to_string())?;

    let coordinator = LyricResolutionCoordinator::new(LyricResolutionCoordinatorBuilder {
        sources: Vec::new(),
        trust_policy: Some(trust_policy),
        pipeline_cache: use_cache.then(|| LyricPipelineCache::with_paths(&paths.raw, &paths.parsed)),
        local_provider,
        resolved_cache: use_cache.then(|| JsonResolvedLyricCache::new(&paths.resolved)),
        ..Default::default()
    })
    .map_err(|error| error.to_string())?;

    let result = coordinator.resolve(&track);

    if args.json() {
        outln!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
        );
        return Ok(());
    }

    outln!("status: {:?}", result.status);
    let Some(lyrics) = result.lyrics else {
        outln!("no lyrics resolved");
        return Ok(());
    };

    outln!("provider: {}", lyrics.provider_id);
    outln!("candidate: {}", lyrics.candidate_id);
    outln!("acquisition: {:?}", lyrics.acquisition);
    outln!("format: {:?}", lyrics.content.format);
    outln!("timing: {:?} / {:?}", lyrics.content.timing_kind, lyrics.content.timing_provenance);
    outln!("lines: {}", lyrics.content.lines.len());
    if let Some(score) = lyrics.identity_score() {
        outln!("identity score: {score}");
    }
    for line in lyrics.content.lines.iter().take(10) {
        outln!(
            "  {:>8.3}  {}{}",
            line.start_time.as_secs_f64(),
            line.text,
            line.translation
                .as_ref()
                .map(|translation| format!("   / {translation}"))
                .unwrap_or_default()
        );
    }
    if lyrics.content.lines.len() > 10 {
        outln!("  ... {} more line(s)", lyrics.content.lines.len() - 10);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// frame
// ---------------------------------------------------------------------------

fn command_frame(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let path = args
        .positional
        .first()
        .ok_or_else(|| "frame requires a file path".to_string())?;
    let at: f64 = args
        .required("at")?
        .parse()
        .map_err(|_| "--at must be a number of seconds".to_string())?;

    let raw = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let format = parse_format(args.flag("format"), Some(path));
    let content = decode_payload(format, &raw);

    let track = TrackInfo::new(
        "cli-track",
        path.to_string(),
        "cli",
        "",
        "Local",
        Duration::from_secs_f64(at.max(1.0)),
    );

    let mut service = LyricSyncService::new();
    service.begin_track(&track);

    let decoded = lyric_core::models::DecodedLyricPayload {
        provider_id: LyricProviderId::try_new("Local").expect("constant provider id"),
        candidate_id: "cli".to_string(),
        format,
        original_lyrics: Some(content),
        translation_lyrics: None,
        is_pure_music: false,
        diagnostics: Default::default(),
    };
    let parsed = LyricifyPayloadParser::parse(&decoded).map_err(|error| error.to_string())?;

    let resolved = lyric_core::models::ResolvedLyrics::try_new(
        parsed,
        LyricProviderId::try_new("Local").expect("constant provider id"),
        "cli",
        lyric_core::models::LyricAcquisitionKind::LocalFile,
        Default::default(),
    )
    .map_err(|error| error.to_string())?;

    service.complete_search(&track, Some(&resolved));

    let snapshot = lyric_core::models::PlaybackSnapshot::playing(
        track.clone(),
        Duration::from_secs_f64(at.max(0.0)),
    );
    let frame = service.get_display_frame(&snapshot);

    if args.json() {
        outln!(
            "{}",
            serde_json::to_string_pretty(&frame).unwrap_or_else(|_| "{}".to_string())
        );
        return Ok(());
    }

    outln!("at {at:.3}s");
    outln!("  current   : {}", frame.current_line);
    if let Some(translation) = frame.current_translation.as_deref() {
        outln!("  translation: {translation}");
    }
    outln!("  next      : {}", frame.next_line);
    outln!("  line index: {}", frame.current_line_index);
    outln!("  progress  : {:.1}%", frame.line_progress * 100.0);
    if let Some(scan) = frame.word_scan_progress {
        outln!("  word scan : {:.1}%", scan * 100.0);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// cache-clear
// ---------------------------------------------------------------------------

fn command_cache_clear(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let data_directory = args
        .flag("data-dir")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| lyric_core::default_data_directory().join("database"));

    let paths = DatabasePaths {
        raw: data_directory.join("lyric-pipeline-raw-v1.json"),
        parsed: data_directory.join("lyric-pipeline-parsed-v1.json"),
        resolved: data_directory.join(JsonResolvedLyricCache::DEFAULT_FILE_NAME),
    };

    LyricPipelineCache::with_paths(&paths.raw, &paths.parsed).clear();
    JsonResolvedLyricCache::new(&paths.resolved).clear();

    outln!("cleared caches under {}", data_directory.display());
    Ok(())
}
