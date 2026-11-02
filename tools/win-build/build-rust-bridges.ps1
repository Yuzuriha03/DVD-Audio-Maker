param(
 [ValidateSet('direct','media','author')][string]$Component='direct',
 [Parameter(Mandatory=$true)][string]$Output,
 [string]$FfmpegPrefix,
 [string]$DependencyRuntime,
 [string]$AcceptanceReport,
 [ValidateSet('x86_64-pc-windows-msvc','aarch64-pc-windows-msvc')][string]$Target='x86_64-pc-windows-msvc'
)
$ErrorActionPreference='Stop'
if($Component -ne 'direct' -and $Component -ne 'author'){Write-Warning 'DLL mode are differential adapters only, not production integration.'}
if($Component -eq 'direct' -and (Get-ChildItem $Output -Filter 'dvda-*.dll' -File -ErrorAction SilentlyContinue)){throw 'Direct output must not contain project-owned adapter DLLs'}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$BuildTriple=$Target
if($Target -eq 'aarch64-pc-windows-msvc'){throw 'ARM64 bridge builds require ARM64 FFmpeg libraries, which are not available yet; use build-rust-arm64.ps1 for the portable ARM64 checks.'}
$msvcJson=& python -X utf8 (Join-Path $PSScriptRoot 'msvc_env.py') --json
if($LASTEXITCODE -ne 0){throw 'Cannot initialize Visual Studio x64 environment'}
$msvc=$msvcJson | ConvertFrom-Json
foreach($property in $msvc.PSObject.Properties){[Environment]::SetEnvironmentVariable($property.Name,[string]$property.Value,'Process')}
$Output=[IO.Path]::GetFullPath($Output)
if($FfmpegPrefix){$FfmpegPrefix=(Resolve-Path -LiteralPath $FfmpegPrefix).Path}
New-Item -ItemType Directory -Force $Output | Out-Null
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:DVDA_FFMPEG_PREFIX=$FfmpegPrefix
$staticFfmpeg=$false
if($FfmpegPrefix){
 $vendor=Get-Content -Raw -LiteralPath (Join-Path $FfmpegPrefix 'build-manifest.json') | ConvertFrom-Json
 $staticFfmpeg=$vendor.linkage -eq 'static'
}
$cargoTarget=Join-Path $Output 'cargo-target'
$args=@('build','--offline','--release','--manifest-path',"$repo\rust\Cargo.toml",'--target',$BuildTriple,'-p','dvda-bridges','--target-dir',$cargoTarget)
switch($Component){
 direct {$args+=@('--features','media,menu','--lib')}
 media {$args+=@('--features','media','--lib','--bin','mlp-decode-probe')}
 author {$args+=@('--features','media,menu,author-iso','--lib')}
}
& cargo @args
if($LASTEXITCODE -ne 0){throw 'Rust bridge build failed'}
$release=Join-Path $cargoTarget "$BuildTriple\release"
switch($Component){
 direct {Copy-Item "$release\libdvda_bridges.rlib" $Output}
 media {Copy-Item "$release\dvda_bridges.dll" "$Output\dvda-media.dll";Copy-Item "$release\mlp-decode-probe.exe" $Output;if(!$staticFfmpeg){Copy-Item "$FfmpegPrefix\bin\*.dll" $Output}}
 author {
  $static=Join-Path $release 'dvda_bridges.lib'
  if(!(Test-Path -LiteralPath $static)){throw 'MSVC author bridge build did not produce dvda_bridges.lib.'}
  Copy-Item $static $Output
  if(!$staticFfmpeg){Copy-Item "$FfmpegPrefix\bin\*.dll" $Output}
 }
}
function Hash($path){(Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant()}
$runtimeSources=[ordered]@{}
$roots=@("$Output", "$FfmpegPrefix\bin", "$repo\build\msvc-native\media-runtime")
if($DependencyRuntime){$roots=@($Output,$DependencyRuntime)+$roots[1..($roots.Length-1)]}
$system="$env:SystemRoot\System32"
$vcRuntimeRootsJson=& python -X utf8 (Join-Path $PSScriptRoot 'msvc_env.py') --runtime-roots-json
if($LASTEXITCODE -ne 0){throw 'Cannot locate Visual Studio redistributable runtime'}
$vcRuntimeRoots=@($vcRuntimeRootsJson | ConvertFrom-Json)
$vcRuntimeRoots=@($vcRuntimeRoots | ForEach-Object {[string]$_})
function IsVcRuntime($name){$name -match '^(vcruntime140|msvcp140|concrt140).*\.dll$'}
function Imports($path){
 $lines=& dumpbin.exe /nologo /dependents $path
 if($LASTEXITCODE -ne 0){throw "Cannot inspect PE imports: $path"}
 @($lines | Select-String '^\s+([^\s]+\.dll)\s*$' | ForEach-Object {$_.Matches[0].Groups[1].Value.Trim()})
}
$pending=New-Object 'System.Collections.Generic.Queue[string]'
foreach($file in Get-ChildItem $Output -File | Where-Object {$_.Extension -in @('.dll','.exe')}){$pending.Enqueue($file.FullName)}
$seen=@{}
while($pending.Count){
 $file=$pending.Dequeue()
 if($seen.ContainsKey($file.ToLowerInvariant())){continue}
 $seen[$file.ToLowerInvariant()]=$true
 foreach($import in Imports $file){
  if($import -match '^(api-ms-win-|ext-ms-win-)' -or (!(IsVcRuntime $import) -and (Test-Path (Join-Path $system $import)))){continue}
  $destination=Join-Path $Output $import
  $source=$null
  if(IsVcRuntime $import){
   $source=Get-ChildItem -Path $vcRuntimeRoots -Filter ([IO.Path]::GetFileName($import)) -File -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
   if(!$source -and $env:VCToolsRedistDir){$source=Get-ChildItem -Path $env:VCToolsRedistDir -Recurse -Filter ([IO.Path]::GetFileName($import)) -File -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName}
  }
  if(!$source){
   foreach($root in @($roots)){$candidate=Join-Path ([string]$root) ([string]$import);if(Test-Path -LiteralPath $candidate){$source=[string]$candidate;break}}
  }
  if(!$source){throw "Unresolved $Component dependency $import required by $file"}
  if([IO.Path]::GetFullPath($source) -ne [IO.Path]::GetFullPath($destination)){Copy-Item $source $destination}
  $runtimeSources[$import]=@{path=[IO.Path]::GetFullPath($source);sha256=Hash $source}
  $pending.Enqueue($destination)
 }
}
$inputs=[ordered]@{}
foreach($file in Get-ChildItem "$repo\rust\crates\dvda-bridges","$repo\rust\crates\dvda-image" -Recurse -File){$inputs[$file.FullName.Substring($repo.Length+1)]=Hash $file.FullName}
if($Component -eq 'author'){
 foreach($file in Get-ChildItem "$repo\rust\crates\dvda-author" -Recurse -File){$inputs[$file.FullName.Substring($repo.Length+1)]=Hash $file.FullName}
}
if($Component -in @('direct','author')){
 foreach($file in Get-ChildItem "$repo\rust\crates\dvda-menu","$repo\tools\menu-native" -Recurse -File){$inputs[$file.FullName.Substring($repo.Length+1)]=Hash $file.FullName}
}

foreach($name in @('Cargo.toml','Cargo.lock')){$inputs["rust\$name"]=Hash "$repo\rust\$name"}
$inputs['.cargo\config.toml']=Hash "$repo\.cargo\config.toml"
$inputs['rust\.cargo\config.toml']=Hash "$repo\rust\.cargo\config.toml"
$inputs['builder']=Hash $PSCommandPath
$inputs['tools\win-build\msvc_env.py']=Hash (Join-Path $PSScriptRoot 'msvc_env.py')
$dependencies=[ordered]@{}
if($FfmpegPrefix){foreach($root in @('include','lib','bin')){$path=Join-Path $FfmpegPrefix $root;if(Test-Path -LiteralPath $path){foreach($file in Get-ChildItem -LiteralPath $path -Recurse -File){$dependencies[$file.FullName]=Hash $file.FullName}}}}
$files=[ordered]@{}
foreach($file in Get-ChildItem $Output -File){if($file.Name -notlike '*build.json'){$entry=@{sha256=Hash $file.FullName;bytes=$file.Length};if($file.Extension -in @('.dll','.exe')){$entry.imports=@(Imports $file.FullName)};$files[$file.Name]=$entry}}
$name=switch($Component){direct{'direct-bridge-build.json'}media{'media-build.json'}author{'author-bridge-build.json'}}
$features=switch($Component){direct{@('media','menu')}media{@('media')}author{@('media','menu','author-iso')}}
$evidence=$null
if($AcceptanceReport){$evidence=@{path=[IO.Path]::GetFullPath($AcceptanceReport);sha256=Hash $AcceptanceReport}}
$record=@{schema_version=1;implementation='project-owned-rust';component=$Component;features=@($features);target=$BuildTriple;toolchain='msvc';compiler=(& rustc --version);cargo_arguments=@($args);source_inputs=$inputs;third_party_inputs=$dependencies;runtime_dependency_inputs=$runtimeSources;files=$files;acceptance_evidence=$evidence;production_accepted=$false}
if($Component -eq 'media'){
 $vendorManifest=Join-Path $FfmpegPrefix 'build-manifest.json'
 $vendor=Get-Content $vendorManifest -Raw | ConvertFrom-Json
 if($vendor.profile -notin @('shared','static')){throw 'Media requires the authenticated complete FFmpeg profile'}
 if($vendor.target -ne $BuildTriple -or $vendor.toolchain -ne 'msvc'){throw 'Media requires FFmpeg built with Windows x64 MSVC'}
 $record.profile=$vendor.profile
 $record.ffmpeg_linkage=if($staticFfmpeg){'static'}else{'shared'}
 $record.third_party_inputs[$vendorManifest]=Hash $vendorManifest
}
$record | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $Output $name)
Write-Output "Rust $Component bridge runtime: $Output; manifest: $name (acceptance remains gated)"
