Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '..' 'install.ps1')

function Assert-Equal([object]$Expected, [object]$Actual, [string]$Message) {
    if ($Expected -ne $Actual) {
        throw "$Message. Expected '$Expected', got '$Actual'."
    }
}

Assert-Equal 'x86_64-pc-windows-msvc' (Get-TargetTriple -Architecture 'X64') 'x64 target mapping'
try {
    Get-TargetTriple -Architecture 'Arm64' | Out-Null
    throw 'non-x64 architecture was accepted'
} catch {
    if ($_.Exception.Message -notmatch 'x64') { throw }
}

Assert-Equal $true (Test-SafeZipEntries @('mpaka.exe')) 'safe ZIP entry'
Assert-Equal $false (Test-SafeZipEntries @('mpaka.exe', 'extra.exe')) 'extra ZIP entry rejection'
Assert-Equal $false (Test-SafeZipEntries @('../mpaka.exe')) 'parent ZIP entry rejection'
Assert-Equal $false (Test-SafeZipEntries @('/mpaka.exe')) 'absolute ZIP entry rejection'

if ((Get-Command Get-FileHash -ErrorAction SilentlyContinue) -eq $null) {
    throw 'Get-FileHash is required'
}
if ((Select-String -Path (Join-Path $PSScriptRoot '..' 'install.ps1') -Pattern 'Invoke-WebRequest|Get-FileHash|VerifyData|Move-Item' -AllMatches).Count -lt 4) {
    throw 'installer security operations are missing'
}

Write-Output 'PowerShell installer fixture tests passed'
