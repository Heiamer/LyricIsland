# Lyric Island 1.0 release chain: Rust backend + GPUI settings + WPF publish + package.
#
# This replaces the old dual-.NET/WinUI chain. One command produces the EXE, the portable
# ZIP, the source ZIP and SHA256SUMS into ..\..\outputs (override with -Output).
#
# Reproducibility notes:
#  * Rust must use the GNU toolchain with LLVM's dlltool and rust-lld. The GNU ld shipped
#    with the toolchain crashes in build scripts, and gnu-lld needs unstable flags.
#  * Two import libraries are missing from the GNU sysroot and are linked from the tree:
#    build\shlwapi\libshlwapi.a (webbrowser crate, AssocQueryStringW) and
#    build\ktmw32\libktmw32.a (gpui's transitive dependencies).
#  * The settings frontend is GPUI, so this chain also needs the Windows SDK's fxc.exe
#    (gpui compiles its HLSL shaders in release builds) and a C compiler for transitive
#    dependencies. Pass -FxcPath and -ZigCc; see build\native.ps1 for the same inputs.
#  * llvm-dlltool.exe itself is a ~67 MB llvm-tools-preview binary and is expected from the
#    toolchain or a scratch directory, so it is taken as a parameter rather than shipped.
param(
    [string]$Output = '',
    [string]$DllTool = '',
    [string]$FxcPath = '',
    [string]$ZigCc = '',
    [string]$TargetDirectory = '',
    [switch]$SkipRust
)
$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot
if (-not $Output) { $Output = Join-Path (Split-Path (Split-Path $Root)) 'outputs' }

$Toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-gnu'
if (-not (Test-Path $Toolchain)) { throw "missing Rust toolchain: $Toolchain" }

# Prefer the harness python, then PATH.
$Python = Join-Path $env:USERPROFILE '.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
if (-not (Test-Path $Python)) { $Python = (Get-Command python -ErrorAction SilentlyContinue).Source }
if (-not $Python) { throw 'python not found' }

$env:PATH = "$Toolchain\bin;$Toolchain\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;$env:PATH"
if (-not $TargetDirectory) { $TargetDirectory = Join-Path (Split-Path $Root) 'rust-target-lld-08' }
$env:CARGO_TARGET_DIR = $TargetDirectory

if (-not $DllTool) { $DllTool = (Get-Command llvm-dlltool.exe -ErrorAction SilentlyContinue).Source }
if (-not $DllTool -or -not (Test-Path -LiteralPath $DllTool)) {
    throw 'Supply -DllTool with the absolute path to llvm-dlltool.exe (LLVM tools).'
}
# gpui compiles HLSL with the Windows SDK's fxc.exe, which is not installed system-wide here.
if (-not $FxcPath) { $FxcPath = $env:GPUI_FXC_PATH }
if (-not $FxcPath -or -not (Test-Path -LiteralPath $FxcPath)) {
    throw 'Supply -FxcPath with the absolute path to fxc.exe (Windows SDK bin/<ver>/x64/fxc.exe).'
}
# Some transitive dependencies build C code and there is no MSVC here, so zig cc stands in.
if (-not $ZigCc) { $ZigCc = $env:LYRICISLAND_ZIG_CC }
if (-not $ZigCc -or -not (Test-Path -LiteralPath $ZigCc)) {
    throw 'Supply -ZigCc with a C compiler driver (zig cc wrapper) for dependencies that build C.'
}
$env:GPUI_FXC_PATH = $FxcPath
$env:CC = $ZigCc
$env:AR = Join-Path $Toolchain 'lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-ar.exe'

$ShlwapiLib = Join-Path $Root 'build\shlwapi'
if (-not (Test-Path (Join-Path $ShlwapiLib 'libshlwapi.a'))) { throw "missing $ShlwapiLib\libshlwapi.a" }
$Ktmw32Lib = Join-Path $Root 'build\ktmw32'
if (-not (Test-Path (Join-Path $Ktmw32Lib 'libktmw32.a'))) { throw "missing $Ktmw32Lib\libktmw32.a" }

# The import libraries go into RUSTFLAGS rather than a per-crate `-- -L native` argument: any link
# step that pulls in the webbrowser crate or gpui needs them, including `cargo test --workspace`,
# which has no place to pass extra rustc flags. `+crt-static` is repeated here because setting
# RUSTFLAGS makes cargo ignore the project's .cargo/config.toml.
$env:RUSTFLAGS = "-C target-feature=+crt-static -C dlltool=$DllTool -C linker=$Toolchain\lib\rustlib\x86_64-pc-windows-gnu\bin\rust-lld.exe -C linker-flavor=ld.lld -L native=$ShlwapiLib -L native=$Ktmw32Lib"

function Invoke-Checked([string]$Label, [scriptblock]$Step) {
    & $Step
    if ($LASTEXITCODE -ne 0) { throw "$Label failed (exit $LASTEXITCODE)" }
    Write-Host "[ok] $Label"
}

# A GUI-subsystem exe is not awaited by the call operator; go through .NET explicitly.
# PowerShell 5.1 runs on .NET Framework, where ProcessStartInfo has no ArgumentList: build the
# command line as one string and quote the arguments that need it.
function Invoke-GuiChecked([string]$Label, [string]$Exe, [string[]]$Arguments) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.UseShellExecute = $false
    $psi.Arguments = (@($Arguments | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' ')
    $process = [System.Diagnostics.Process]::Start($psi)
    if (-not $process.WaitForExit(300000)) { $process.Kill(); throw "$Label timed out" }
    if ($process.ExitCode -ne 0) { throw "$Label failed (exit $($process.ExitCode))" }
    Write-Host "[ok] $Label"
}

Set-Location $Root

if (-not $SkipRust) {
    Push-Location (Join-Path $Root 'Rust')
    try {
        Invoke-Checked 'cargo build -p lyric-ffi' { cargo build --offline --release -p lyric-ffi }
        Invoke-Checked 'cargo build -p lyric-settings' {
            cargo build --offline --release -p lyric-settings
        }
    } finally { Pop-Location }

    $ffi = Join-Path $env:CARGO_TARGET_DIR 'release\lyric_ffi.dll'
    if (-not (Test-Path $ffi)) { throw "missing built backend: $ffi" }
    # A fresh checkout has no Rust\artifacts directory, and Copy-Item will not create one.
    $artifacts = Join-Path $Root 'Rust\artifacts'
    if (-not (Test-Path $artifacts)) { New-Item -ItemType Directory -Force -Path $artifacts | Out-Null }
    Copy-Item $ffi (Join-Path $artifacts 'lyric_ffi.dll') -Force
    Write-Host "[ok] staged lyric_ffi.dll ($((Get-Item $ffi).Length) bytes)"
}

# LyricIsland.Tests builds this same project with UseRustBackend=false into the same bin/obj.
# Drop the previous app output and state the property explicitly so a shipped publish can
# never mix the legacy and production assembly variants.
$AppBin = Join-Path $Root 'LyricIsland.App\bin\Release'
if (Test-Path $AppBin) { Remove-Item $AppBin -Recurse -Force }
Invoke-Checked 'dotnet publish LyricIsland.App' {
    dotnet publish LyricIsland.App -c Release -r win-x64 --self-contained true `
        -p:UseRustBackend=true -p:PublishSingleFile=false -p:DebugType=None -p:DebugSymbols=false -o publish\app-10
}
Invoke-Checked 'merge_runtime.py' { & $Python -X utf8 (Join-Path $Root 'build\merge_runtime.py') }
Invoke-Checked 'package_release.py' { & $Python -X utf8 (Join-Path $PSScriptRoot 'package_release.py') --output $Output }

# The shipped launcher must really unpack the payload it carries before we hand it over.
$verify = Join-Path $Root 'publish\verify-package'
if (Test-Path $verify) { Remove-Item $verify -Recurse -Force }
Invoke-GuiChecked 'verify-package' (Join-Path $Output 'LyricIsland-1.0.exe') @('--verify-package', $verify)
$report = Join-Path $verify 'verification.json'
if (-not (Test-Path $report)) { throw "verify-package did not write $report" }
Write-Host "[ok] verification.json: $(Get-Content $report -Raw)"
Write-Host "release written to $Output"
