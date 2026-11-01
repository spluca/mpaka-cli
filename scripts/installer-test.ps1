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
Assert-Equal 'latest-v2' $script:LatestPointerPath 'bridge release pointer'

$manifestBytes = [Text.Encoding]::UTF8.GetBytes('{"version":"v0.2.0"}')
$primary = [Security.Cryptography.RSA]::Create(2048)
$fallback = [Security.Cryptography.RSA]::Create(2048)
$untrusted = [Security.Cryptography.RSA]::Create(2048)
try {
    $primaryPem = $primary.ExportSubjectPublicKeyInfoPem()
    $fallbackPem = $fallback.ExportSubjectPublicKeyInfoPem()
    $primarySignature = $primary.SignData($manifestBytes, [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    $fallbackSignature = $fallback.SignData($manifestBytes, [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    $untrustedSignature = $untrusted.SignData($manifestBytes, [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    Assert-Equal $true (Test-ManifestSignature -ManifestBytes $manifestBytes -SignatureBytes $primarySignature -PublicKeysPem @($primaryPem, $fallbackPem)) 'primary trusted signature'
    Assert-Equal $true (Test-ManifestSignature -ManifestBytes $manifestBytes -SignatureBytes $fallbackSignature -PublicKeysPem @($primaryPem, $fallbackPem)) 'fallback trusted signature'
    Assert-Equal $false (Test-ManifestSignature -ManifestBytes $manifestBytes -SignatureBytes $untrustedSignature -PublicKeysPem @($primaryPem, $fallbackPem)) 'untrusted signature rejection'
} finally {
    $primary.Dispose()
    $fallback.Dispose()
    $untrusted.Dispose()
}

$retired = Join-Path $PSScriptRoot '..' 'tests' 'fixtures' 'retired-key'
$retiredManifest = [IO.File]::ReadAllBytes((Join-Path $retired 'manifest.json'))
$retiredSignature = [IO.File]::ReadAllBytes((Join-Path $retired 'manifest.json.sig'))
$retiredPem = [IO.File]::ReadAllText((Join-Path $retired 'public.pem'))
Assert-Equal $true (Test-ManifestSignature -ManifestBytes $retiredManifest -SignatureBytes $retiredSignature -PublicKeysPem @($retiredPem)) 'retired-key fixture validity'
Remove-Item Env:MPAKA_CLI_TEST_PUBLIC_KEY_PEM -ErrorAction SilentlyContinue
Assert-Equal $false (Test-ManifestSignature -ManifestBytes $retiredManifest -SignatureBytes $retiredSignature) 'retired-key signature rejection'

if ((Get-Command Get-FileHash -ErrorAction SilentlyContinue) -eq $null) {
    throw 'Get-FileHash is required'
}
if ((Select-String -Path (Join-Path $PSScriptRoot '..' 'install.ps1') -Pattern 'Invoke-WebRequest|Get-FileHash|VerifyData|Move-Item' -AllMatches).Count -lt 4) {
    throw 'installer security operations are missing'
}

Write-Output 'PowerShell installer fixture tests passed'
