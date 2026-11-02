$CargoArguments = $args
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $root
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $installation) { throw 'Visual Studio C++ x64 build tools are required.' }
$devcmd = Join-Path $installation 'Common7\Tools\VsDevCmd.bat'
$environment = & $env:ComSpec /d /c "call `"$devcmd`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
if ($LASTEXITCODE -ne 0) { throw 'Cannot initialize the MSVC environment.' }
foreach ($line in $environment) {
    if ($line -match '^([^=]+)=(.*)$') {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
    }
}
$defaults = @{
    DVDA_FFMPEG_PREFIX = 'build\msvc-native\ffmpeg-static\install'
}
foreach ($name in $defaults.Keys) {
    if (-not [Environment]::GetEnvironmentVariable($name, 'Process')) {
        [Environment]::SetEnvironmentVariable($name, (Join-Path $root $defaults[$name]), 'Process')
    }
}
& cargo @CargoArguments
exit $LASTEXITCODE
