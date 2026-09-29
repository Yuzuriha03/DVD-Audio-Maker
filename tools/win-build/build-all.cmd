@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0..\..\"
set "PROJECT=%ROOT%src\DvdaMaker.Toolchain\DvdaMaker.Toolchain.csproj"
if not exist "%PROJECT%" (
  echo [ERROR] Project not found: %PROJECT%
  exit /b 2
)
dotnet run --project "%PROJECT%" -- package %*
exit /b %ERRORLEVEL%
