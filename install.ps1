param(
    [string]$Version,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArguments
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ReleaseBaseUrl = if ($env:MPAKA_CLI_BASE_URL) { $env:MPAKA_CLI_BASE_URL.TrimEnd('/') } else { 'https://cli.mpaka.dev' }
$script:EmbeddedPublicKeyPem = @"
-----BEGIN PUBLIC KEY-----
MIIBojANBgkqhkiG9w0BAQEFAAOCAY8AMIIBigKCAYEAj4+ezaZgsT5NlaDScaKn
oxXg+CjSmYQLRxe6zHFZTkAj668hHFlgWX7zrYlxLy9qSb29uj4JyBLyCISZOdye
FumDXOZOiVwVl2HDINiaWAAHzRSC7Jm21MZ5DYR6fV2gm50Rm+r8DO72OhlQqH/T
R5UKnUN8YxnlJZoVb73PqxFnwpYN7E0Cuc8D0VI47Vor06gdvTTaXCY+L7+UlT/H
kcd2UeMkBQcYtusdqvBn1hVeIg3UYQL2QoaN5DNTHtGg5Xoj3FtaJt5QFWqO/hUs
DAb3IvFxwWkik9kbLpMhuN5RQlMHOXmZPDIkl49thFw6FHgCYPUiUYC+XBOkOzTv
KzvrwcFabuosV7Sw/Mg1F8hSsdrfnWSphXPvHukrm7SPAQy1Fm80r+YpplwIWJ7s
59mlh3lfJbzwa6Mej92TmXh35EiV3ImOBdTx8Y7LIdHS2m6YmxxZYiDgDIc//027
/Vrt9RLw5cYgJKvFWhmvPLAX44naMd+fNQENa8e+DP6tAgMBAAE=
-----END PUBLIC KEY-----
"@

function Get-TargetTriple {
    param([string]$Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString())
    if ($Architecture -ne 'X64') {
        throw "unsupported Windows architecture '$Architecture'; only x64 is supported"
    }
    return 'x86_64-pc-windows-msvc'
}

function Test-SafeZipEntries {
    param([string[]]$Entries)
    if ($Entries.Count -ne 1 -or $Entries[0] -ne 'mpaka.exe') {
        return $false
    }
    foreach ($entry in $Entries) {
        if ([System.IO.Path]::IsPathRooted($entry) -or $entry.Contains('..') -or $entry.Contains('\')) {
            return $false
        }
    }
    return $true
}

function Test-Version {
    param([string]$Candidate)
    return $Candidate -match '^v[0-9]+\.[0-9]+\.[0-9]+$'
}

function Get-DownloadBytes {
    param([Uri]$Uri)
    if ($Uri.Scheme -ne 'https') {
        throw "refusing non-HTTPS URL: $Uri"
    }
    $handler = [System.Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $true
    if ($env:MPAKA_CLI_CA_CERT) {
        # Test harness only: production does not set this variable.
        $handler.ServerCertificateCustomValidationCallback = { $true }
    }
    $client = [System.Net.Http.HttpClient]::new($handler)
    try {
        $response = $client.GetAsync($Uri).GetAwaiter().GetResult()
        if ($response.RequestMessage.RequestUri.Scheme -ne 'https') {
            throw 'download redirected away from HTTPS'
        }
        $response.EnsureSuccessStatusCode()
        if ($response.Content.Headers.ContentLength -gt 52428800) {
            throw 'download is larger than the installer limit'
        }
        return $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    } finally {
        $client.Dispose()
        $handler.Dispose()
    }
}

function Get-ReleaseBytes {
    param([string]$Path)
    $uri = [Uri]::new("$script:ReleaseBaseUrl/$Path")
    return Get-DownloadBytes -Uri $uri
}

function Test-ManifestSignature {
    param(
        [byte[]]$ManifestBytes,
        [byte[]]$SignatureBytes,
        [string]$PublicKeyPem = $(if ($env:MPAKA_CLI_TEST_PUBLIC_KEY_PEM) { $env:MPAKA_CLI_TEST_PUBLIC_KEY_PEM } else { $script:EmbeddedPublicKeyPem })
    )
    $pemBody = ($PublicKeyPem -replace '-----BEGIN PUBLIC KEY-----', '' -replace '-----END PUBLIC KEY-----', '' -replace '\s', '')
    $der = [Convert]::FromBase64String($pemBody)
    $bytesRead = 0
    $rsa = [System.Security.Cryptography.RSA]::Create()
    try {
        $rsa.ImportSubjectPublicKeyInfo($der, [ref]$bytesRead)
        return $rsa.VerifyData(
            $ManifestBytes,
            $SignatureBytes,
            [System.Security.Cryptography.HashAlgorithmName]::SHA256,
            [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
        )
    } finally {
        $rsa.Dispose()
    }
}

function Invoke-MpakaInstaller {
    if (-not $Version -and $RemainingArguments -and $RemainingArguments[0] -eq '--version') {
        if ($RemainingArguments.Count -ne 2) { throw 'usage: install.ps1 [-Version vMAJOR.MINOR.PATCH]' }
        $Version = $RemainingArguments[1]
    }
    if ($RemainingArguments -and $RemainingArguments[0] -ne '--version') {
        throw 'usage: install.ps1 [-Version vMAJOR.MINOR.PATCH]'
    }
    if ($script:ReleaseBaseUrl -notmatch '^https://') {
        throw 'release base URL must use HTTPS'
    }
    $target = Get-TargetTriple
    if ($env:MPAKA_CLI_TEST_ONLY_TARGET -eq '1') {
        Write-Output "target=$target"
        return
    }

    $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("mpaka-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $temp | Out-Null
    try {
        if (-not $Version) {
            $Version = [Text.Encoding]::UTF8.GetString((Get-ReleaseBytes -Path 'latest')).Trim()
        }
        if (-not (Test-Version $Version)) { throw "invalid release version '$Version'" }

        $manifestBytes = Get-ReleaseBytes -Path "releases/$Version/manifest.json"
        $signatureBytes = Get-ReleaseBytes -Path "releases/$Version/manifest.json.sig"
        if (-not (Test-ManifestSignature -ManifestBytes $manifestBytes -SignatureBytes $signatureBytes)) {
            throw 'manifest signature verification failed'
        }
        $manifest = [Text.Encoding]::UTF8.GetString($manifestBytes) | ConvertFrom-Json
        if ($manifest.version -ne $Version) { throw 'manifest version does not match requested version' }
        $artifact = @($manifest.artifacts | Where-Object { $_.target -eq $target })
        if ($artifact.Count -ne 1 -or $artifact[0].filename -ne 'mpaka-x86_64-pc-windows-msvc.zip') {
            throw "manifest has no valid Windows x64 artifact"
        }
        $artifact = $artifact[0]
        $archivePath = Join-Path $temp $artifact.filename
        [IO.File]::WriteAllBytes($archivePath, (Get-ReleaseBytes -Path "releases/$Version/$($artifact.filename)"))
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
        if ($hash -ne $artifact.sha256.ToLowerInvariant()) { throw 'downloaded artifact SHA-256 does not match manifest' }
        if ((Get-Item -LiteralPath $archivePath).Length -ne [int64]$artifact.size) { throw 'downloaded artifact size does not match manifest' }

        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
        try {
            $entries = @($zip.Entries | ForEach-Object { $_.FullName })
            if (-not (Test-SafeZipEntries $entries)) { throw 'archive contains unsafe or unexpected entries' }
        } finally { $zip.Dispose() }
        $extract = Join-Path $temp 'extract'
        New-Item -ItemType Directory -Path $extract | Out-Null
        [System.IO.Compression.ZipFile]::ExtractToDirectory($archivePath, $extract)
        $source = Join-Path $extract 'mpaka.exe'
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw 'archive did not contain mpaka.exe' }

        $localAppData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { throw 'LOCALAPPDATA is not set' }
        $installDir = Join-Path $localAppData 'mPaka\bin'
        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        $destination = Join-Path $installDir 'mpaka.exe'
        if (Test-Path -LiteralPath $destination) {
            $existing = Get-Item -LiteralPath $destination -Force
            if ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'refusing to install through a reparse point' }
        }
        $temporary = Join-Path $installDir ('.mpaka-' + [Guid]::NewGuid().ToString('N') + '.exe')
        Copy-Item -LiteralPath $source -Destination $temporary
        Move-Item -LiteralPath $temporary -Destination $destination -Force
        Write-Output "Installed mpaka $Version for $target at $destination"
        $pathParts = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ })
        if ($pathParts -notcontains $installDir) {
            Write-Output "Add $installDir to the user PATH to run mpaka directly:"
            Write-Output "[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$installDir', 'User')"
        }
    } finally {
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    try { Invoke-MpakaInstaller } catch { Write-Error "mpaka installer: $($_.Exception.Message)"; exit 1 }
}
