$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'windows-test-process.ps1')

$msix = Join-Path $env:RUNNER_TEMP 'firefox-protocol-test.msix'
$source = 'https://archive.mozilla.org/pub/firefox/releases/149.0/win64/multi/Firefox%20Setup%20149.0.msix'
$checksum = '26AE4A6F0E963B3DE9F73C8F61E0B4D843A23565C1B3AF65715404EE0721B5D4'

$package = $null
$canRemovePackage = $true
try {
    Invoke-WebRequest -UseBasicParsing -Uri $source -OutFile $msix -TimeoutSec 360
    if ((Get-FileHash $msix -Algorithm SHA256).Hash -ne $checksum) {
        throw 'The Firefox MSIX checksum does not match'
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($msix)
    try {
        $manifest = [xml]::new()
        $stream = $archive.GetEntry('AppxManifest.xml').Open()
        try { $manifest.Load($stream) } finally { $stream.Dispose() }
    } finally {
        $archive.Dispose()
    }
    $identity = $manifest.Package.Identity
    if ($null -eq $identity) { throw 'The Firefox MSIX manifest is missing its package identity' }
    $name = $identity.GetAttribute('Name')
    $publisher = $identity.GetAttribute('Publisher')
    if (-not $name -or -not $publisher) {
        throw 'The Firefox MSIX manifest is missing its package identity'
    }
    $existing = @(Get-AppxPackage -Name $name | Where-Object { $_.Publisher -eq $publisher })
    if ($existing.Count -gt 0) {
        throw 'The Firefox MSIX test requires an environment without this Firefox package family installed.'
    }

    Add-AppxPackage -Path $msix
    $installed = @(Get-AppxPackage -Name $name | Where-Object { $_.Publisher -eq $publisher })
    if ($installed.Count -ne 1) { throw 'Expected one newly registered Firefox package' }
    $package = $installed[0]
    $env:LINK_SELECTOR_TEST_MSIX = $package.PackageFamilyName
    $cargo = (Get-Command cargo.exe -CommandType Application).Source
    $cargoArguments = '+stable test --manifest-path src-tauri/Cargo.toml --locked selected_firefox_msix_receives_the_link -- --ignored'
    $exitCode = Invoke-WindowsTestProcess $cargo $cargoArguments 120
    if ($exitCode -ne 0) { throw "Firefox MSIX link activation failed with exit code $exitCode" }
} catch [WindowsTestJobCleanupException] {
    $canRemovePackage = $false
    Write-Warning 'Leaving the Firefox MSIX installed because the test process tree has not been confirmed stopped.'
    throw
} finally {
    try {
        if ($canRemovePackage -and $null -ne $package) {
            Get-Process firefox -ErrorAction SilentlyContinue |
                Where-Object { $_.Path -like "$($package.InstallLocation)\*" } |
                Stop-Process -Force
            Remove-AppxPackage -Package $package.PackageFullName
        }
    } finally {
        if (Test-Path -LiteralPath $msix) {
            Remove-Item -LiteralPath $msix
        }
    }
}
