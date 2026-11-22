$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-test-process.ps1')

$cmd = Join-Path $env:SystemRoot 'System32\cmd.exe'
if ((Invoke-WindowsTestProcess $cmd @('/d', '/c', 'exit', '7') 30) -ne 7) {
    throw 'Expected the process exit code to be preserved'
}

try {
    Invoke-WindowsTestProcess $cmd @('/d', '/c', 'ping.exe', '-n', '30', '127.0.0.1') 1 | Out-Null
    throw 'Expected a timeout while waiting for the child process'
} catch [System.TimeoutException] {
    Write-Host "Process timeout verified: $($_.Exception.Message)"
}
