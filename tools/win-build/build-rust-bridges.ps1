param(
 [ValidateSet('direct','media','image','author','shim')][string]$Component='direct',
 [Parameter(Mandatory=$true)][string]$Output,
 [string]$FfmpegPrefix,
 [string]$MagickWork,
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
$Output=[IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force $Output | Out-Null
$env:PATH="$env:USERPROFILE\.cargo\bin;$MsysRoot\mingw64\bin;$env:PATH"
$env:DVDA_FFMPEG_PREFIX=$FfmpegPrefix
$env:DVDA_MAGICK_WORK=$MagickWork
$env:DVDA_MSYS_ROOT=$MsysRoot
$target=Join-Path $Output 'cargo-target'
$args=@('build','--offline','--release','--manifest-path',"$repo\rust\Cargo.toml",'-p','dvda-bridges','--target-dir',$target)
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
$release=Join-Path $target 'release'
switch($Component){
 direct {Copy-Item "$release\libdvda_bridges.rlib" $Output;if($ImageOracle){foreach($name in @('policy.xml','colors.xml','NOTICE.txt')){Copy-Item (Join-Path $ImageOracle $name) $Output}}}
 media {Copy-Item "$release\dvda_bridges.dll" "$Output\dvda-media.dll";Copy-Item "$release\mlp-decode-probe.exe" $Output;Copy-Item "$FfmpegPrefix\bin\*.dll" $Output}
 image {Copy-Item "$release\dvda_bridges.dll" "$Output\dvda-image.dll";if($ImageOracle){foreach($name in @('policy.xml','colors.xml','NOTICE.txt')){Copy-Item (Join-Path $ImageOracle $name) $Output}}}
 author {Copy-Item "$release\libdvda_bridges.a" $Output;Copy-Item "$FfmpegPrefix\bin\*.dll" $Output}
 shim {Copy-Item "$release\magick-shim.exe" $Output}
}
function Hash($path){(Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant()}
$runtimeSources=[ordered]@{}
$roots=@("$Output", "$FfmpegPrefix\bin", "$repo\build\media-native-fixed", "$MsysRoot\mingw64\bin")
if($DependencyRuntime){$roots=@($Output,$DependencyRuntime)+$roots[1..($roots.Length-1)]}
$system="$env:SystemRoot\System32"
function Imports($path){
 $lines=& "$MsysRoot\mingw64\bin\objdump.exe" -p $path
 if($LASTEXITCODE -ne 0){throw "Cannot inspect PE imports: $path"}
 @($lines | Select-String 'DLL Name:\s*(.+)$' | ForEach-Object {$_.Matches[0].Groups[1].Value.Trim()})
}
$pending=New-Object 'System.Collections.Generic.Queue[string]'
foreach($file in Get-ChildItem $Output -File | Where-Object {$_.Extension -in @('.dll','.exe')}){$pending.Enqueue($file.FullName)}
$seen=@{}
while($pending.Count){
 $file=$pending.Dequeue()
 if($seen.ContainsKey($file.ToLowerInvariant())){continue}
 $seen[$file.ToLowerInvariant()]=$true
 foreach($import in Imports $file){
  if($import -match '^(api-ms-win-|ext-ms-win-)' -or (Test-Path (Join-Path $system $import))){continue}
  $destination=Join-Path $Output $import
  $source=$null
  foreach($root in $roots){$candidate=Join-Path $root $import;if(Test-Path $candidate){$source=$candidate;break}}
  if(!$source){throw "Unresolved $Component dependency $import required by $file"}
  if([IO.Path]::GetFullPath($source) -ne [IO.Path]::GetFullPath($destination)){Copy-Item $source $destination;$runtimeSources[$import]=@{path=[IO.Path]::GetFullPath($source);sha256=Hash $source}}
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
 if(!$menuVendor){$manifests=@(Get-ChildItem "$target\release\build\dvda-menu-*\out\menu-vendor\menu-build.json" -ErrorAction SilentlyContinue);if($manifests.Count -ne 1){throw 'Cannot identify the embedded menu vendor manifest'};$menuVendor=$manifests[0].DirectoryName}
 foreach($name in @('libdvda_menu_spu_vendor.a','libdvda_menu_nav_vendor.a','menu-build.json')){$file=Get-Item (Join-Path $menuVendor $name);$inputs[$file.FullName]=Hash $file.FullName}
}
foreach($name in @('Cargo.toml','Cargo.lock')){$inputs["rust\$name"]=Hash "$repo\rust\$name"}
$inputs['builder']=Hash $PSCommandPath
$dependencies=[ordered]@{}
if($FfmpegPrefix){foreach($file in Get-ChildItem "$FfmpegPrefix\include","$FfmpegPrefix\lib","$FfmpegPrefix\bin" -Recurse -File){$dependencies[$file.FullName]=Hash $file.FullName}}
if($MagickWork){foreach($pattern in @('compile\MagickWand\.libs\*.a','compile\MagickCore\.libs\*.a','libfreetype-minimal.a','libucrt-jump.a','image-build.json','compile\config\*.h','ImageMagick-7.0.8-47\MagickCore\*.h')){foreach($file in Get-ChildItem (Join-Path $MagickWork $pattern) -ErrorAction SilentlyContinue){$dependencies[$file.FullName]=Hash $file.FullName}};foreach($name in @('libjpeg.a','libpng16.a','libwebpdecoder.a','libwebpmux.a','libz.a')){$path="$MsysRoot\mingw64\lib\$name";$dependencies[$path]=Hash $path}}
$files=[ordered]@{}
foreach($file in Get-ChildItem $Output -File){if($file.Name -notlike '*build.json'){$entry=@{sha256=Hash $file.FullName;bytes=$file.Length};if($file.Extension -in @('.dll','.exe')){$entry.imports=@(Imports $file.FullName)};$files[$file.Name]=$entry}}
$name=switch($Component){direct{'direct-bridge-build.json'}media{'media-build.json'}image{'image-build.json'}author{'author-bridge-build.json'}shim{'shim-build.json'}}
$features=switch($Component){direct{@('media','image','menu')}media{@('media')}image{@('image')}author{if($LegacyImageLoader){@('media','author-loader','author-iso');if($DirectMenu){'menu'}}else{@('media','image','menu','author-iso')}}shim{@()}}
$evidence=$null
if($AcceptanceReport){$evidence=@{path=[IO.Path]::GetFullPath($AcceptanceReport);sha256=Hash $AcceptanceReport}}
$record=@{schema_version=1;implementation='project-owned-rust';component=$Component;features=@($features);target='x86_64-pc-windows-gnu';compiler=(& rustc --version);cargo_arguments=@($args);source_inputs=$inputs;third_party_inputs=$dependencies;runtime_dependency_inputs=$runtimeSources;files=$files;acceptance_evidence=$evidence;production_accepted=$false}
if($Component -eq 'media'){
 $vendorManifest=Join-Path $FfmpegPrefix 'build-manifest.json'
 $vendor=Get-Content $vendorManifest -Raw | ConvertFrom-Json
 if($vendor.profile -ne 'shared'){throw 'Media requires the authenticated shared FFmpeg profile'}
 $record.profile=$vendor.profile
 $record.third_party_inputs[$vendorManifest]=Hash $vendorManifest
}
$record | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $Output $name)
Write-Output "Rust $Component bridge runtime: $Output; manifest: $name (acceptance remains gated)"
