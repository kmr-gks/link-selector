param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][ValidateSet('x64', 'arm64')][string]$Architecture
)

$ErrorActionPreference = 'Stop'
$directory = Join-Path $env:RUNNER_TEMP 'link-selector-test'
$app = Join-Path $directory 'link-selector.exe'
$uninstaller = Join-Path $directory 'uninstall.exe'
$registeredApps = 'HKCU:\Software\RegisteredApplications'
$capabilities = 'HKCU:\Software\Link Selector\Capabilities\URLAssociations'
$class = 'HKCU:\Software\Classes\LinkSelector.Url'
$progId = 'LinkSelector.Url'

$machines = @{ x64 = 0x8664; arm64 = 0xAA64 }

function Confirm-InstalledArchitecture {
    $image = [System.IO.BinaryReader]::new([System.IO.File]::OpenRead($app))
    try {
        if ($image.ReadUInt16() -ne 0x5A4D) { throw 'Missing executable DOS header' }
        $image.BaseStream.Position = 0x3C
        $image.BaseStream.Position = $image.ReadUInt32()
        if ($image.ReadUInt32() -ne 0x4550) { throw 'Missing PE header' }
        $machine = $image.ReadUInt16()
        if ($machine -ne $machines[$Architecture]) {
            throw ('Expected a native {0} executable, found PE machine 0x{1:X4}' -f $Architecture, $machine)
        }
    } finally {
        $image.Dispose()
    }
}

try {
    $install = Start-Process $Installer -ArgumentList '/S', "/D=$directory" -Wait -PassThru
    if ($install.ExitCode -ne 0 -or !(Test-Path $app)) { throw 'Silent installation failed' }
    Confirm-InstalledArchitecture
    $registration = Get-ItemPropertyValue $registeredApps 'Link Selector'
    if ($registration -ne 'Software\Link Selector\Capabilities') { throw 'Incorrect app registration' }
    foreach ($scheme in 'http', 'https') {
        if ((Get-ItemPropertyValue $capabilities $scheme) -ne $progId) {
            throw "Missing handler for $scheme"
        }
    }
    $command = (Get-Item "$class\shell\open\command").GetValue('')
    if ($command -ne ('"{0}" "%1"' -f $app)) { throw "Incorrect launch command: $command" }
    node (Join-Path $PSScriptRoot "check-windows-picker.mjs") $app
    if ($LASTEXITCODE -ne 0) { throw "The installed app did not render both picker URLs" }
} finally {
    if (Test-Path $uninstaller) {
        $uninstall = Start-Process $uninstaller -ArgumentList '/S' -Wait -PassThru
        if ($uninstall.ExitCode -ne 0) { throw 'Silent uninstall failed' }
    }
}
if ((Test-Path $class) -or (Test-Path $capabilities) -or
    (Get-ItemProperty $registeredApps -Name 'Link Selector' -ErrorAction SilentlyContinue)) {
    throw 'Default-app registration was left behind after uninstall'
}
