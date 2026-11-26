param(
    [Parameter(Mandatory)][int]$AppProcessId,
    [Parameter(Mandatory)][int]$ExpectedCount
)

$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class PickerWindows
{
    private delegate bool WindowCallback(IntPtr window, IntPtr context);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(WindowCallback callback, IntPtr context);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")]
    private static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr window, StringBuilder text, int length);

    public static int CountVisible(uint appProcessId)
    {
        int count = 0;
        EnumWindows((window, context) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == appProcessId && IsWindowVisible(window) && !IsIconic(window))
            {
                var title = new StringBuilder(64);
                GetWindowText(window, title, title.Capacity);
                if (title.ToString() == "Link Selector")
                    count++;
            }
            return true;
        }, IntPtr.Zero);
        return count;
    }
}
'@

# WebView targets can finish rendering while their native windows are still hidden.
$timer = [System.Diagnostics.Stopwatch]::StartNew()
do {
    $count = [PickerWindows]::CountVisible($AppProcessId)
    if ($count -eq $ExpectedCount) { exit 0 }
    Start-Sleep -Milliseconds 250
} while ($timer.Elapsed.TotalSeconds -lt 10)
throw "Expected $ExpectedCount visible Link Selector windows for PID $AppProcessId, found $count"
