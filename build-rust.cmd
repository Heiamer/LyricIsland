@echo off
rem ---------------------------------------------------------------------------
rem  LyricIsland 1.0 - Rust build helper (backend DLL + GPUI settings EXE).
rem
rem  Batch instead of PowerShell on purpose: the default Windows execution policy
rem  is "Restricted", which refuses to run .ps1 files without a policy change.
rem  ASCII-only output for the same reason (console code page).
rem
rem  This is the Rust half of build\release.ps1, for iterating without the whole
rem  packaging chain. release.ps1 remains the single entry point for a release.
rem  The inputs mirror build\native.ps1: --dlltool (-DllTool), --fxc-path (-FxcPath),
rem  --zig-cc (-ZigCc), --target-dir (-TargetDirectory) and --offline (-Offline).
rem
rem  Toolchain requirements are not optional on this machine:
rem    * the GNU toolchain's ld fails in the GPUI build script (0xC0000005), so
rem      rust-lld with linker-flavor=ld.lld is used instead;
rem    * gnu-lld needs unstable options and does not work;
rem    * the settings frontend is GPUI, so the release build also needs the Windows
rem      SDK's fxc.exe (gpui compiles its HLSL shaders with it) and a C compiler
rem      for transitive dependencies that build C (zig cc stands in for MSVC);
rem    * the GNU sysroot has neither libshlwapi.a (the webbrowser crate wants
rem      AssocQueryStringW) nor libktmw32.a (gpui's transitive dependencies), so
rem      build\shlwapi and build\ktmw32 are linked from the tree;
rem    * setting RUSTFLAGS makes cargo ignore the project's .cargo/config.toml, so
rem      -C target-feature=+crt-static has to be repeated below. Without it the
rem      binaries import libgcc_s_seh-1.dll / libwinpthread-1.dll at runtime.
rem
rem  Usage:
rem    build-rust.cmd                    build backend DLL, then the GPUI settings EXE
rem    build-rust.cmd --skip-settings    build only the backend DLL
rem    build-rust.cmd [--dlltool <path>] [--fxc-path <path>] [--zig-cc <path>]
rem                   [--target-dir <path>] [--online]
rem
rem  build\native.ps1's -DllTool -FxcPath -ZigCc -TargetDirectory -Offline spellings are
rem  accepted as well.
rem  --dlltool falls back to native-tools\llvm-dlltool.exe next to the checkout and
rem  then to PATH; --fxc-path and --zig-cc fall back to %GPUI_FXC_PATH% and
rem  %LYRICISLAND_ZIG_CC%. A missing input is reported before cargo runs.
rem  Builds are --offline by default (like build\release.ps1); --online drops it.
rem ---------------------------------------------------------------------------
setlocal enabledelayedexpansion
set "ROOT=%~dp0"
set "WORK=%~dp0.."
set "RUSTDIR=%ROOT%Rust"
set "SKIPSETTINGS=0"
set "OFFLINE=--offline"
set "DLLTOOL="
set "FXC="
set "ZIGCC="
set "TARGETDIR="

:parseargs
if "%~1"=="" goto argsdone
rem Accept the batch-style --options and build\native.ps1's -DllTool/-FxcPath/-ZigCc/
rem -TargetDirectory/-Offline spellings, so one command line works for either helper.
set "OPT=%~1"
if /i "!OPT!"=="-SkipSettings" set "OPT=--skip-settings"
if /i "!OPT!"=="-Offline" set "OPT=--offline"
if /i "!OPT!"=="-DllTool" set "OPT=--dlltool"
if /i "!OPT!"=="-FxcPath" set "OPT=--fxc-path"
if /i "!OPT!"=="-ZigCc" set "OPT=--zig-cc"
if /i "!OPT!"=="-TargetDirectory" set "OPT=--target-dir"
if /i "!OPT!"=="--skip-settings" ( set "SKIPSETTINGS=1" & shift & goto parseargs )
if /i "!OPT!"=="--offline" ( set "OFFLINE=--offline" & shift & goto parseargs )
if /i "!OPT!"=="--online" ( set "OFFLINE=" & shift & goto parseargs )
if /i "!OPT!"=="--dlltool" (
    if "%~2"=="" goto missingvalue
    set "DLLTOOL=%~2"
    shift
    shift
    goto parseargs
)
if /i "!OPT!"=="--fxc-path" (
    if "%~2"=="" goto missingvalue
    set "FXC=%~2"
    shift
    shift
    goto parseargs
)
if /i "!OPT!"=="--zig-cc" (
    if "%~2"=="" goto missingvalue
    set "ZIGCC=%~2"
    shift
    shift
    goto parseargs
)
if /i "!OPT!"=="--target-dir" (
    if "%~2"=="" goto missingvalue
    set "TARGETDIR=%~2"
    shift
    shift
    goto parseargs
)
echo [error] unknown option: %~1
goto usage

:missingvalue
echo [error] %~1 needs a value
:usage
echo usage: build-rust.cmd [--skip-settings] [--dlltool ^<path^>] [--fxc-path ^<path^>]
echo                        [--zig-cc ^<path^>] [--target-dir ^<path^>] [--online]
echo        build\native.ps1 spellings are accepted as well: -DllTool -FxcPath -ZigCc
echo        -TargetDirectory -Offline
exit /b 2

:argsdone
set "TOOLCHAIN=%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu"
if not exist "%TOOLCHAIN%\bin\cargo.exe" (
    echo [error] Rust GNU toolchain not found: %TOOLCHAIN%
    echo         Install it with: rustup toolchain install stable-x86_64-pc-windows-gnu
    exit /b 1
)
set "LLVMAR=%TOOLCHAIN%\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-ar.exe"
set "RUSTLLD=%TOOLCHAIN%\lib\rustlib\x86_64-pc-windows-gnu\bin\rust-lld.exe"
set "LLVMOBJDUMP=%TOOLCHAIN%\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-objdump.exe"
if not exist "%LLVMAR%" (
    echo [error] missing %LLVMAR%
    echo         The GNU toolchain looks incomplete; reinstall it with rustup.
    exit /b 1
)
if not exist "%RUSTLLD%" (
    echo [error] missing %RUSTLLD%
    echo         The GNU toolchain looks incomplete; reinstall it with rustup.
    exit /b 1
)

rem llvm-dlltool.exe is a ~67 MB llvm-tools-preview binary that is deliberately not shipped
rem in the source tree, so take --dlltool, then ..\native-tools, then PATH.
if not defined DLLTOOL set "DLLTOOL=%WORK%\native-tools\llvm-dlltool.exe"
if not exist "%DLLTOOL%" (
    set "ONPATH="
    for %%I in (llvm-dlltool.exe) do set "ONPATH=%%~$PATH:I"
    if defined ONPATH set "DLLTOOL=!ONPATH!"
)
if not exist "%DLLTOOL%" (
    echo [error] llvm-dlltool.exe not found.
    echo         Pass --dlltool ^<path^> with the absolute path to llvm-dlltool.exe, or
    echo         put a copy at %WORK%\native-tools\llvm-dlltool.exe.
    exit /b 1
)

set "SHLWAPI=%ROOT%build\shlwapi"
if not exist "%SHLWAPI%\libshlwapi.a" (
    echo [error] missing %SHLWAPI%\libshlwapi.a
    echo         The webbrowser crate needs it for AssocQueryStringW; keep it in the tree.
    exit /b 1
)
set "KTMW32=%ROOT%build\ktmw32"
if not exist "%KTMW32%\libktmw32.a" (
    echo [error] missing %KTMW32%\libktmw32.a
    echo         gpui's transitive dependencies need it; keep it in the tree.
    exit /b 1
)

rem fxc.exe and the C compiler only matter for the GPUI settings build, so resolve them here
rem and validate them below, after the --skip-settings shortcut.
if not defined FXC set "FXC=%GPUI_FXC_PATH%"
if not defined ZIGCC set "ZIGCC=%LYRICISLAND_ZIG_CC%"

rem RUSTFLAGS is one space-separated string, so a space inside any of these paths would be
rem split into extra bogus arguments. Refuse to build instead of linking something wrong.
set "SPACED="
if not "%SHLWAPI%"=="%SHLWAPI: =%" set "SPACED=%SHLWAPI%"
if not "%KTMW32%"=="%KTMW32: =%" set "SPACED=%KTMW32%"
if not "%DLLTOOL%"=="%DLLTOOL: =%" set "SPACED=%DLLTOOL%"
if not "%TOOLCHAIN%"=="%TOOLCHAIN: =%" set "SPACED=%TOOLCHAIN%"
if defined SPACED (
    echo [error] this path contains a space: %SPACED%
    echo         RUSTFLAGS is passed as one space-separated string, so the space would be
    echo         split into extra arguments. Move the checkout or the toolchain to a
    echo         space-free path before building.
    exit /b 1
)

set "PATH=%TOOLCHAIN%\bin;%TOOLCHAIN%\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;%PATH%"
if defined TARGETDIR (
    set "CARGO_TARGET_DIR=%TARGETDIR%"
) else (
    set "CARGO_TARGET_DIR=%WORK%\rust-target-lld-08"
)

rem gpui's build script compiles HLSL with the Windows SDK's fxc.exe and some transitive
rem dependencies compile C code, so both tools have to reach the build scripts via the
rem environment. The archiver comes from the toolchain itself.
set "GPUI_FXC_PATH=%FXC%"
set "CC=%ZIGCC%"
set "AR=%LLVMAR%"

rem The import libraries go into RUSTFLAGS rather than a per-crate "-L native" argument: every
rem link step that pulls in the webbrowser crate or gpui needs them, and "cargo test --workspace"
rem has no place to pass extra rustc flags. +crt-static is repeated here because setting
rem RUSTFLAGS makes cargo ignore the project's .cargo/config.toml.
set "RUSTFLAGS=-C target-feature=+crt-static -C dlltool=%DLLTOOL% -C linker=%RUSTLLD% -C linker-flavor=ld.lld -L native=%SHLWAPI% -L native=%KTMW32%"

echo cargo      : %TOOLCHAIN%\bin\cargo.exe
echo workspace  : %RUSTDIR%
echo target dir : %CARGO_TARGET_DIR%
echo dlltool    : %DLLTOOL%
echo native libs: %SHLWAPI%; %KTMW32%
echo fxc        : %FXC%
echo C compiler : %ZIGCC%
echo archiver   : %LLVMAR%
echo.

pushd "%RUSTDIR%"

echo ^> cargo build %OFFLINE% --release -p lyric-ffi
call "%TOOLCHAIN%\bin\cargo.exe" build %OFFLINE% --release -p lyric-ffi
if errorlevel 1 (
    popd
    echo.
    echo [error] building lyric_ffi.dll failed.
    exit /b 1
)

popd

rem A GUI-subsystem-aware sanity check before the DLL is staged: +crt-static must have taken
rem effect, otherwise the payload would silently depend on the GNU runtime.
call :forbid_gnu_runtime "%CARGO_TARGET_DIR%\release\lyric_ffi.dll"
if errorlevel 1 exit /b 1

rem The WPF project picks the backend up from Rust\artifacts\lyric_ffi.dll.
if not exist "%RUSTDIR%\artifacts" mkdir "%RUSTDIR%\artifacts"
copy /y "%CARGO_TARGET_DIR%\release\lyric_ffi.dll" "%RUSTDIR%\artifacts\lyric_ffi.dll" >nul
if errorlevel 1 (
    echo [error] could not copy lyric_ffi.dll to %RUSTDIR%\artifacts
    exit /b 1
)
echo    backend dll: %RUSTDIR%\artifacts\lyric_ffi.dll

if "%SKIPSETTINGS%"=="1" goto done

rem gpui compiles its HLSL shaders with the Windows SDK's fxc.exe, which is not installed
rem system-wide here, and some transitive dependencies compile C but there is no MSVC.
if not exist "%FXC%" (
    echo [error] fxc.exe not found: %FXC%
    echo         gpui needs it in release builds. Pass --fxc-path ^<path^>, the same value
    echo         build\native.ps1 takes as -FxcPath, or set GPUI_FXC_PATH. It lives in
    echo         ^<Windows SDK^>\bin\^<version^>\x64\fxc.exe.
    exit /b 1
)
if not exist "%ZIGCC%" (
    echo [error] C compiler not found: %ZIGCC%
    echo         Pass --zig-cc ^<path^>, the same value build\native.ps1 takes as -ZigCc,
    echo         with a C compiler driver such as the zig cc wrapper, or set
    echo         LYRICISLAND_ZIG_CC.
    exit /b 1
)

pushd "%RUSTDIR%"
echo.
echo ^> cargo build %OFFLINE% --release -p lyric-settings
call "%TOOLCHAIN%\bin\cargo.exe" build %OFFLINE% --release -p lyric-settings
if errorlevel 1 (
    popd
    echo.
    echo [error] building the GPUI settings EXE failed.
    exit /b 1
)
popd

set "SETTINGS=%CARGO_TARGET_DIR%\release\lyric-island-settings.exe"
if not exist "%SETTINGS%" (
    echo [error] build output missing: %SETTINGS%
    exit /b 1
)
call :forbid_gnu_runtime "%SETTINGS%"
if errorlevel 1 exit /b 1
echo    settings exe: %SETTINGS%
echo    build\merge_runtime.py ships it as LyricIsland.Settings.exe

:done
echo.
echo Done. Run build\release.ps1 for the full publish and packaging chain.
exit /b 0

rem Verify the "no GNU runtime dependency" constraint directly from the PE import table.
:forbid_gnu_runtime
if not exist "%LLVMOBJDUMP%" (
    echo [warn] llvm-objdump.exe missing; skipped GNU runtime import check for %~nx1
    exit /b 0
)
"%LLVMOBJDUMP%" -p "%~1" | findstr /i /c:"libgcc_s_seh-1.dll" /c:"libwinpthread-1.dll" >nul
if not errorlevel 1 (
    echo.
    echo [error] %~nx1 imports the GNU runtime DLLs, so +crt-static did not take effect.
    echo         Check that RUSTFLAGS still carries -C target-feature=+crt-static.
    exit /b 1
)
exit /b 0
