$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'windows-test-process.ps1')

$msix = Join-Path $env:RUNNER_TEMP 'firefox-protocol-test.msix'
$source = 'https://archive.mozilla.org/pub/firefox/releases/149.0/win64/multi/Firefox%20Setup%20149.0.msix'
$checksum = '26AE4A6F0E963B3DE9F73C8F61E0B4D843A23565C1B3AF65715404EE0721B5D4'

$before = @(Get-AppxPackage '*Firefox*' | Select-Object -ExpandProperty PackageFullName)
$package = $null
try {
    Invoke-WebRequest -UseBasicParsing -Uri $source -OutFile $msix -TimeoutSec 360
    if ((Get-FileHash $msix -Algorithm SHA256).Hash -ne $checksum) {
        throw 'The Firefox MSIX checksum does not match'
    }
    Add-AppxPackage -Path $msix
    $installed = @(Get-AppxPackage '*Firefox*' | Where-Object { $_.PackageFullName -notin $before })
    if ($installed.Count -ne 1) { throw 'Expected one newly registered Firefox package' }
    $package = $installed[0]
    $env:LINK_SELECTOR_TEST_MSIX = $package.PackageFamilyName
    $cargo = (Get-Command cargo.exe -CommandType Application).Source
    $cargoArguments = @('+stable', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked',
        'selected_firefox_msix_receives_the_link', '--', '--ignored')
    $exitCode = Invoke-WindowsTestProcess $cargo $cargoArguments 120
    if ($exitCode -ne 0) { throw "Firefox MSIX link activation failed with exit code $exitCode" }
} finally {
    if ($null -ne $package) {
        Get-Process firefox -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -like "$($package.InstallLocation)\*" } |
            Stop-Process -Force
        Remove-AppxPackage -Package $package.PackageFullName
    }
    if (Test-Path -LiteralPath $msix) {
        Remove-Item -LiteralPath $msix
    }
}
