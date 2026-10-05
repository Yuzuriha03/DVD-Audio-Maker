@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
pushd "%ROOT%" || exit /b 1
call "%ROOT%tools\win-build\rust-dev-env.cmd"
set "PROFILE_ARG="
set "HAS_PROFILE="
for %%A in (%*) do if /i "%%~A"=="--profile" set "HAS_PROFILE=1"
for %%A in (%*) do if /i "%%~A"=="--config" set "HAS_PROFILE=1"
if not defined HAS_PROFILE if exist "%ROOT%settings.local.json" set "PROFILE_ARG=--profile "%ROOT%settings.local.json""
if "%~1"=="" (
  cargo run --target x86_64-pc-windows-gnu --manifest-path "%ROOT%rust\Cargo.toml" --offline -p dvda-cli -- config %PROFILE_ARG%
) else (
  cargo run --target x86_64-pc-windows-gnu --manifest-path "%ROOT%rust\Cargo.toml" --offline -p dvda-cli -- %* %PROFILE_ARG%
)
set "CLI_EXIT=%ERRORLEVEL%"
popd
exit /b %CLI_EXIT%
