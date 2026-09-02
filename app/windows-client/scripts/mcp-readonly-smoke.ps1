param(
    [string]$PipeName
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($PipeName)) {
    # Named pipes cannot be enumerated reliably by ordinary PowerShell
    # sessions. MyLIST therefore exposes one stable, current-user-only pipe.
    $PipeName = 'MyLIST-MCP'
}
$client = [System.IO.Pipes.NamedPipeClientStream]::new('.', $PipeName, [System.IO.Pipes.PipeDirection]::InOut, [System.IO.Pipes.PipeOptions]::None)
try {
    $client.Connect(2000)
    $utf8 = [System.Text.UTF8Encoding]::new($false)
    $reader = [System.IO.StreamReader]::new($client, $utf8, $false, 65536, $true)
    $writer = [System.IO.StreamWriter]::new($client, $utf8, 65536, $true)
    $writer.AutoFlush = $true

    function Invoke-Mcp([int]$Id, [string]$Method, $Params) {
        $request = @{ jsonrpc = '2.0'; id = $Id; method = $Method; params = $Params } | ConvertTo-Json -Compress -Depth 20
        $writer.WriteLine($request)
        $line = $reader.ReadLine()
        if ([string]::IsNullOrWhiteSpace($line)) { throw "MCP returned no response for $Method" }
        $response = $line | ConvertFrom-Json
        [pscustomobject]@{ method = $Method; response = $response }
    }

    $results = @()
    $results += Invoke-Mcp 1 'initialize' @{}
    $writer.WriteLine((@{ jsonrpc = '2.0'; method = 'notifications/initialized'; params = @{} } | ConvertTo-Json -Compress -Depth 20))
    # Keep the notification on its own pipe message before the next request.
    Start-Sleep -Milliseconds 25
    $results += Invoke-Mcp 2 'tools/list' @{}
    $results += Invoke-Mcp 3 'tools/call' @{ name = 'mylist_get_overview'; arguments = @{} }
    $results += Invoke-Mcp 4 'tools/call' @{ name = 'mylist_list_tasks'; arguments = @{ status = 'todo'; page = 0; pageSize = 5 } }
    $results += Invoke-Mcp 5 'tools/call' @{ name = 'mylist_list_categories'; arguments = @{ page = 0; pageSize = 20 } }
    $results += Invoke-Mcp 6 'tools/call' @{ name = 'mylist_get_palette'; arguments = @{ page = 0; pageSize = 24 } }

    Write-Output "MCP pipe: $PipeName"
    foreach ($item in $results) {
        $result = $item.response.result
        if ($null -eq $result) {
            throw "MCP returned an error for $($item.method): $($item.response.error.message)"
        }
        $toolCount = if ($item.method -eq 'tools/list') { @($result.tools | ForEach-Object { $_ }).Count } else { $null }
        if ($null -ne $toolCount) {
            Write-Output "$($item.method): tools=$toolCount"
        } elseif ($item.method -eq 'initialize') {
            Write-Output "$($item.method): protocol=$($result.protocolVersion)"
        } else {
            Write-Output "$($item.method): ok=$($null -ne $result)"
        }
    }
}
finally {
    if ($null -ne $client) { $client.Dispose() }
}
