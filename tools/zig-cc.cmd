@echo off
if defined DOTISLE_ZIG (
  "%DOTISLE_ZIG%" cc %*
) else (
  zig cc %*
)
