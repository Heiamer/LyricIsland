<#
.SYNOPSIS
编译当前歌词岛主程序与设置窗口，并执行工作区测试。
.DESCRIPTION
本目录还提供发行打包、应用图标嵌入及可选通知身份包的生成工具。
#>
param(
    [string]$DllTool = '',
    [string]$TargetDirectory = '',
    [string]$FxcPath = '',
    [string]$ZigCc = '',
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path $PSScriptRoot
$ToolchainRoot = Join-Path $env:USERPROFILE '.rustup/toolchains/stable-x86_64-pc-windows-gnu'
if (-not $DllTool) {
    $DllTool = (Get-Command llvm-dlltool.exe -ErrorAction SilentlyContinue).Source
}
if (-not $DllTool -or -not (Test-Path -LiteralPath $DllTool)) {
    throw 'Supply -DllTool with the absolute path to llvm-dlltool.exe (LLVM tools).'
}
# gpui compiles its HLSL shaders with the Windows SDK's fxc.exe in release builds. It is not
# installed system-wide on this machine, so an extracted SDK is pointed at explicitly.
if (-not $FxcPath) { $FxcPath = $env:GPUI_FXC_PATH }
if (-not $FxcPath -or -not (Test-Path -LiteralPath $FxcPath)) {
    throw 'Supply -FxcPath with the absolute path to fxc.exe (Windows SDK bin/<ver>/x64/fxc.exe).'
}
# Some transitive dependencies build C code; there is no MSVC here, so zig cc stands in.
if (-not $ZigCc) { $ZigCc = $env:LYRICISLAND_ZIG_CC }
if (-not $ZigCc -or -not (Test-Path -LiteralPath $ZigCc)) {
    throw 'Supply -ZigCc with a C compiler driver (zig cc wrapper) for dependencies that build C.'
}
if (-not $TargetDirectory) { $TargetDirectory = Join-Path $ProjectRoot 'Rust/target-native' }

$env:PATH = (Join-Path $ToolchainRoot 'bin') + ';' + (Join-Path $ToolchainRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained') + ';' + $env:PATH
$env:CARGO_TARGET_DIR = $TargetDirectory
$env:GPUI_FXC_PATH = $FxcPath
$env:CC = $ZigCc
$env:AR = Join-Path $ToolchainRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin/llvm-ar.exe'

$LinkerPath = Join-Path $ToolchainRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin/rust-lld.exe'
# Import libraries the GNU sysroot lacks: shlwapi (webbrowser) and ktmw32 (gpui's transitive deps).
$NativeLibraries = @((Join-Path $ProjectRoot 'build/shlwapi'), (Join-Path $ProjectRoot 'build/ktmw32'))
$Encoded = @('-C', 'target-feature=+crt-static', '-C', "dlltool=$DllTool", '-C', "linker=$LinkerPath", '-C', 'linker-flavor=ld.lld')
foreach ($library in $NativeLibraries) { $Encoded += @('-L', "native=$library") }
$env:CARGO_ENCODED_RUSTFLAGS = $Encoded -join [char]31

$ExtraCargo = @()
if ($Offline) { $ExtraCargo += '--offline' }
Push-Location (Join-Path $ProjectRoot 'Rust')
try {
    & cargo test @ExtraCargo --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Native checks failed.' }
    & cargo build @ExtraCargo --release -p lyric-native -p lyric-settings
    if ($LASTEXITCODE -ne 0) { throw 'Native build failed.' }
} finally { Pop-Location }
Write-Output "Built executables in $TargetDirectory/release. Keep the complete Assets font directory and the lyric-island-settings (GPUI) executable with the application."
