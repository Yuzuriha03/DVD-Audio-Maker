param(
    [ValidateSet('Debug','Release')][string]$Configuration='Debug',
    [switch]$CheckOnly
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$target='aarch64-pc-windows-msvc'
$msvcJson=& python -X utf8 (Join-Path $PSScriptRoot 'msvc_env.py') --json --arch arm64
if($LASTEXITCODE -ne 0){throw 'Cannot initialize the Visual Studio ARM64 environment'}
$msvc=$msvcJson | ConvertFrom-Json
foreach($property in $msvc.PSObject.Properties){[Environment]::SetEnvironmentVariable($property.Name,[string]$property.Value,'Process')}
$env:Path=(Join-Path $env:USERPROFILE '.cargo/bin')+';'+$env:Path
$oldLocation=Get-Location
try {
    Set-Location (Join-Path $root 'rust')
    $installed = @(& rustup target list --installed)
    if($LASTEXITCODE -ne 0){throw 'rustup target query failed'}
    if($installed -notcontains $target){
        & rustup target add $target
        if($LASTEXITCODE -ne 0){throw "Cannot install Rust target $target"}
    }
    $core=@('-p','dvda-mlp','-p','dvda-author','-p','dvda-menu','-p','dvda-core','-p','dvda-native')
    $profile=@()
    if($Configuration -eq 'Release'){$profile+='--release'}
    if($CheckOnly){
        & cargo check --offline --target $target @profile @core
        if($LASTEXITCODE -ne 0){throw 'ARM64 core check failed'}
    } else {
        & cargo test --no-run --offline --target $target @profile @core
        if($LASTEXITCODE -ne 0){throw 'ARM64 core test link failed'}
    }
    # The media/image bridges need ARM64 FFmpeg/ImageMagick builds; compile the
    # CLI and desktop crates without those features to validate the ARM64 code.
    & cargo check --offline --target $target @profile --no-default-features -p dvda-cli -p dvda-desktop
    if($LASTEXITCODE -ne 0){throw 'ARM64 CLI/desktop check failed'}
    if(-not $CheckOnly){
        & cargo build --offline --target $target @profile --no-default-features -p dvda-cli -p dvda-desktop
        if($LASTEXITCODE -ne 0){throw 'ARM64 CLI/desktop build failed'}
        Write-Host "ARM64 binaries: $root\rust\target\$target\$($Configuration.ToLower())"
    }
    Write-Host 'ARM64 cross-build passed (media/image bridges remain Windows x64-only until ARM64 native libraries exist).'
} finally {Set-Location $oldLocation}
