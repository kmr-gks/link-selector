$ErrorActionPreference = 'Stop'
$msix = Join-Path $env:RUNNER_TEMP 'firefox-protocol-test.msix'
$source = 'https://archive.mozilla.org/pub/firefox/releases/149.0/win64/multi/Firefox%20Setup%20149.0.msix'
$checksum = '26AE4A6F0E963B3DE9F73C8F61E0B4D843A23565C1B3AF65715404EE0721B5D4'

Invoke-WebRequest -UseBasicParsing -Uri $source -OutFile $msix
if ((Get-FileHash $msix -Algorithm SHA256).Hash -ne $checksum) {
    throw 'The Firefox MSIX checksum does not match'
}

$before = @(Get-AppxPackage '*Firefox*' | Select-Object -ExpandProperty PackageFullName)
$package = $null
try {
    Add-AppxPackage -Path $msix
    $installed = @(Get-AppxPackage '*Firefox*' | Where-Object { $_.PackageFullName -notin $before })
    if ($installed.Count -ne 1) { throw 'Expected one newly registered Firefox package' }
    $package = $installed[0]
    $env:LINK_SELECTOR_TEST_MSIX = $package.PackageFamilyName
    cargo +stable test --manifest-path src-tauri/Cargo.toml --locked selected_firefox_msix_receives_the_link -- --ignored
    if ($LASTEXITCODE -ne 0) { throw 'Firefox MSIX link activation failed' }
} finally {
    if ($null -ne $package) {
        Get-Process firefox -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -like "$($package.InstallLocation)\*" } |
            Stop-Process -Force
        Remove-AppxPackage -Package $package.PackageFullName
    }
    Remove-Item -LiteralPath $msix
}
