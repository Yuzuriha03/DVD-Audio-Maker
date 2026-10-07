@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0..\.."
set "MANIFEST=%ROOT%\rust\Cargo.toml"
if not exist "%MANIFEST%" (
  echo [ERROR] Rust workspace not found: %MANIFEST%
  exit /b 2
)
cargo run --target x86_64-pc-windows-gnu --manifest-path "%MANIFEST%" --release --offline -p dvda-toolchain -- package --repo "%ROOT%" %*
exit /b %ERRORLEVEL%
