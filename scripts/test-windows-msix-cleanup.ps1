$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-test-process.ps1')

function Invoke-WebRequest {
    param([switch]$UseBasicParsing, $Uri, $OutFile, $TimeoutSec)
    Set-Content -LiteralPath $OutFile -Value 'mock Firefox MSIX'
}

function Get-FileHash {
    param($Path, $Algorithm)
    [pscustomobject]@{ Hash = $checksum }
}

function Get-AppxPackage {
    param($Name)
    if ($fixtureState.Installed) {
        [pscustomobject]@{
            PackageFullName = 'Firefox_149.0.0.0_x64__test'
            PackageFamilyName = 'Firefox_test'
            InstallLocation = $env:RUNNER_TEMP
        }
    }
}

function Add-AppxPackage {
    param($Path)
    $fixtureState.Installed = $true
}

function Remove-AppxPackage {
    param($Package)
    $fixtureState.Removed = $Package
}

function Get-Process {
    [CmdletBinding()]
    param([Parameter(Position = 0)] [string]$Name)
    $fixtureState.ProcessChecks++
}

$previousTemp = $env:RUNNER_TEMP
$previousMsix = $env:LINK_SELECTOR_TEST_MSIX
$scratch = Join-Path $previousTemp ('msix-cleanup-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $env:RUNNER_TEMP = $scratch
    $smoke = Join-Path $scratch 'test-windows-msix.ps1'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-msix.ps1') -Destination $smoke
    @'
function Invoke-WindowsTestProcess {
    throw $fixtureState.Failure
}
'@ | Set-Content -LiteralPath (Join-Path $scratch 'windows-test-process.ps1')

    $unconfirmed = [WindowsTestJobCleanupException]::new([InvalidOperationException]::new('Injected drain failure'))
    $confirmed = [TimeoutException]::new('Injected timeout after confirmed drain')
    foreach ($testCase in @(
        @{ Failure = $unconfirmed; ExpectedChecks = 0; ExpectedRemoval = $null },
        @{ Failure = $confirmed; ExpectedChecks = 1; ExpectedRemoval = 'Firefox_149.0.0.0_x64__test' }
    )) {
        $fixtureState = @{ Failure = $testCase.Failure; Installed = $false; ProcessChecks = 0; Removed = $null }
        $failure = $null
        try {
            & $smoke
        } catch {
            $failure = $_.Exception
        }
        if ($null -eq $failure -or $failure.GetType() -ne $testCase.Failure.GetType()) {
            throw "Expected the smoke script to propagate [$($testCase.Failure)] but received [$failure]"
        }
        if (-not $fixtureState.Installed -or $fixtureState.ProcessChecks -ne $testCase.ExpectedChecks -or $fixtureState.Removed -ne $testCase.ExpectedRemoval) {
            throw "Unexpected Firefox cleanup after [$failure]"
        }
        if (Test-Path -LiteralPath (Join-Path $scratch 'firefox-protocol-test.msix')) {
            throw "The smoke script left the downloaded MSIX behind after [$failure]"
        }
    }
} finally {
    $env:RUNNER_TEMP = $previousTemp
    $env:LINK_SELECTOR_TEST_MSIX = $previousMsix
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
