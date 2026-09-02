$ErrorActionPreference = 'Stop'

$client = Join-Path $PSScriptRoot 'app\windows-client\src-tauri\target\release\windows-client.exe'
if (-not (Test-Path -LiteralPath $client)) {
  Add-Type -AssemblyName PresentationFramework
  [System.Windows.MessageBox]::Show('未找到 MyLIST 客户端，请先完成构建。', 'MyLIST') | Out-Null
  exit 1
}

Start-Process -FilePath $client
