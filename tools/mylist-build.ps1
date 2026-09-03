param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('Test', 'Release', 'List', 'CleanTemp')]
  [string]$Mode,
  [string]$Version = '',
  [ValidateRange(1, 3650)]
  [int]$RetentionDays = 30,
  [switch]$ConfirmCleanup
)

$ErrorActionPreference = 'Stop'
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ClientRoot = Join-Path $RepoRoot 'app\windows-client'
$ReleaseRoot = Join-Path $RepoRoot 'releases'
$TempRoot = Join-Path $RepoRoot 'temp'
$IndexPath = Join-Path $ReleaseRoot 'version-index.json'

if (!(Test-Path -LiteralPath (Join-Path $RepoRoot '.git')) -or !(Test-Path -LiteralPath (Join-Path $ClientRoot 'package.json'))) {
  throw "The script is not inside a valid MyLIST repository: $RepoRoot"
}

function Invoke-Checked([string]$Command, [string[]]$Arguments, [string]$WorkingDirectory = $RepoRoot) {
  Push-Location $WorkingDirectory
  try {
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
  } finally { Pop-Location }
}

function Get-TomlVersion([string]$Path) {
  $match = [regex]::Match((Get-Content -LiteralPath $Path -Raw), '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"')
  if (!$match.Success) { throw "Cannot read package version from $Path" }
  return $match.Groups[1].Value
}

function Get-ProductVersion {
  $appSourcePath = Join-Path $ClientRoot 'src\App.tsx'
  $appSource = Get-Content -LiteralPath $appSourcePath -Raw
  if ($appSource -match 'PRODUCT_VERSION\s*=\s*["'']\d+\.\d+\.\d+["'']') {
    throw "Hard-coded UI product version found in $appSourcePath. Read the runtime Tauri version instead."
  }
  $versions = @(
    [string](Get-Content -LiteralPath (Join-Path $ClientRoot 'package.json') -Raw | ConvertFrom-Json).version
    [string](Get-Content -LiteralPath (Join-Path $ClientRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
    (Get-TomlVersion (Join-Path $ClientRoot 'src-tauri\Cargo.toml'))
    [string](Get-Content -LiteralPath (Join-Path $ClientRoot 'installer-shell\package.json') -Raw | ConvertFrom-Json).version
    [string](Get-Content -LiteralPath (Join-Path $ClientRoot 'installer-shell\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
    (Get-TomlVersion (Join-Path $ClientRoot 'installer-shell\src-tauri\Cargo.toml'))
  )
  $unique = @($versions | Sort-Object -Unique)
  if ($unique.Count -ne 1) { throw "Version mismatch: $($versions -join ', ')" }
  if ($unique[0] -notmatch '^\d+\.\d+\.\d+$') { throw "Invalid version $($unique[0])" }
  return [string]$unique[0]
}

function Get-DirectoryBytes([string]$Path) {
  if (!(Test-Path -LiteralPath $Path)) { return [int64]0 }
  $sum = (Get-ChildItem -LiteralPath $Path -File -Recurse | Measure-Object Length -Sum).Sum
  if ($null -eq $sum) { return [int64]0 }
  return [int64]$sum
}

function Update-VersionIndex {
  New-Item -ItemType Directory -Force -Path $ReleaseRoot | Out-Null
  $items = @()
  Get-ChildItem -LiteralPath $ReleaseRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object Name -Match '^\d+\.\d+$' | ForEach-Object {
      Get-ChildItem -LiteralPath $_.FullName -Directory -ErrorAction SilentlyContinue |
        Where-Object Name -Match '^\d+\.\d+\.\d+$' | ForEach-Object {
          $infoPath = Join-Path $_.FullName 'build-info.json'
          $info = if (Test-Path $infoPath) { Get-Content $infoPath -Raw | ConvertFrom-Json } else { $null }
          $installer = Get-ChildItem $_.FullName -Filter 'MyLIST_*_x64-setup.exe' -File | Select-Object -First 1
          $items += [pscustomobject]@{
            version = $_.Name
            builtAt = if ($info) { $info.builtAt } else { $null }
            gitCommit = if ($info) { $info.gitCommit } else { $null }
            installerBytes = if ($installer) { $installer.Length } else { 0 }
            archiveBytes = Get-DirectoryBytes $_.FullName
            path = $_.FullName
          }
        }
    }
  $items = @($items | Sort-Object { [version]$_.version })
  $total = ($items | Measure-Object archiveBytes -Sum).Sum
  if ($null -eq $total) { $total = 0 }
  $index = [ordered]@{
    generatedAt = (Get-Date).ToString('o')
    retentionRule = 'Keep the latest patch for each major.minor line.'
    totalBytes = [int64]$total
    versions = $items
  }
  $index | ConvertTo-Json -Depth 5 | Set-Content $IndexPath -Encoding UTF8
  return $index
}

function Show-VersionIndex {
  $index = Update-VersionIndex
  Write-Output 'MyLIST retained releases'
  if ($index.versions.Count -eq 0) { Write-Output 'No archived releases.' }
  $index.versions | ForEach-Object {
    Write-Output ("{0,-10} installer {1,8:N2} MB   archive {2,8:N2} MB" -f $_.version, ($_.installerBytes / 1MB), ($_.archiveBytes / 1MB))
  }
  Write-Output ("Total archive size: {0:N2} MB" -f ($index.totalBytes / 1MB))
  Write-Output "Index: $IndexPath"
}

$ProductVersion = Get-ProductVersion
New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null
if ($Mode -eq 'List') { Show-VersionIndex; exit 0 }

if ($Mode -eq 'CleanTemp') {
  $cutoff = (Get-Date).AddDays(-$RetentionDays)
  $candidates = @(Get-ChildItem -LiteralPath $TempRoot -Force | Where-Object LastWriteTime -LT $cutoff)
  $bytes = [int64]0
  foreach ($item in $candidates) {
    $bytes += if ($item.PSIsContainer) { Get-DirectoryBytes $item.FullName } else { $item.Length }
  }
  Write-Output ("Temporary items older than {0} days: {1}, {2:N2} MB" -f $RetentionDays, $candidates.Count, ($bytes / 1MB))
  $candidates | ForEach-Object { Write-Output $_.FullName }
  if (!$ConfirmCleanup) {
    Write-Output 'Preview only. Nothing was deleted. Use -ConfirmCleanup after user approval.'
    exit 0
  }
  foreach ($item in $candidates) {
    if (!$item.FullName.StartsWith(($TempRoot + '\'), [StringComparison]::OrdinalIgnoreCase)) {
      throw "Cleanup target escaped temp root: $($item.FullName)"
    }
    Remove-Item -LiteralPath $item.FullName -Recurse -Force
  }
  Write-Output ("Deleted {0} temporary items and freed approximately {1:N2} MB." -f $candidates.Count, ($bytes / 1MB))
  exit 0
}

if ($Mode -eq 'Test') {
  Invoke-Checked 'npm' @('run', 'package:test') $ClientRoot
  $testExe = Join-Path $ClientRoot 'artifacts\MyLIST-test.exe'
  if (!(Test-Path $testExe)) { throw "Test EXE missing: $testExe" }
  Write-Output 'RESULT: TEST EXE (NOT AN INSTALLER)'
  Write-Output "Version: $ProductVersion"
  Write-Output "File: $testExe"
  Write-Output ("Size: {0:N2} MB" -f ((Get-Item $testExe).Length / 1MB))
  Write-Output "SHA-256: $((Get-FileHash $testExe -Algorithm SHA256).Hash)"
  exit 0
}

if (!$Version) { throw 'Release mode requires -Version x.y.z' }
if ($Version -ne $ProductVersion) { throw "Requested $Version but product files contain $ProductVersion." }
$gitStatus = & git -c "safe.directory=$RepoRoot" -C $RepoRoot status --porcelain
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect Git status.' }
if ($gitStatus) { throw 'Uncommitted changes exist. Commit and verify them before a formal release.' }
$gitCommit = (& git -c "safe.directory=$RepoRoot" -C $RepoRoot rev-parse HEAD).Trim()
if (!$gitCommit) { throw 'Cannot identify the source revision.' }

Invoke-Checked 'npm' @('run', 'package:installer') $ClientRoot
$installerName = "MyLIST_$($ProductVersion)_x64-setup.exe"
$builtInstaller = Join-Path $ClientRoot "artifacts\$installerName"
if (!(Test-Path $builtInstaller)) { throw "Formal installer missing: $builtInstaller" }
$installer = Get-Item $builtInstaller
if ($installer.VersionInfo.ProductName -ne 'MyLIST Installer' -or $installer.VersionInfo.FileDescription -ne 'MyLIST Installer') {
  throw 'Rejected: output is not the custom MyLIST installer UI.'
}
if ($installer.Length -lt 5MB) { throw 'Rejected: output may not contain the custom UI.' }

$parts = $ProductVersion.Split('.')
$line = "$($parts[0]).$($parts[1])"
$lineDirectory = Join-Path $ReleaseRoot $line
$destination = Join-Path $lineDirectory $ProductVersion
if (Test-Path $destination) { throw "Release $ProductVersion already exists. Increase the version." }
$staging = Join-Path $TempRoot ("release-$ProductVersion-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $staging | Out-Null
try {
  Copy-Item $builtInstaller (Join-Path $staging $installerName)
  $sourceName = "MyLIST_$($ProductVersion)_source.zip"
  $sourceZip = Join-Path $staging $sourceName
  Invoke-Checked 'git' @('-c', "safe.directory=$RepoRoot", '-C', $RepoRoot, 'archive', '--format=zip', "--output=$sourceZip", 'HEAD')
  if (!(Test-Path $sourceZip) -or (Get-Item $sourceZip).Length -lt 1KB) { throw 'Source archive verification failed.' }
  $installerHash = (Get-FileHash (Join-Path $staging $installerName) -Algorithm SHA256).Hash
  $sourceHash = (Get-FileHash $sourceZip -Algorithm SHA256).Hash
  [ordered]@{
    product = 'MyLIST'; version = $ProductVersion; builtAt = (Get-Date).ToString('o')
    gitCommit = $gitCommit; packageKind = 'custom-ui-installer'
    installerFile = $installerName; installerBytes = $installer.Length; installerSha256 = $installerHash
    sourceFile = $sourceName; sourceBytes = (Get-Item $sourceZip).Length
  } | ConvertTo-Json | Set-Content (Join-Path $staging 'build-info.json') -Encoding UTF8
  @("$installerName  $installerHash", "$sourceName  $sourceHash") | Set-Content (Join-Path $staging 'SHA256.txt') -Encoding ASCII
  New-Item -ItemType Directory -Force $lineDirectory | Out-Null
  Move-Item $staging $destination
  $tag = "v$ProductVersion"
  if (!(& git -c "safe.directory=$RepoRoot" -C $RepoRoot tag --list $tag)) {
    Invoke-Checked 'git' @('-c', "safe.directory=$RepoRoot", '-C', $RepoRoot, 'tag', '-a', $tag, '-m', "MyLIST $ProductVersion")
  }
  $removed = @()
  Get-ChildItem $lineDirectory -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' -and $_.Name -ne $ProductVersion } |
    ForEach-Object { $removed += $_.Name; Remove-Item $_.FullName -Recurse -Force }
  $index = Update-VersionIndex
  Write-Output 'RESULT: FORMAL INSTALLER WITH CUSTOM UI'
  Write-Output "Version: $ProductVersion"
  Write-Output "Git commit/tag: $gitCommit / $tag"
  Remove-Item -LiteralPath $builtInstaller -Force
  Write-Output "Installer: $(Join-Path $destination $installerName)"
  Write-Output ("Installer size: {0:N2} MB" -f ($installer.Length / 1MB))
  Write-Output "SHA-256: $installerHash"
  Write-Output ("Removed older patches in {0}: {1}" -f $line, $(if ($removed.Count) { $removed -join ', ' } else { 'none' }))
  Write-Output ("All retained releases: {0:N2} MB" -f ($index.totalBytes / 1MB))
} finally {
  if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
}
