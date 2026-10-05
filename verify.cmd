@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
call "%ROOT%cli.cmd" verify %*
exit /b %ERRORLEVEL%
