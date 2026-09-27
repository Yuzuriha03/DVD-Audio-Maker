@echo off
REM ===========================================================================
REM  build-all.bat - one-shot Windows-native build.  NO WSL REQUIRED.
REM
REM  Usage:   build-all.bat
REM
REM  Optional environment variables (set before running):
REM      MSYS2_ROOT       path to MSYS2 if not in a default location
REM      DVDA_SRC_TREE    source tree if not in <kit>\src
REM      DVDA_FONT_SRC    path to NotoSansCJK-Regular.ttc
REM      DVDA_SCRIPTS     directory holding 01_prepare.py (default: repo root)
REM      JOBS             parallel make jobs (default: CPU count)
REM
REM  Windows-style values (D:\x\y) are accepted; the bash side converts them.
REM
REM  Steps (all inside MSYS2, which is a NATIVE WINDOWS environment):
REM      1. sanity check the source tree and toolchain
REM      2. build dvda-author-dev.exe (24-bit lossless MLP)
REM      3. build dvdauthor / spumux / spuunmux (AMGM patch)
REM      4. assemble the menu tool dir (exe + DLLs + ImageMagick + fonts)
REM      5. produce the shippable release folder
REM
REM  NOTE: keep this file pure ASCII. cmd.exe reads .bat/.cmd in the OEM code
REM  page, and UTF-8 text (even inside comments) can break parsing.
REM ===========================================================================
setlocal EnableExtensions

REM 8.3 short path: guarantees NO SPACES, which keeps the bash invocation
REM below free of quoting problems. %~sdp0 = short drive+path of this script.
set "KITS=%~sdp0"
if "%KITS:~-1%"=="\" set "KITS=%KITS:~0,-1%"
set "KIT=%~dp0"
if "%KIT:~-1%"=="\" set "KIT=%KIT:~0,-1%"

REM ---- locate MSYS2 (subroutines: safer than nesting for inside if) --------
if not "%MSYS2_ROOT%"=="" goto :have_msys
call :probe "%KIT%\..\msys64"
if not "%MSYS2_ROOT%"=="" goto :have_msys
call :probe "D:\dev\msys64"
if not "%MSYS2_ROOT%"=="" goto :have_msys
call :probe "C:\msys64"
if not "%MSYS2_ROOT%"=="" goto :have_msys
call :probe "D:\msys64"
if not "%MSYS2_ROOT%"=="" goto :have_msys
call :probe "C:\msys32"
if not "%MSYS2_ROOT%"=="" goto :have_msys
call :probe "D:\msys32"
if not "%MSYS2_ROOT%"=="" goto :have_msys

echo [ERROR] MSYS2 not found.
echo         Set MSYS2_ROOT, or install it to C:\msys64 or D:\msys64.
echo         Install steps are in tools\win-build\README.md
exit /b 1

:have_msys
if not exist "%MSYS2_ROOT%\usr\bin\bash.exe" (
  echo [ERROR] bad MSYS2_ROOT: %MSYS2_ROOT%
  exit /b 1
)

echo   kit   : %KIT%
echo   msys2 : %MSYS2_ROOT%
echo   (WSL is not used; everything runs inside MSYS2)
echo.

set "MSYSTEM=MINGW64"
set "CHERE_INVOKING=1"
set "PATH=%MSYS2_ROOT%\mingw64\bin;%MSYS2_ROOT%\usr\bin;%PATH%"

REM DVDA_SRC_TREE / DVDA_FONT_SRC / DVDA_SCRIPTS / MSYS2_ROOT are inherited
REM from the environment automatically, so they are not repeated here.
"%MSYS2_ROOT%\usr\bin\bash.exe" -lc "cd $(cygpath -u '%KITS%') && exec bash ./build-all.sh"

set "RC=%ERRORLEVEL%"
echo.
if "%RC%"=="0" (echo   BUILD OK) else (echo   BUILD FAILED rc=%RC%)
endlocal & exit /b %RC%

REM ---------------------------------------------------------------------------
:probe
if exist "%~1\usr\bin\bash.exe" set "MSYS2_ROOT=%~f1"
goto :eof
