param([Parameter(Mandatory=$true,ValueFromRemainingArguments=$true)][string[]]$CargoArgs)
$ErrorActionPreference='Stop'
$ProjectRoot = Split-Path $PSScriptRoot -Parent
$ToolchainRoot = Join-Path $env:USERPROFILE '.rustup/toolchains/stable-x86_64-pc-windows-gnu'
$env:PATH = (Join-Path $ToolchainRoot 'bin') + ';' + (Join-Path $ToolchainRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained') + ';' + $env:PATH
if ($env:DOTISLE_TARGET_DIR) { $env:CARGO_TARGET_DIR = $env:DOTISLE_TARGET_DIR } else { $env:CARGO_TARGET_DIR = Join-Path $ProjectRoot 'target' }
$env:CC = Join-Path $ProjectRoot 'tools/zig-cc.cmd'
$env:AR = Join-Path $ToolchainRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin/llvm-ar.exe'
$DllTool=$env:DOTISLE_DLLTOOL
if(-not $DllTool){$command=Get-Command llvm-dlltool.exe -ErrorAction SilentlyContinue;if($command){$DllTool=$command.Source}}
if(-not $DllTool -or -not (Test-Path -LiteralPath $DllTool)){throw 'Install LLVM llvm-dlltool.exe on PATH, or set DOTISLE_DLLTOOL to its absolute path.'}
$LinkerPath=Join-Path $ToolchainRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin/rust-lld.exe'
$BuildSupport=$env:DOTISLE_BUILD_SUPPORT
if(-not $BuildSupport){$BuildSupport=$ProjectRoot}
$Encoded=@('-C','target-feature=+crt-static','-C',"dlltool=$DllTool",'-C',"linker=$LinkerPath",'-C','linker-flavor=ld.lld','-L',"native=$BuildSupport/build/shlwapi",'-L',"native=$BuildSupport/build/ktmw32")
$env:CARGO_ENCODED_RUSTFLAGS=$Encoded -join [char]31
Push-Location (Join-Path $ProjectRoot 'Rust')
try { & (Join-Path $ToolchainRoot 'bin/cargo.exe') @CargoArgs; exit $LASTEXITCODE } finally { Pop-Location }
