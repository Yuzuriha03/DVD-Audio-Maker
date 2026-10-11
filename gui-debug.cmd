@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
pushd "%ROOT%" || exit /b 1
call "%ROOT%tools\win-build\rust-dev-env.cmd"
if errorlevel 1 (
  popd
  exit /b 1
)
set "PROFILE_ARG="
if "%~1"=="" if exist "%ROOT%settings.local.json" set "PROFILE_ARG=--profile "%ROOT%settings.local.json""
cargo run --target x86_64-pc-windows-msvc --manifest-path "%ROOT%rust\Cargo.toml" --offline -p dvda-desktop -- %PROFILE_ARG% %*
set "GUI_EXIT=%ERRORLEVEL%"
popd
exit /b %GUI_EXIT%
