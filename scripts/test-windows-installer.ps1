param([Parameter(Mandatory)][string]$Installer)

$ErrorActionPreference = 'Stop'
$directory = Join-Path $env:RUNNER_TEMP 'link-selector-test'
$app = Join-Path $directory 'link-selector.exe'
$uninstaller = Join-Path $directory 'uninstall.exe'
$registeredApps = 'HKCU:\Software\RegisteredApplications'
$capabilities = 'HKCU:\Software\Link Selector\Capabilities\URLAssociations'
$class = 'HKCU:\Software\Classes\LinkSelector.Url'
$progId = 'LinkSelector.Url'

try {
    $install = Start-Process $Installer -ArgumentList '/S', "/D=$directory" -Wait -PassThru
    if ($install.ExitCode -ne 0 -or !(Test-Path $app)) { throw 'Silent installation failed' }
    $registration = Get-ItemPropertyValue $registeredApps 'Link Selector'
    if ($registration -ne 'Software\Link Selector\Capabilities') { throw 'Incorrect app registration' }
    foreach ($scheme in 'http', 'https') {
        if ((Get-ItemPropertyValue $capabilities $scheme) -ne $progId) {
            throw "Missing handler for $scheme"
        }
    }
    $command = (Get-Item "$class\shell\open\command").GetValue('')
    if ($command -ne ('"{0}" "%1"' -f $app)) { throw "Incorrect launch command: $command" }
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
