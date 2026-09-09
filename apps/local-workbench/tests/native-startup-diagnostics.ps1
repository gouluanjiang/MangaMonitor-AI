# Diagnostic artifacts for a disposable Windows CI runner only. Never run this
# against a user's desktop, and never collect environment or command-line data.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 2147483647)]
    [int] $AppProcessId,

    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory
)

$ErrorActionPreference = 'Stop'
if ($env:CI -cne 'true' -or [Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Startup diagnostics are restricted to disposable Windows CI.'
}

$expectedOutput = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\native-smoke-results')).TrimEnd('\', '/')
$actualOutput = [IO.Path]::GetFullPath($OutputDirectory).TrimEnd('\', '/')
if (-not [string]::Equals($expectedOutput, $actualOutput, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputDirectory must be this workbench checkout native-smoke-results directory.'
}

$executableName = 'mangamonitor-workbench-preview.exe'
$expectedExecutable = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\src-tauri\target\x86_64-pc-windows-msvc\release\mangamonitor-workbench-preview.exe'))
$application = Get-Process -Id $AppProcessId -ErrorAction SilentlyContinue
if ($null -ne $application -and -not [string]::Equals($application.Path, $expectedExecutable, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'AppProcessId must identify the built application in this checkout.'
}

New-Item -ItemType Directory -Path $actualOutput -Force | Out-Null
# Reject an existing redirected output directory before writing any artifacts.
if (((Get-Item -LiteralPath $actualOutput).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'The diagnostic output directory cannot be a reparse point.'
}

$diagnostic = [ordered]@{
    capturedAtUtc = [DateTime]::UtcNow.ToString('o')
    appProcessId = $AppProcessId
    application = $null
    screenshot = $null
    webview2ProcessCount = $null
    webview2Processes = @()
    applicationErrors = @()
    errors = [Collections.Generic.List[object]]::new()
}
$diagnosticPath = Join-Path $actualOutput 'native-process-diagnostics.json'

function Save-Diagnostics {
    $diagnostic | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $diagnosticPath -Encoding utf8
}

function Record-DiagnosticError([string] $Stage, $Failure) {
    # Exception types locate the failed operation without disclosing arbitrary
    # event data, environment values or process command lines in error messages.
    $diagnostic.errors.Add([ordered]@{
        stage = $Stage
        exception = $Failure.Exception.GetType().FullName
    })
}

try {
    if ($null -eq $application) {
        $diagnostic.application = [ordered]@{ exists = $false }
    }
    else {
        $application.Refresh()
        $diagnostic.application = [ordered]@{
            exists = $true
            name = $application.ProcessName
            startedAtUtc = $application.StartTime.ToUniversalTime().ToString('o')
            sessionId = $application.SessionId
            mainWindowTitle = $application.MainWindowTitle
            mainWindowHandle = ('0x{0:X}' -f $application.MainWindowHandle.ToInt64())
            responding = $application.Responding
            handleCount = $application.HandleCount
            workingSetBytes = $application.WorkingSet64
        }
    }
}
catch { Record-DiagnosticError 'application-process' $_ }
Save-Diagnostics

# Capture early, so a later diagnostic timeout still leaves evidence of modal
# startup errors. The guards above restrict this to the owned CI application.
$bitmap = $null
$graphics = $null
try {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
    if ($bounds.Width -le 0 -or $bounds.Height -le 0 -or [long] $bounds.Width * $bounds.Height -gt 40000000) {
        throw 'CI desktop dimensions are unsupported.'
    }
    $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bounds.Size, [Drawing.CopyPixelOperation]::SourceCopy)
    $screenshotName = 'runner-desktop.png'
    $bitmap.Save((Join-Path $actualOutput $screenshotName), [Drawing.Imaging.ImageFormat]::Png)
    $diagnostic.screenshot = $screenshotName
}
catch { Record-DiagnosticError 'ci-desktop-screenshot' $_ }
finally {
    if ($null -ne $graphics) { $graphics.Dispose() }
    if ($null -ne $bitmap) { $bitmap.Dispose() }
}
Save-Diagnostics

try {
    # Toolhelp supplies parent IDs without WMI/CIM or command-line inspection.
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace MangaMonitorCi {
    public sealed class ProcessNode {
        public uint ProcessId { get; set; }
        public uint ParentProcessId { get; set; }
        public string Name { get; set; }
    }

    public static class ProcessSnapshot {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct ProcessEntry {
            public uint Size;
            public uint Usage;
            public uint ProcessId;
            public UIntPtr DefaultHeapId;
            public uint ModuleId;
            public uint Threads;
            public uint ParentProcessId;
            public int BasePriority;
            public uint Flags;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
            public string ExeFile;
        }

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);
        [DllImport("kernel32.dll", EntryPoint = "Process32FirstW", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool First(IntPtr snapshot, ref ProcessEntry entry);
        [DllImport("kernel32.dll", EntryPoint = "Process32NextW", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool Next(IntPtr snapshot, ref ProcessEntry entry);
        [DllImport("kernel32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr handle);

        public static ProcessNode[] Read() {
            IntPtr snapshot = CreateToolhelp32Snapshot(2, 0);
            if (snapshot == new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                var entry = new ProcessEntry { Size = (uint)Marshal.SizeOf(typeof(ProcessEntry)) };
                if (!First(snapshot, ref entry)) throw new Win32Exception(Marshal.GetLastWin32Error());
                var result = new List<ProcessNode>();
                do {
                    result.Add(new ProcessNode {
                        ProcessId = entry.ProcessId,
                        ParentProcessId = entry.ParentProcessId,
                        Name = entry.ExeFile
                    });
                } while (Next(snapshot, ref entry));
                int error = Marshal.GetLastWin32Error();
                if (error != 18) throw new Win32Exception(error);
                return result.ToArray();
            }
            finally { CloseHandle(snapshot); }
        }
    }
}
'@
    $snapshot = [MangaMonitorCi.ProcessSnapshot]::Read()
    $owned = [Collections.Generic.HashSet[uint32]]::new()
    $null = $owned.Add([uint32] $AppProcessId)
    do {
        $changed = $false
        foreach ($entry in $snapshot) {
            if ($owned.Contains($entry.ParentProcessId) -and $owned.Add($entry.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    $diagnostic.webview2Processes = @(
        $snapshot | Where-Object { $owned.Contains($_.ProcessId) -and $_.Name -ieq 'msedgewebview2.exe' } |
            ForEach-Object { [ordered]@{ processId = $_.ProcessId; parentProcessId = $_.ParentProcessId; name = $_.Name } }
    )
    $diagnostic.webview2ProcessCount = $diagnostic.webview2Processes.Count
}
catch { Record-DiagnosticError 'owned-webview-processes' $_ }
Save-Diagnostics

try {
    $eventErrors = @()
    $events = @(Get-WinEvent -FilterHashtable @{
        LogName = 'Application'
        StartTime = (Get-Date).AddMinutes(-5)
        Level = @(1, 2)
    } -MaxEvents 100 -ErrorAction SilentlyContinue -ErrorVariable eventErrors)
    # An empty recent error window is normal. Other failures are diagnostic.
    foreach ($eventError in $eventErrors) {
        if ($eventError.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') {
            Record-DiagnosticError 'application-event-log' $eventError
        }
    }
    $diagnostic.applicationErrors = @(
        foreach ($event in $events) {
            if ($event.Message -notmatch [regex]::Escape($executableName)) { continue }
            $safeLines = @($event.Message -split '\r?\n' | Where-Object {
                $_ -notmatch '(?i)command.?line|environment|password|token|secret|credential'
            })
            $message = $safeLines -join "`n"
            if ($message.Length -gt 6000) { $message = $message.Substring(0, 6000) }
            [ordered]@{
                timeCreatedUtc = $event.TimeCreated.ToUniversalTime().ToString('o')
                eventId = $event.Id
                provider = $event.ProviderName
                message = $message
            }
        }
    )
}
catch { Record-DiagnosticError 'application-event-log' $_ }
Save-Diagnostics
$diagnostic | ConvertTo-Json -Depth 8 -Compress | Write-Output
