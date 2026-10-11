@echo off
rem Called inside each launcher's setlocal; explicit component overrides win.
if /i "%VSCMD_ARG_TGT_ARCH%"=="x64" (
  where cl.exe >nul 2>nul
  if not errorlevel 1 goto check_tools
)
set "_DVDA_VSDEV_CMD=%DVDA_VSDEVCMD%"
if defined _DVDA_VSDEV_CMD goto initialize
set "_DVDA_VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%_DVDA_VSWHERE%" (
  for /f "delims=" %%I in ('where vswhere.exe 2^>nul') do set "_DVDA_VSWHERE=%%I"
)
if exist "%_DVDA_VSWHERE%" (
  for /f "usebackq delims=" %%I in (`"%_DVDA_VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "_DVDA_VSDEV_CMD=%%I\Common7\Tools\VsDevCmd.bat"
)
if not defined _DVDA_VSDEV_CMD if defined VSINSTALLDIR set "_DVDA_VSDEV_CMD=%VSINSTALLDIR%\Common7\Tools\VsDevCmd.bat"
if not defined _DVDA_VSDEV_CMD goto missing_tools

:initialize
if not exist "%_DVDA_VSDEV_CMD%" goto missing_tools
call "%_DVDA_VSDEV_CMD%" -no_logo -arch=x64 -host_arch=x64 >nul 2>&1
if errorlevel 1 (
  >&2 echo [ERROR] Visual Studio x64 environment initialization failed.
  exit /b 1
)

:check_tools
for %%I in (cl.exe lib.exe link.exe dumpbin.exe rc.exe) do (
  where %%I >nul 2>nul
  if errorlevel 1 (
    >&2 echo [ERROR] MSVC x64 build tools or Windows SDK are missing %%I.
    exit /b 1
  )
)
set "CC=cl.exe"
set "DVDA_LIB_TOOL=lib.exe"
set "_DVDA_VSDEV_CMD="
set "_DVDA_VSWHERE="
if not defined DVDA_FFMPEG_PREFIX set "DVDA_FFMPEG_PREFIX=%~dp0..\..\build\msvc-native\ffmpeg\install"
if not defined DVDA_MAGICK_WORK set "DVDA_MAGICK_WORK=%~dp0..\..\build\msvc-native\image"
if not defined DVDA_IMAGE_DEPS_DIR set "DVDA_IMAGE_DEPS_DIR=%DVDA_MAGICK_WORK%\lib"
if not defined DVDA_MENU_NATIVE_DIR set "DVDA_MENU_NATIVE_DIR=%~dp0..\..\build\menu-msvc-vendor"
if not defined DVDA_MEDIA_NATIVE_DIR set "DVDA_MEDIA_NATIVE_DIR=%~dp0..\..\build\msvc-native\media-runtime"
if not defined DVDA_IMAGE_NATIVE_DIR set "DVDA_IMAGE_NATIVE_DIR=%~dp0..\..\build\msvc-native\image-runtime"
if not defined DVDA_ENCODER_LIBRARY set "DVDA_ENCODER_LIBRARY=%~dp0..\..\build\mlp-encoder\mlp_encoder.dll"
exit /b 0

:missing_tools
>&2 echo [ERROR] Install Visual Studio C++ x64 tools and Windows SDK, or set DVDA_VSDEVCMD to VsDevCmd.bat.
exit /b 1
