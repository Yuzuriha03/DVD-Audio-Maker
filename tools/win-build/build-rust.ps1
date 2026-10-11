param(
    [ValidateSet('Debug','Release')][string]$Configuration='Debug',
    [switch]$Check
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$msvcJson=& python -X utf8 (Join-Path $PSScriptRoot 'msvc_env.py') --json
if($LASTEXITCODE -ne 0){throw 'Cannot initialize Visual Studio x64 environment'}
$msvc=$msvcJson | ConvertFrom-Json
foreach($property in $msvc.PSObject.Properties){[Environment]::SetEnvironmentVariable($property.Name,[string]$property.Value,'Process')}
$env:Path=(Join-Path $env:USERPROFILE '.cargo/bin')+';'+$env:Path
$target='x86_64-pc-windows-msvc'
$oldLocation=Get-Location
try {
    Set-Location (Join-Path $root 'rust')
    $cargoArgs=@('build','--target',$target,'--workspace','--locked')
    if($Configuration -eq 'Release') {$cargoArgs+='--release'}
    & cargo @cargoArgs
    if($LASTEXITCODE -ne 0){throw 'Rust build failed'}
    if($Check) {
        & cargo fmt --all -- --check
        if($LASTEXITCODE -ne 0){throw 'Rust formatting failed'}
        & cargo clippy --target $target --workspace --all-targets --locked -- -D warnings
        if($LASTEXITCODE -ne 0){throw 'Rust lint failed'}
        & cargo test --target $target --workspace --locked
        if($LASTEXITCODE -ne 0){throw 'Rust tests failed'}
    }
} finally {Set-Location $oldLocation}
