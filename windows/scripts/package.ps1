# Builds a signed MSIX of NACUT (adds the Widgets board provider and a startup task).
#
#   windows\scripts\package.ps1                      # this PC's architecture, Release
#   windows\scripts\package.ps1 -Platform x64
#   windows\scripts\package.ps1 -Install             # also trusts the test certificate and installs (admin)
#
# Signing: uses $env:NACUT_CERT_THUMBPRINT if set (a real code-signing certificate in Cert:\CurrentUser\My).
# Otherwise it creates a self-signed test certificate "CN=Casey Gibson" once and reuses it. A test certificate
# must be trusted (LocalMachine\TrustedPeople) before Windows will install the package; -Install does that.
param(
    [ValidateSet('ARM64', 'x64')][string]$Platform = $(if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'ARM64' } else { 'x64' }),
    [switch]$Install
)
$ErrorActionPreference = 'Continue'
$root = Resolve-Path "$PSScriptRoot\..\.."
$app = Join-Path $root 'windows\NACUT'
$subject = 'CN=Casey Gibson' # must match Publisher in Package.appxmanifest

if (-not (Test-Path (Join-Path $app "native\$Platform\nacut_ffi.dll")) -or -not (Test-Path (Join-Path $app 'Core\nacut.cs'))) {
    & "$PSScriptRoot\build-core.ps1" -All:($Platform -ne $(if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'ARM64' } else { 'x64' }))
}

$thumb = $env:NACUT_CERT_THUMBPRINT
if (-not $thumb) {
    $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $subject -and $_.FriendlyName -eq 'NACUT test signing' } | Select-Object -First 1
    if (-not $cert) {
        $cert = New-SelfSignedCertificate -Type Custom -Subject $subject -FriendlyName 'NACUT test signing' `
            -KeyUsage DigitalSignature -CertStoreLocation Cert:\CurrentUser\My `
            -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
        Write-Host "Created test signing certificate $($cert.Thumbprint)"
    }
    $thumb = $cert.Thumbprint
}

Push-Location $app
try {
    dotnet build -c Release -p:Platform=$Platform -p:Packaged=true -p:NACUT_CERT_THUMBPRINT=$thumb -nologo -v:q
    if ($LASTEXITCODE -ne 0) { throw 'MSIX build failed' }
}
finally { Pop-Location }

$msix = Get-ChildItem (Join-Path $app 'AppPackages') -Recurse -Filter *.msix | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Host "Package: $($msix.FullName)"

if ($Install) {
    if (-not $env:NACUT_CERT_THUMBPRINT) {
        $pub = Join-Path $env:TEMP 'nacut-test.cer'
        Export-Certificate -Cert "Cert:\CurrentUser\My\$thumb" -FilePath $pub | Out-Null
        Import-Certificate -FilePath $pub -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
    }
    Get-Process NACUT -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-AppxPackage CaseyGibson.NACUT | Remove-AppxPackage  # same-version reinstall
    Add-AppxPackage -Path $msix.FullName -ForceApplicationShutdown -ForceUpdateFromAnyVersion
    Write-Host 'Installed. Start NACUT from the Start menu.'
}
