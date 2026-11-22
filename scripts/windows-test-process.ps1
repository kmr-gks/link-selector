function Invoke-WindowsTestProcess {
    param(
        [Parameter(Mandatory)] [string]$Executable,
        [Parameter(Mandatory)] [string[]]$Arguments,
        [Parameter(Mandatory)] [int]$TimeoutSeconds
    )

    $process = Start-Process -FilePath $Executable -ArgumentList $Arguments `
        -WorkingDirectory (Get-Location).Path -NoNewWindow -PassThru
    try {
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            if (-not $process.HasExited) {
                # Cargo starts the test in a descendant process. Stop the whole tree.
                $taskkill = Join-Path $env:SystemRoot 'System32\taskkill.exe'
                & $taskkill /F /T /PID "$($process.Id)" 2>$null | Out-Null
                if (-not $process.WaitForExit(5000)) {
                    throw "Timed out after $TimeoutSeconds seconds and could not stop $Executable"
                }
            }
            throw [System.TimeoutException]::new("$Executable timed out after $TimeoutSeconds seconds")
        }
        return $process.ExitCode
    } finally {
        $process.Dispose()
    }
}
