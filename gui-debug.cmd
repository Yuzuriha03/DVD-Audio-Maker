@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
pushd "%ROOT%" || exit /b 1
dotnet run --project "%ROOT%src\DvdaMaker.Desktop\DvdaMaker.Desktop.csproj" --configuration Debug --runtime win-x64 --self-contained false --no-launch-profile -- %*
set "GUI_EXIT=%ERRORLEVEL%"
popd
exit /b %GUI_EXIT%
