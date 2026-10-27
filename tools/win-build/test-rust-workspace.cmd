@echo off
rem Runs the workspace tests with TEMP/TMP inside build\test-tmp. Some Windows
rem setups deny test processes access to the system temp folder; this folder
rem is created on demand and lives under the ignored build directory.
setlocal
set "ROOT=%~dp0..\.."
call "%~dp0rust-dev-env.cmd"
if errorlevel 1 exit /b 1
if not exist "%ROOT%\build\test-tmp" mkdir "%ROOT%\build\test-tmp"
set "TEMP=%ROOT%\build\test-tmp"
set "TMP=%ROOT%\build\test-tmp"
cd /d "%ROOT%\rust"
cargo test --workspace --no-default-features --features rust-mlp %*
exit /b %ERRORLEVEL%
