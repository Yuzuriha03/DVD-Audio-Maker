param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [string]$Archive,
    [string]$Icon = (Join-Path $PSScriptRoot '..\..\assets\dvd-audio-maker.ico')
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AppIconResources {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr LoadLibraryExW(string name, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")]
    public static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")]
    public static extern bool FreeLibrary(IntPtr module);
}
'@
function Read-Resource($Module, [int]$Name, [int]$Type) {
    $resource = [AppIconResources]::FindResourceW($Module, [IntPtr]$Name, [IntPtr]$Type)
    if ($resource -eq [IntPtr]::Zero) { throw "Missing resource $Type/$Name" }
    $size = [AppIconResources]::SizeofResource($Module, $resource)
    $pointer = [AppIconResources]::LockResource([AppIconResources]::LoadResource($Module, $resource))
    $bytes = [byte[]]::new($size)
    [Runtime.InteropServices.Marshal]::Copy($pointer, $bytes, 0, $size)
    return ,$bytes
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$expected = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Icon).Path)
$module = [AppIconResources]::LoadLibraryExW($Executable, [IntPtr]::Zero, 2)
if ($module -eq [IntPtr]::Zero) { throw 'Cannot load executable resources' }
try {
    $group = Read-Resource $module 1 14
    $count = [BitConverter]::ToUInt16($expected, 4)
    if ([BitConverter]::ToUInt16($group, 4) -ne $count) { throw 'Icon frame count differs' }
    for ($index = 0; $index -lt $count; $index++) {
        $entry = 6 + 16 * $index
        $groupEntry = 6 + 14 * $index
        for ($field = 0; $field -lt 12; $field++) {
            if ($expected[$entry + $field] -ne $group[$groupEntry + $field]) {
                throw "Icon frame $index directory differs"
            }
        }
        $id = [BitConverter]::ToUInt16($group, $groupEntry + 12)
        $actual = Read-Resource $module $id 3
        $length = [BitConverter]::ToUInt32($expected, $entry + 8)
        $offset = [BitConverter]::ToUInt32($expected, $entry + 12)
        if ($actual.Length -ne $length) { throw "Icon frame $index length differs" }
        for ($byte = 0; $byte -lt $length; $byte++) {
            if ($actual[$byte] -ne $expected[$offset + $byte]) { throw "Icon frame $index pixels differ" }
        }
    }
    Write-Output "[OK] EXE icon: all $count frames match the project ICO byte-for-byte"
} finally {
    [void][AppIconResources]::FreeLibrary($module)
}
if ($Archive) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Archive).Path)
    try {
        $entries = @($zip.Entries | Where-Object { $_.Name -eq 'DVD-Audio-Maker.exe' })
        if ($entries.Count -ne 1) { throw 'ZIP must contain exactly one application EXE' }
        $stream = $entries[0].Open()
        $sha = [Security.Cryptography.SHA256]::Create()
        try {
            $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '')
            if ($hash -ne (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash) {
                throw 'ZIP executable differs from the verified EXE'
            }
        } finally {
            $sha.Dispose()
            $stream.Dispose()
        }
        Write-Output '[OK] ZIP executable matches the icon-verified EXE'
    } finally {
        $zip.Dispose()
    }
}
