if (-not ('WindowsTestJob' -as [type])) {
    Add-Type -Path (Join-Path $PSScriptRoot 'WindowsTestJob.cs')
}

function Invoke-WindowsTestProcess {
    param(
        [Parameter(Mandatory)] [string]$Executable,
        [Parameter(Mandatory)] [string]$CommandLine,
        [Parameter(Mandatory)] [ValidateRange(1, 3600)] [int]$TimeoutSeconds
    )
    try {
        [WindowsTestJob]::Run($Executable, $CommandLine, (Get-Location).Path, $TimeoutSeconds)
    } catch [System.Management.Automation.MethodInvocationException] {
        throw $_.Exception.InnerException
    }
}
