$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSCommandPath
if (-not (Test-Path -LiteralPath (Join-Path $projectDir 'package.json'))) {
    throw "Prototype package.json was not found: $projectDir"
}
$npm = (Get-Command npm.cmd -ErrorAction Stop).Source
$port = 4176
$url = "http://127.0.0.1:$port/"
if (-not (Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue)) {
    Start-Process -FilePath $npm -ArgumentList @('run', 'dev', '--', '--host', '127.0.0.1', '--port', $port) -WorkingDirectory $projectDir -WindowStyle Hidden
    Start-Sleep -Seconds 2
}
Start-Process $url