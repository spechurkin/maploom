param(
    [Parameter(Mandatory = $true)][string]$PackageDirectory,
    [switch]$NsisOnly
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT')
{
    throw 'Windows is required'
}
if (!$NsisOnly -and $env:GITHUB_ACTIONS -ne 'true')
{
    throw 'Full MSI testing requires an isolated CI runner. Use -NsisOnly locally.'
}
$taskRepo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskPackages = [IO.Path]::GetFullPath($PackageDirectory)
# Never replace the user's existing installation or its registration/shortcuts.
foreach ($taskRegistryRoot in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'))
{
    if (Get-ChildItem -LiteralPath $taskRegistryRoot -ErrorAction SilentlyContinue | Get-ItemProperty | Where-Object DisplayName -EQ 'Maploom')
    {
        throw 'Maploom is already registered. Run this test on an isolated Windows runner.'
    }
}
$taskStage = Join-Path $taskRepo ('artifacts\theme-package-check\windows-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskStage -Force | Out-Null
$taskMetadataPath = 'Software\maploom\Maploom'
$taskMetadataBefore = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($taskMetadataPath)
$taskMetadataExisted = $null -ne $taskMetadataBefore
$taskMetadataValues = @{ }
foreach ($taskName in @('', 'Installer Language'))
{
    if ($taskMetadataBefore -and $taskName -in $taskMetadataBefore.GetValueNames())
    {
        $taskMetadataValues[$taskName] = @($taskMetadataBefore.GetValue($taskName),$taskMetadataBefore.GetValueKind($taskName))
    }
}
if ($taskMetadataBefore)
{
    $taskMetadataBefore.Close()
}
function Restore-TaskMetadata
{
    $taskKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($taskMetadataPath, $true)
    if (!$taskKey)
    {
        return
    }
    foreach ($taskName in @('', 'Installer Language'))
    {
        if ( $taskMetadataValues.ContainsKey($taskName))
        {
            $taskKey.SetValue($taskName, $taskMetadataValues[$taskName][0], $taskMetadataValues[$taskName][1])
        }
        else
        {
            $taskKey.DeleteValue($taskName, $false)
        }
    }
    $taskEmpty = $taskKey.ValueCount -eq 0 -and $taskKey.SubKeyCount -eq 0
    $taskKey.Close()
    if (!$taskMetadataExisted -and $taskEmpty)
    {
        [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey($taskMetadataPath, $false)
    }
}
function Invoke-TaskInstaller([string]$File, [string]$Arguments)
{
    $taskProcess = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -PassThru -Wait
    if ($taskProcess.ExitCode -notin @(0, 3010))
    {
        throw "Installer failed: $( $taskProcess.ExitCode ), $File"
    }
}
function New-TaskMarker([string]$InstallPath)
{
    $taskTheme = Join-Path $InstallPath 'themes\Моя тема [夜] 100%'
    New-Item -ItemType Directory -Path $taskTheme -Force | Out-Null
    $taskHashes = @{ }
    foreach ($taskName in @('theme.json', 'style.css', 'entry.tsx', 'font.woff2'))
    {
        $taskFile = Join-Path $taskTheme $taskName
        [IO.File]::WriteAllBytes($taskFile,[Text.Encoding]::UTF8.GetBytes("User-owned bytes: $taskName"))
        $taskHashes[$taskFile] = (Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash
    }
    return $taskHashes
}
function Assert-TaskMarkers($Hashes)
{
    foreach ($taskFile in $Hashes.Keys)
    {
        if (!(Test-Path -LiteralPath $taskFile) -or (Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash -ne $Hashes[$taskFile])
        {
            throw "Theme lost or changed: $taskFile"
        }
    }
}
$taskNsis = Get-ChildItem -LiteralPath $taskPackages -Filter '*.exe' -Recurse | Select-Object -First 1
if (!$taskNsis)
{
    throw 'Missing NSIS package'
}
$taskInstall = Join-Path $taskStage 'nsis-app'
# /NS suppresses shortcuts; /S is silent. /D must be the final argument.
Invoke-TaskInstaller $taskNsis.FullName "/S /NS /D=$taskInstall"
$taskMarkers = New-TaskMarker $taskInstall
try
{
    Invoke-TaskInstaller $taskNsis.FullName "/S /NS /D=$taskInstall"
    Assert-TaskMarkers $taskMarkers
    Invoke-TaskInstaller $taskNsis.FullName "/S /NS /UPDATE /D=$taskInstall"
    Assert-TaskMarkers $taskMarkers
}
finally
{
    $taskUninstaller = Join-Path $taskInstall 'uninstall.exe'
    try
    {
        if (Test-Path -LiteralPath $taskUninstaller)
        {
            Invoke-TaskInstaller $taskUninstaller "/S _?=$taskInstall"
        }
    }
    finally
    {
        Restore-TaskMetadata
    }
}
Assert-TaskMarkers $taskMarkers
if (Test-Path -LiteralPath (Join-Path $taskInstall 'maploom.exe'))
{
    throw 'NSIS did not remove the application binary'
}
Write-Output 'NSIS: install, reinstall, updater flags, uninstall; theme SHA-256 hashes preserved'
if ($NsisOnly)
{
    exit 0
}

$taskMsi = Get-ChildItem -LiteralPath $taskPackages -Filter '*.msi' -Recurse | Select-Object -First 1
if (!$taskMsi)
{
    throw 'Missing MSI package'
}
$taskInstall = Join-Path $taskStage 'msi-app'
$taskLog = Join-Path $taskStage 'msi.log'
$taskMsiArgs = '/i "' + $taskMsi.FullName + '" /qn /norestart INSTALLDIR="' + $taskInstall + '" ADDLOCAL=ALL /l*v "' + $taskLog + '"'
Invoke-TaskInstaller 'msiexec.exe' $taskMsiArgs
$taskMarkers = New-TaskMarker $taskInstall
try
{
    Invoke-TaskInstaller 'msiexec.exe' ($taskMsiArgs + ' REINSTALL=ALL REINSTALLMODE=vomus')
    Assert-TaskMarkers $taskMarkers
}
finally
{
    Invoke-TaskInstaller 'msiexec.exe' ('/x "' + $taskMsi.FullName + '" /qn /norestart /l*v "' + (Join-Path $taskStage 'msi-remove.log') + '"')
}
Assert-TaskMarkers $taskMarkers
if (Test-Path -LiteralPath (Join-Path $taskInstall 'maploom.exe'))
{
    throw 'MSI did not remove the application binary'
}
Write-Output 'MSI: install, repair/reinstall, uninstall; theme SHA-256 hashes preserved'
