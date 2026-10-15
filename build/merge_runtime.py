"""Compose the shipped runtime directory for Lyric Island 1.0.

Layout of 1.0 is a WPF capsule + a GPUI settings app, not the old dual-.NET/WinUI merge:

  publish/app-10/            self-contained WPF publish (the only .NET tree)
  <cargo target>/release/
    lyric-island-settings.exe   GPUI settings UI  -> shipped as LyricIsland.Settings.exe
  e_sqlite3.dll              x64 native SQLite used by the Rust backend to read song_maps.db

The old script merged app-final + settings-final (two .NET directories) and carried
WinUI/WindowsAppSDK files. Those are gone: the GPUI settings UI is a Rust binary with no
.NET directory of its own, and the app project no longer references TaskbarLyrics.Core, so
e_sqlite3.dll has to be placed here explicitly.

Every copy is checked, and the result is asserted before the packaging step sees it.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import sys

root = Path(__file__).resolve().parents[1]
app = root / 'publish' / 'app-10'
dst = root / 'publish' / 'LyricIsland-1.0'

target = Path(os.environ.get('CARGO_TARGET_DIR') or (root.parent / 'rust-target-lld-08'))
settings_exe = target / 'release' / 'lyric-island-settings.exe'

# WIN-x64 native SQLite from the pinned NuGet package. Verified byte-identical to the
# e_sqlite3.dll shipped with 0.7.8.3, so upgrading users keep the same engine.
SQLITE_PACKAGE = 'sqlitepclraw.lib.e_sqlite3'
SQLITE_VERSION = '2.1.6'
sqlite = (Path(os.environ.get('NUGET_PACKAGES') or (Path.home() / '.nuget' / 'packages'))
          / SQLITE_PACKAGE.lower() / SQLITE_VERSION / 'runtimes' / 'win-x64' / 'native' / 'e_sqlite3.dll')

# The complete OPPO Sans face; a truncated copy still renders, so pin the digest.
FONT_SHA256 = '6C7D5864C661516E1F400D9F21E4297F2E2A0719909691E29607CC4EF484A9F4'
FONT_RELATIVE = Path('Assets') / 'Fonts' / 'OPPOSans.ttf'

REQUIRED = [
    'LyricIsland.exe',
    'LyricIsland.dll',
    'LyricIsland.runtimeconfig.json',
    'LyricIsland.Settings.exe',
    'lyric_ffi.dll',
    'e_sqlite3.dll',
    'coreclr.dll',
    'PresentationFramework.dll',
    'WindowsBase.dll',
    'WinRT.Runtime.dll',
    'Microsoft.Windows.SDK.NET.dll',
    str(FONT_RELATIVE),
    'Assets/LyricIsland-v2.ico',
]

# GNU runtime DLLs are build-tool-only: PE imports of both shipped binaries were checked and
# neither imports them. Shipping them would just be dead weight. WinUI/WindowsAppSDK files
# must never come back either.
FORBIDDEN = ['libgcc_s_seh-1.dll', 'libwinpthread-1.dll', 'Microsoft.UI.Xaml.dll',
             'Microsoft.WindowsAppRuntime.dll', 'WindowsAppRuntime.dll', 'Microsoft.WinUI.dll']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_tree(source, destination):
    """Copy source over destination with hard conflict detection instead of silent overwrite."""
    duplicates, conflicts = [], []
    for path in sorted(source.rglob('*')):
        if not path.is_file():
            continue
        relative = path.relative_to(source)
        target_path = destination / relative
        if target_path.exists():
            if digest(path) != digest(target_path):
                conflicts.append(str(relative))
                continue
            duplicates.append({'path': relative.as_posix(), 'bytes': path.stat().st_size})
            continue
        target_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target_path)
    if conflicts:
        raise RuntimeError('Conflicting files between sources: ' + ', '.join(conflicts))
    return duplicates


if not app.is_dir():
    sys.exit(f'missing WPF publish output: {app} (run release.ps1)')
if not settings_exe.is_file():
    sys.exit(f'missing GPUI settings binary: {settings_exe} (run release.ps1)')
if not sqlite.is_file():
    sys.exit(f'missing {SQLITE_PACKAGE} {SQLITE_VERSION} win-x64 e_sqlite3.dll: {sqlite}')

if dst.exists():
    shutil.rmtree(dst)
dst.mkdir(parents=True)

duplicates = copy_tree(app, dst)
shutil.copy2(settings_exe, dst / 'LyricIsland.Settings.exe')
shutil.copy2(sqlite, dst / 'e_sqlite3.dll')

problems = []
for name in REQUIRED:
    if not (dst / name).is_file():
        problems.append(f'missing required file: {name}')
for name in FORBIDDEN:
    if (dst / name).exists():
        problems.append(f'forbidden file present: {name}')
for path in dst.rglob('*.pri'):
    problems.append(f'forbidden resource index present: {path.relative_to(dst)}')
font = dst / FONT_RELATIVE
if font.is_file() and digest(font).upper() != FONT_SHA256.upper():
    problems.append('bundled OPPO Sans does not match the pinned digest')
if problems:
    sys.exit('runtime composition failed:\n  ' + '\n  '.join(problems))

files = [p for p in dst.rglob('*') if p.is_file()]
result = {
    'files': len(files),
    'bytes': sum(p.stat().st_size for p in files),
    'deduplicated_bytes': sum(d['bytes'] for d in duplicates),
    'settings_exe_bytes': (dst / 'LyricIsland.Settings.exe').stat().st_size,
    'e_sqlite3_source': f'{SQLITE_PACKAGE} {SQLITE_VERSION} win-x64',
    'e_sqlite3_sha256': digest(dst / 'e_sqlite3.dll'),
    'lyric_ffi_sha256': digest(dst / 'lyric_ffi.dll'),
    'duplicates': duplicates,
}
(root / 'publish' / 'composition.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps({k: v for k, v in result.items() if k != 'duplicates'}))
