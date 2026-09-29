@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
set "PROJECT=%ROOT%src\DvdaMaker.Cli\DvdaMaker.Cli.csproj"

if not exist "%PROJECT%" (
  echo [ERROR] Project not found: %PROJECT%
  exit /b 2
)

if "%~1"=="" (
  dotnet run --project "%PROJECT%" -- verify all
) else (
  dotnet run --project "%PROJECT%" -- verify %*
)
exit /b %ERRORLEVEL%
