param(
  [ValidateSet('test', 'installer')]
  [string]$Mode = 'installer'
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$PackageDirectory = Join-Path $ProjectRoot 'artifacts'
$ReleaseExecutable = Join-Path $ProjectRoot 'src-tauri\target\release\windows-client.exe'
$TauriConfig = Get-Content -LiteralPath (Join-Path $ProjectRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$ProductVersion = [string]$TauriConfig.version
$Installer = Join-Path $ProjectRoot "src-tauri\target\release\bundle\nsis\MyLIST_${ProductVersion}_x64-setup.exe"

function Invoke-Checked([string]$Command, [string[]]$Arguments) {
  & $Command @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$Command failed with exit code $LASTEXITCODE"
  }
}

function Write-ArtifactSummary([System.IO.FileInfo]$File) {
  $stream = [System.IO.File]::OpenRead($File.FullName)
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $hash = [System.BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '')
  } finally {
    $sha256.Dispose()
    $stream.Dispose()
  }
  Write-Output "Artifact: $($File.FullName)"
  Write-Output "Size: $([math]::Round($File.Length / 1MB, 2)) MB ($($File.Length) bytes)"
  Write-Output "Modified: $($File.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss'))"
  Write-Output "SHA-256: $hash"
}

Push-Location $ProjectRoot
try {
  New-Item -ItemType Directory -Force -Path $PackageDirectory | Out-Null

  if ($Mode -eq 'test') {
    Invoke-Checked 'npm' @('run', 'build:debug-client')
    $debugExecutable = Join-Path $ProjectRoot 'src-tauri\target\debug\windows-client.exe'
    if (!(Test-Path -LiteralPath $debugExecutable)) {
      throw "Test executable was not generated: $debugExecutable"
    }
    $output = Join-Path $PackageDirectory 'MyLIST-test.exe'
    Copy-Item -LiteralPath $debugExecutable -Destination $output -Force
    Write-Output 'Package mode: test executable (no installer)'
    Write-ArtifactSummary (Get-Item -LiteralPath $output)
    exit 0
  }

  Invoke-Checked 'npm' @('run', 'build:web-installer')
  if (!(Test-Path -LiteralPath $Installer)) {
    throw "Installer was not generated: $Installer"
  }

  $installerFile = Get-Item -LiteralPath $Installer
  $version = $installerFile.VersionInfo
  if ($version.ProductName -ne 'MyLIST Installer' -or $version.FileDescription -ne 'MyLIST Installer') {
    throw 'Packaging rejected: output is not the custom MyLIST web installer.'
  }
  if ($installerFile.Length -lt 5MB) {
    throw 'Packaging rejected: output is too small and is likely the NSIS core without the custom UI.'
  }

  $output = Join-Path $PackageDirectory $installerFile.Name
  Copy-Item -LiteralPath $installerFile.FullName -Destination $output -Force
  Write-Output 'Package mode: full installer with custom install and uninstall UI'
  Write-ArtifactSummary (Get-Item -LiteralPath $output)
} finally {
  Pop-Location
}
