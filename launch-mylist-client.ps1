$ErrorActionPreference = 'Stop'

$client = Join-Path $PSScriptRoot 'app\windows-client\src-tauri\target\release\windows-client.exe'
if (-not (Test-Path -LiteralPath $client)) {
  Add-Type -AssemblyName PresentationFramework
  [System.Windows.MessageBox]::Show('MyLIST client was not found. Build the application first.', 'MyLIST') | Out-Null
  exit 1
}

Start-Process -FilePath $client
