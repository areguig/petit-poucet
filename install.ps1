# Installs the petit-poucet binary on Windows: the latest release, or $env:PETIT_POUCET_VERSION. Re-run it to upgrade.
#   irm https://raw.githubusercontent.com/areguig/petit-poucet/main/install.ps1 | iex
$ErrorActionPreference = 'Stop'

function Log($message) { [Console]::Error.WriteLine("petit-poucet install: $message") }
function Fail($message) { Log $message; exit 1 }
# .NET rather than Get-FileHash: that cmdlet's module may not load when PSModulePath comes from another PowerShell.
function Get-Sha256($path) {
    $stream = [System.IO.File]::OpenRead($path)
    try { -join ([System.Security.Cryptography.SHA256]::Create().ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) }
    finally { $stream.Dispose() }
}

$target = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    'X64' { 'x86_64-pc-windows-msvc' }
    'Arm64' { 'aarch64-pc-windows-msvc' }
    default { Fail "no release for Windows $_" }
}

$releases = if ($env:PETIT_POUCET_RELEASES) { $env:PETIT_POUCET_RELEASES } else { 'https://github.com/areguig/petit-poucet/releases' }
$asset = "petit-poucet-$target.exe"
$url = if ($env:PETIT_POUCET_VERSION) {
    "$releases/download/v$($env:PETIT_POUCET_VERSION.TrimStart('v'))/$asset"
} else {
    "$releases/latest/download/$asset"
}
$dir = if ($env:PETIT_POUCET_INSTALL_DIR) { $env:PETIT_POUCET_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }

New-Item -ItemType Directory -Force -Path $dir | Out-Null
$tmp = Join-Path $dir ".petit-poucet.download.$PID"
# WebClient, like other installers' scripts: it works in Windows PowerShell 5.1 and PowerShell 7, file:// URLs included.
$client = New-Object System.Net.WebClient
try {
    Log "downloading $url"
    try { $client.DownloadFile($url, $tmp) } catch { Fail "download failed: $url" }
    try { $expected = ($client.DownloadString("$url.sha256") -split '\s+')[0] } catch { Fail "checksum download failed: $url.sha256" }
    $actual = Get-Sha256 $tmp
    if (-not $expected -or $actual -ne $expected.ToLower()) {
        Fail "checksum mismatch for $url (expected $expected, got $actual)"
    }
    $exe = Join-Path $dir 'petit-poucet.exe'
    Move-Item -Force $tmp $exe
} finally {
    Remove-Item -Force -ErrorAction SilentlyContinue $tmp
}

Log "installed $(& $exe --version) in $dir"
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (($userPath -split ';') -contains $dir)) {
    if ($env:PETIT_POUCET_NO_MODIFY_PATH) {
        Log "$dir is not on your PATH: add it to your user Path"
    } else {
        [Environment]::SetEnvironmentVariable('Path', (@($dir, $userPath) | Where-Object { $_ }) -join ';', 'User')
        Log "added $dir to your user Path: open a new terminal to use petit-poucet"
    }
}
Log 'next: run `petit-poucet setup` to create your memory vault and set up your agents'
