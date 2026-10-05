@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
call "%ROOT%tools\win-build\rust-dev-env.cmd"
set "MANIFEST=%ROOT%rust\Cargo.toml"
set "PROFILE="
set "DRY_RUN="

:parse
if "%~1"=="" goto run
if /i "%~1"=="--dry-run" (
  set "DRY_RUN=1"
  shift
  goto parse
)
if /i "%~1"=="--profile" (
  if "%~2"=="" (
    echo [ERROR] --profile requires a JSON profile path.
    exit /b 2
  )
  set "PROFILE=%~2"
  shift
  shift
  goto parse
)
echo [ERROR] Unknown argument: %~1
exit /b 2

:run
if not defined PROFILE if exist "%ROOT%settings.local.json" set "PROFILE=%ROOT%settings.local.json"
set "PROFILE_ARG="
if defined PROFILE set "PROFILE_ARG=--profile "%PROFILE%""
echo [INFO] Preparing source audio...
cargo run --target x86_64-pc-windows-gnu --manifest-path "%MANIFEST%" --release --offline -p dvda-cli -- prepare %PROFILE_ARG%
if errorlevel 1 exit /b 1
if defined DRY_RUN (
  echo [INFO] Writing a dry-run plan...
  cargo run --target x86_64-pc-windows-gnu --manifest-path "%MANIFEST%" --release --offline -p dvda-cli -- build --dry-run %PROFILE_ARG%
) else (
  echo [INFO] Building DVD-Audio output...
  cargo run --target x86_64-pc-windows-gnu --manifest-path "%MANIFEST%" --release --offline -p dvda-cli -- build %PROFILE_ARG%
)
exit /b %ERRORLEVEL%
