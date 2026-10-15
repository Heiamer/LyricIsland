"""Package the composed 1.0 runtime into the shipped artefacts.

   LyricIsland-1.0.exe          .NET Framework launcher carrying the payload as a resource
   LyricIsland-1.0-Portable.zip payload folder + docs, ready to unzip and run
   LyricIsland-1.0-Source.zip   full source tree for a reproducible rebuild
   LyricIsland-1.0-SHA256.txt   digests of the three files above

The release directory is deleted before copying so a previous architecture can never leak
into a new build.
"""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import zipfile

parser = argparse.ArgumentParser()
parser.add_argument('--output', default='')
options = parser.parse_args()

root = Path(__file__).resolve().parents[1]
out = Path(options.output) if options.output else root.parent.parent / 'outputs'
out.mkdir(parents=True, exist_ok=True)
name = 'LyricIsland-1.0'
release = out / name

composed = root / 'publish' / name
if not composed.is_dir():
    raise SystemExit(f'missing composed runtime: {composed} (run merge_runtime.py)')

if release.exists():
    shutil.rmtree(release)
shutil.copytree(composed, release, ignore=shutil.ignore_patterns('Logs', '*.log'))

docs = ['LICENSE', 'README.md', 'VALIDATION.md', 'FONT-NOTICE.md', 'NETWORK.md', 'MOTION.md',
        'CHANGELOG.md', 'THIRD-PARTY-NOTICES.md', 'OPTIMIZATION.md', 'RUST-PORT.md']
for doc in docs:
    shutil.copy2(root / doc, release / doc)

# Full texts of the third-party licences referenced by THIRD-PARTY-NOTICES.md.
shutil.copytree(root / 'licenses', release / 'licenses', dirs_exist_ok=True)

payload = out / f'{name}-Portable.zip'
with zipfile.ZipFile(payload, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for path in sorted(release.rglob('*')):
        if path.is_file():
            archive.write(path, path.relative_to(out).as_posix())

payload_hash = hashlib.sha256(payload.read_bytes()).hexdigest()
version_file = root / 'LyricIsland.Bootstrap' / 'BundleVersion.g.cs'
version_file.write_text(
    'internal static class BundleVersion { internal const string Version="1.0"; '
    f'internal const string PayloadHash="{payload_hash}"; }}\n',
    encoding='utf-8')

# GuiContracts and Bootstrap are part of production; the legacy C# projects stay in the
# source zip as reference but do not participate in the shipped backend.
# `target-native` is build\native.ps1's default CARGO_TARGET_DIR (the GPUI frontend lives in
# this same tree); like the other `target` directories it is a multi-GB build cache, never
# source, and must not be packed. build\package_native.py already excludes it.
projects = ['LyricIsland.App', 'TaskbarLyrics.GuiContracts', 'LyricIsland.Bootstrap', 'Rust', 'build',
            'licenses', 'LyricIsland.Settings', 'LyricIsland.Tests', 'TaskbarLyrics.Core',
            'TaskbarLyrics.Core.Tests', 'RustInterop.Smoke']
excluded = {'bin', 'obj', 'target', 'target-native', 'artifacts', 'Logs', 'TestResults', '.git',
            '__pycache__', 'publish'}
source = out / f'{name}-Source.zip'
with zipfile.ZipFile(source, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for folder in projects:
        for path in sorted((root / folder).rglob('*')):
            if not path.is_file():
                continue
            if any(part in excluded for part in path.relative_to(root).parts):
                continue
            archive.write(path, f'{name}-Source/{path.relative_to(root).as_posix()}')
    for doc in docs + ['LyricIsland.sln', 'build-rust.cmd']:
        archive.write(root / doc, f'{name}-Source/{doc}')

framework = Path('C:/Windows/Microsoft.NET/Framework64/v4.0.30319')
executable = out / f'{name}.exe'
subprocess.run([str(framework / 'csc.exe'), '/nologo', '/target:winexe', '/platform:x64', '/optimize+',
                '/out:' + str(executable),
                '/win32icon:' + str(root / 'LyricIsland.App/Assets/LyricIsland-v2.ico'),
                '/resource:' + str(payload) + ',LyricIsland.Payload.zip',
                '/reference:' + str(framework / 'System.IO.Compression.dll'),
                str(root / 'LyricIsland.Bootstrap/Program.cs'), str(version_file)], check=True)

checks = []
for path in [payload, source, executable]:
    checks.append(hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + path.name)
(out / f'{name}-SHA256.txt').write_text('\n'.join(checks) + '\n', encoding='utf-8')

print(json.dumps({'exe_bytes': executable.stat().st_size,
                  'portable_bytes': payload.stat().st_size,
                  'source_bytes': source.stat().st_size}))
