@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0..\.."
call "%~dp0rust-dev-env.cmd"
if errorlevel 1 exit /b 1
set "MANIFEST=%ROOT%\rust\Cargo.toml"
set "TARGET=x86_64-pc-windows-msvc"
if not exist "%MANIFEST%" (
  echo [ERROR] Rust workspace not found: %MANIFEST%
  exit /b 2
)
cargo run --target %TARGET% --manifest-path "%MANIFEST%" --release --offline -p dvda-toolchain -- package --repo "%ROOT%" %*
exit /b %ERRORLEVEL%
