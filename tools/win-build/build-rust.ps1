param(
    [ValidateSet('Debug','Release')][string]$Configuration='Debug',
    [string]$MsysRoot='C:/msys64',
    [switch]$Check
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$compiler=Join-Path $MsysRoot 'mingw64/bin'
foreach($name in @('gcc.exe','g++.exe','ar.exe')) {
    if(-not (Test-Path -LiteralPath (Join-Path $compiler $name))) { throw "Missing existing MSYS2 compiler: $name" }
}
$env:Path=(Join-Path $env:USERPROFILE '.cargo/bin')+';'+$compiler+';'+$env:Path
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=(Join-Path $compiler 'gcc.exe')
$env:CC_x86_64_pc_windows_gnu=(Join-Path $compiler 'gcc.exe')
$env:CXX_x86_64_pc_windows_gnu=(Join-Path $compiler 'g++.exe')
$env:AR_x86_64_pc_windows_gnu=(Join-Path $compiler 'ar.exe')
$oldLocation=Get-Location
try {
    Set-Location (Join-Path $root 'rust')
    $cargoArgs=@('build','--workspace','--locked')
    if($Configuration -eq 'Release') {$cargoArgs+='--release'}
    & cargo @cargoArgs
    if($LASTEXITCODE -ne 0){throw 'Rust build failed'}
    if($Check) {
        & cargo fmt --all -- --check
        if($LASTEXITCODE -ne 0){throw 'Rust formatting failed'}
        & cargo clippy --workspace --all-targets --locked -- -D warnings
        if($LASTEXITCODE -ne 0){throw 'Rust lint failed'}
        & cargo test --workspace --locked
        if($LASTEXITCODE -ne 0){throw 'Rust tests failed'}
    }
} finally {Set-Location $oldLocation}
