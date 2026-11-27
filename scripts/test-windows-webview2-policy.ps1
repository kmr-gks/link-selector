$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-webview2-policy.ps1')

$path = "Software\LinkSelectorTest-$([guid]::NewGuid())"
$registry = [Microsoft.Win32.Registry]::CurrentUser
$key = $registry.CreateSubKey($path)
try {
    $name = 'link-selector.exe'
    $original = '--user-data-dir=%TEMP%\LinkSelector'
    $temporary = '--remote-debugging-port=1234'
    $expandString = [Microsoft.Win32.RegistryValueKind]::ExpandString
    $key.SetValue($name, $original, $expandString)
    try {
        Invoke-WithWebView2Arguments $key $name $temporary {
            if ($key.GetValue($name) -cne $temporary) { throw 'Temporary arguments are missing' }
            throw 'Simulated test failure'
        }
        throw 'Expected the simulated test failure'
    } catch {
        if ($_.Exception.Message -cne 'Simulated test failure') { throw }
    }
    $restored = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    if ($restored -cne $original -or $key.GetValueKind($name) -ne $expandString) {
        throw 'Expandable arguments were not restored verbatim'
    }

    $key.DeleteValue($name)
    Invoke-WithWebView2Arguments $key $name $temporary {
        if ($key.GetValue($name) -cne $temporary) { throw 'Temporary arguments are missing' }
    }
    if ($null -ne $key.GetValue($name)) { throw 'Temporary arguments were left behind' }
} finally {
    $key.Dispose()
    $registry.DeleteSubKeyTree($path, $false)
}
