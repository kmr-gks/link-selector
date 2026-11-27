function Invoke-WithWebView2Arguments(
    [Microsoft.Win32.RegistryKey]$Key,
    [string]$Name,
    [string]$BrowserArguments,
    [scriptblock]$Check
) {
    $previous = $Key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    if ($null -ne $previous) { $previousKind = $Key.GetValueKind($Name) }
    $Key.SetValue($Name, $BrowserArguments, [Microsoft.Win32.RegistryValueKind]::String)
    try {
        & $Check
    } finally {
        if ($null -eq $previous) {
            $Key.DeleteValue($Name, $false)
        } else {
            $Key.SetValue($Name, $previous, $previousKind)
        }
    }
}
