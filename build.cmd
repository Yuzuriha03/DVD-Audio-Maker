@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
set "PROJECT=%ROOT%src\DvdaMaker.Cli\DvdaMaker.Cli.csproj"

if not exist "%PROJECT%" (
  echo [ERROR] Project not found: %PROJECT%
  exit /b 2
)

set "CONFIG_PATH="
set "DRY_RUN="

:parse
if "%~1"=="" goto run
if /i "%~1"=="--dry-run" (
  set "DRY_RUN=1"
  shift
  goto parse
)
if /i "%~1"=="--config" (
  if "%~2"=="" (
    echo [ERROR] --config requires a file path.
    exit /b 2
  )
  set "CONFIG_PATH=%~2"
  shift
  shift
  goto parse
)
echo [ERROR] Unknown argument: %~1
exit /b 2

:run
if defined CONFIG_PATH goto run_with_config
goto run_without_config

:run_with_config
call dotnet run --project "%PROJECT%" -- prepare --config "%CONFIG_PATH%"
if errorlevel 1 exit /b 1
if defined DRY_RUN goto dry_with_config
call dotnet run --project "%PROJECT%" -- build --config "%CONFIG_PATH%"
exit /b %ERRORLEVEL%

:dry_with_config
call dotnet run --project "%PROJECT%" -- build --dry-run --config "%CONFIG_PATH%"
exit /b %ERRORLEVEL%

:run_without_config
call dotnet run --project "%PROJECT%" -- prepare
if errorlevel 1 exit /b 1
if defined DRY_RUN goto dry_without_config
call dotnet run --project "%PROJECT%" -- build
exit /b %ERRORLEVEL%

:dry_without_config
call dotnet run --project "%PROJECT%" -- build --dry-run
exit /b %ERRORLEVEL%
