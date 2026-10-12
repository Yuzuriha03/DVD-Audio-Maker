param(
 [ValidateSet('direct','media','image','author','shim')][string]$Component='direct',
 [Parameter(Mandatory=$true)][string]$Output,
 [string]$FfmpegPrefix,
 [string]$MagickWork,
 [string]$ImageDependencies,
 [string]$ImageOracle,
 [string]$MsysRoot='C:\msys64',
 [string]$DependencyRuntime,
 [string]$AcceptanceReport,
 [switch]$DirectMenu,
 [switch]$LegacyImageLoader
)
$ErrorActionPreference='Stop'
if($Component -ne 'direct' -and $Component -ne 'author'){Write-Warning 'DLL/shim modes are differential adapters only, not production integration.'}
if($Component -eq 'direct' -and (Get-ChildItem $Output -Filter 'dvda-*.dll' -File -ErrorAction SilentlyContinue)){throw 'Direct output must not contain project-owned adapter DLLs'}
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$BuildTriple='x86_64-pc-windows-msvc'
$msvcJson=& python -X utf8 (Join-Path $PSScriptRoot 'msvc_env.py') --json
if($LASTEXITCODE -ne 0){throw 'Cannot initialize Visual Studio x64 environment'}
$msvc=$msvcJson | ConvertFrom-Json
foreach($property in $msvc.PSObject.Properties){[Environment]::SetEnvironmentVariable($property.Name,[string]$property.Value,'Process')}
if(!$ImageDependencies){$ImageDependencies=if($env:DVDA_IMAGE_DEPS_DIR){$env:DVDA_IMAGE_DEPS_DIR}elseif($MagickWork){Join-Path $MagickWork 'lib'}else{$null}}
$Output=[IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force $Output | Out-Null
$needsMenu = $Component -eq 'direct' -or ($Component -eq 'author' -and -not $LegacyImageLoader)
if($needsMenu){
 if(!$env:DVDA_MENU_NATIVE_DIR){throw "MSVC-only $Component build requires DVDA_MENU_NATIVE_DIR pointing to an MSVC-native menu vendor build; GNU/MinGW archives are not link-compatible."}
 $menuManifest=Join-Path $env:DVDA_MENU_NATIVE_DIR 'menu-build.json'
 if(!(Test-Path -LiteralPath $menuManifest)){throw "MSVC-only $Component build cannot authenticate menu vendor ABI: missing $menuManifest"}
 $menuRecord=Get-Content -Raw -LiteralPath $menuManifest | ConvertFrom-Json
 if($menuRecord.target -ne $BuildTriple -or $menuRecord.toolchain -ne 'msvc'){
  throw "MSVC-only $Component build requires menu-build.json target=$BuildTriple/toolchain=msvc; GNU/MinGW vendor archives are not link-compatible."
 }
}
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:DVDA_FFMPEG_PREFIX=$FfmpegPrefix
$env:DVDA_MAGICK_WORK=$MagickWork
$env:DVDA_IMAGE_DEPS_DIR=$ImageDependencies
$staticFfmpeg=$false
if($FfmpegPrefix){
 $vendor=Get-Content -Raw -LiteralPath (Join-Path $FfmpegPrefix 'build-manifest.json') | ConvertFrom-Json
 $staticFfmpeg=$vendor.linkage -eq 'static'
}
$cargoTarget=Join-Path $Output 'cargo-target'
$args=@('build','--offline','--release','--manifest-path',"$repo\rust\Cargo.toml",'--target',$BuildTriple,'-p','dvda-bridges','--target-dir',$cargoTarget)
switch($Component){
 direct {$args+=@('--features','media,image,menu','--lib')}
 media {$args+=@('--features','media','--lib','--bin','mlp-decode-probe')}
 image {$args+=@('--features','image','--lib')}
 author {if($LegacyImageLoader){$args+=@('--features','media,author-loader,author-iso','--lib')}else{$args+=@('--features','media,image,menu,author-iso','--lib')}}
 shim {$args+=@('--bin','magick-shim')}
}
if($Component -eq 'author' -and $DirectMenu -and $LegacyImageLoader){$args+=@('--features','menu')}
& cargo @args
if($LASTEXITCODE -ne 0){throw 'Rust bridge build failed'}
$release=Join-Path $cargoTarget "$BuildTriple\release"
switch($Component){
 direct {Copy-Item "$release\libdvda_bridges.rlib" $Output;if($ImageOracle){foreach($name in @('policy.xml','colors.xml','NOTICE.txt')){Copy-Item (Join-Path $ImageOracle $name) $Output}}}
 media {Copy-Item "$release\dvda_bridges.dll" "$Output\dvda-media.dll";Copy-Item "$release\mlp-decode-probe.exe" $Output;if(!$staticFfmpeg){Copy-Item "$FfmpegPrefix\bin\*.dll" $Output}}
 image {Copy-Item "$release\dvda_bridges.dll" "$Output\dvda-image.dll";if($ImageOracle){foreach($name in @('policy.xml','colors.xml','NOTICE.txt')){Copy-Item (Join-Path $ImageOracle $name) $Output}}}
 author {
  $static=Join-Path $release 'dvda_bridges.lib'
  if(!(Test-Path -LiteralPath $static)){throw 'MSVC author bridge build did not produce dvda_bridges.lib.'}
  Copy-Item $static $Output
  if(!$staticFfmpeg){Copy-Item "$FfmpegPrefix\bin\*.dll" $Output}
 }
 shim {Copy-Item "$release\magick-shim.exe" $Output}
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
foreach($file in Get-ChildItem "$repo\rust\crates\dvda-bridges" -Recurse -File){$inputs[$file.FullName.Substring($repo.Length+1)]=Hash $file.FullName}
if($Component -eq 'author'){
 foreach($file in Get-ChildItem "$repo\rust\crates\dvda-author" -Recurse -File){$inputs[$file.FullName.Substring($repo.Length+1)]=Hash $file.FullName}
}
if($Component -eq 'direct' -or ($Component -eq 'author' -and ($DirectMenu -or !$LegacyImageLoader))){
 foreach($file in Get-ChildItem "$repo\rust\crates\dvda-menu","$repo\tools\menu-native" -Recurse -File){$inputs[$file.FullName.Substring($repo.Length+1)]=Hash $file.FullName}
 $inputs['tools\win-build\build-menu-runtime.py']=Hash "$repo\tools\win-build\build-menu-runtime.py"
 $menuVendor=$env:DVDA_MENU_NATIVE_DIR
 if(!$menuVendor){$manifests=@(Get-ChildItem "$cargoTarget\$BuildTriple\release\build\dvda-menu-*\out\menu-vendor\menu-build.json" -ErrorAction SilentlyContinue);if($manifests.Count -ne 1){throw 'Cannot identify the embedded menu vendor manifest'};$menuVendor=$manifests[0].DirectoryName}
 foreach($name in @('dvda_menu_spu_vendor.lib','dvda_menu_nav_vendor.lib','menu-build.json')){$file=Get-Item (Join-Path $menuVendor $name);$inputs[$file.FullName]=Hash $file.FullName}
}
foreach($name in @('Cargo.toml','Cargo.lock')){$inputs["rust\$name"]=Hash "$repo\rust\$name"}
$inputs['.cargo\config.toml']=Hash "$repo\.cargo\config.toml"
$inputs['rust\.cargo\config.toml']=Hash "$repo\rust\.cargo\config.toml"
$inputs['builder']=Hash $PSCommandPath
$inputs['tools\win-build\msvc_env.py']=Hash (Join-Path $PSScriptRoot 'msvc_env.py')
$dependencies=[ordered]@{}
if($FfmpegPrefix){foreach($root in @('include','lib','bin')){$path=Join-Path $FfmpegPrefix $root;if(Test-Path -LiteralPath $path){foreach($file in Get-ChildItem -LiteralPath $path -Recurse -File){$dependencies[$file.FullName]=Hash $file.FullName}}}}
if($MagickWork){foreach($pattern in @('compile\MagickWand\.libs\*.lib','compile\MagickCore\.libs\*.lib','freetype-minimal.lib','image-build.json','compile\config\*.h','ImageMagick-7.0.8-47\MagickCore\*.h')){foreach($file in Get-ChildItem (Join-Path $MagickWork $pattern) -ErrorAction SilentlyContinue){$dependencies[$file.FullName]=Hash $file.FullName}};foreach($name in @('jpeg.lib','png16.lib','webpdecoder.lib','webpmux.lib','z.lib')){$path=Join-Path $ImageDependencies $name;if(!(Test-Path -LiteralPath $path)){throw "MSVC-native image codec dependency is missing: $path"};$dependencies[$path]=Hash $path}}
$files=[ordered]@{}
foreach($file in Get-ChildItem $Output -File){if($file.Name -notlike '*build.json'){$entry=@{sha256=Hash $file.FullName;bytes=$file.Length};if($file.Extension -in @('.dll','.exe')){$entry.imports=@(Imports $file.FullName)};$files[$file.Name]=$entry}}
$name=switch($Component){direct{'direct-bridge-build.json'}media{'media-build.json'}image{'image-build.json'}author{'author-bridge-build.json'}shim{'shim-build.json'}}
$features=switch($Component){direct{@('media','image','menu')}media{@('media')}image{@('image')}author{if($LegacyImageLoader){@('media','author-loader','author-iso');if($DirectMenu){'menu'}}else{@('media','image','menu','author-iso')}}shim{@()}}
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
