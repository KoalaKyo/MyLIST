param([string]$ProjectRoot="",[string]$OutputPath="")
$ErrorActionPreference='Stop'
if(!$ProjectRoot){$ProjectRoot=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path}
$shell=Join-Path $ProjectRoot 'installer-shell'
$tauriConfig=Get-Content -LiteralPath (Join-Path $ProjectRoot 'src-tauri\tauri.conf.json') -Raw|ConvertFrom-Json
$version=[string]$tauriConfig.version
$stable=Join-Path $ProjectRoot "src-tauri\target\release\bundle\nsis\MyLIST_${version}_x64-setup.exe"
if(!$OutputPath){$OutputPath=$stable}
$dummy=Join-Path $shell 'payload\empty.bin'
$unTarget=Join-Path $shell 'target-uninstaller'
$installerTarget=Join-Path $shell 'target'
$shellIcon=Join-Path $shell 'src-tauri\icons\icon.ico'
$uninstallIcon=Join-Path $ProjectRoot 'src-tauri\icons\uninstall.ico'
$iconBackup=Join-Path ([IO.Path]::GetTempPath()) 'mylist-installer-brand-icon.ico'
Push-Location $ProjectRoot
try {
  Copy-Item -LiteralPath $shellIcon -Destination $iconBackup -Force
  New-Item -ItemType Directory -Force (Split-Path $dummy)|Out-Null
  [IO.File]::WriteAllBytes($dummy,[byte[]]@(0))
  &(Join-Path $ProjectRoot 'node_modules\.bin\vite.cmd') build --config(Join-Path $shell 'vite.config.ts')
  if($LASTEXITCODE){throw 'UI build failed'}
  $env:MYLIST_INSTALLER_PAYLOAD=$dummy
  $env:MYLIST_SHELL_MODE='uninstaller'
  $env:CARGO_TARGET_DIR=$unTarget
  Copy-Item -LiteralPath $uninstallIcon -Destination $shellIcon -Force
  cargo build --release --manifest-path(Join-Path $shell 'src-tauri\Cargo.toml')
  if($LASTEXITCODE){throw 'uninstaller shell build failed'}
  Copy-Item -LiteralPath $iconBackup -Destination $shellIcon -Force
  $env:MYLIST_WEB_UNINSTALLER=Join-Path $unTarget 'release\mylist-installer.exe'
  Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
  npm run build:nsis-core
  if($LASTEXITCODE){throw 'NSIS build failed'}
  $payload=Join-Path $shell 'payload\MyLIST-payload.exe'
  Copy-Item $stable $payload -Force
  $env:MYLIST_INSTALLER_PAYLOAD=$payload
  $env:MYLIST_SHELL_MODE='installer'
  $env:CARGO_TARGET_DIR=$installerTarget
  cargo build --release --manifest-path(Join-Path $shell 'src-tauri\Cargo.toml')
  if($LASTEXITCODE){throw 'installer shell build failed'}
  Copy-Item(Join-Path $installerTarget 'release\mylist-installer.exe')$OutputPath -Force
  "Web installer written: $OutputPath"
} finally {
  if(Test-Path -LiteralPath $iconBackup){Copy-Item -LiteralPath $iconBackup -Destination $shellIcon -Force;Remove-Item -LiteralPath $iconBackup -Force}
  Remove-Item Env:MYLIST_INSTALLER_PAYLOAD,Env:MYLIST_SHELL_MODE,Env:MYLIST_WEB_UNINSTALLER,Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
  Remove-Item (Join-Path $shell 'payload') -Recurse -Force -ErrorAction SilentlyContinue
  Pop-Location
}
