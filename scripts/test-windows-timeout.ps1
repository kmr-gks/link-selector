$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-test-process.ps1')

$cmd = Join-Path $env:SystemRoot 'System32\cmd.exe'
$exitCode = Invoke-WindowsTestProcess $cmd '/d /c exit /b 7' 30
if ($exitCode -ne 7) {
    throw "Expected exit code 7, received [$exitCode]"
}

$scratch = Join-Path $env:RUNNER_TEMP ('process-job-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $fixture = Join-Path $scratch 'parent.ps1'
    $marker = Join-Path $scratch 'child-pid.txt'
    @'
$child = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\ping.exe') `
    -ArgumentList '-n 180 127.0.0.1' -PassThru
Set-Content -LiteralPath (Join-Path $PSScriptRoot 'child-pid.txt') -Value $child.Id
$child.WaitForExit()
'@ | Set-Content -LiteralPath $fixture
    $powershell = (Get-Process -Id $PID).Path
    try {
        Invoke-WindowsTestProcess $powershell "-NoLogo -NoProfile -NonInteractive -File `"$fixture`"" 60 | Out-Null
        throw 'Expected a timeout'
    } catch [System.TimeoutException] {
        if (-not (Test-Path -LiteralPath $marker)) {
            throw 'The timeout fixture did not start its child before the deadline'
        }
        $childId = [int](Get-Content -LiteralPath $marker)
        if (Get-Process -Id $childId -ErrorAction SilentlyContinue) {
            throw "The child process survived the test job timeout: $childId"
        }
        Write-Host 'Timed-out test and its child both stopped.'
    }
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
