@echo off
setlocal
rem Build-only dependency: an x64 MinGW-w64 GCC. No MSYS/Bash shell is used.
rem Usage: build-magick-shim.cmd [gcc.exe] [output-directory]
set "CC=%~1"
if not defined CC set "CC=gcc.exe"
for %%C in ("%CC%") do if exist "%%~fC" set "PATH=%%~dpC;%PATH%"
set "OUT=%~2"
if not defined OUT set "OUT=%~dp0..\..\build\magick-shim"
if not exist "%OUT%" mkdir "%OUT%"
if errorlevel 1 exit /b 1
"%CC%" -Os -Wall -Wextra -Werror -s -nostdlib -fno-builtin -fno-stack-protector -fno-ident -Wl,--entry,entry -Wl,--subsystem,console -Wl,--no-insert-timestamp "%~dp0native\magick-shim.c" -lkernel32 -o "%OUT%\magick-shim.exe"
if errorlevel 1 exit /b 1
echo [OK] %OUT%\magick-shim.exe
