# Builds the Rust core for Windows and generates its C# bindings for the WinUI app.
#
#   windows\scripts\build-core.ps1              # this PC's architecture
#   windows\scripts\build-core.ps1 -All         # ARM64 and x64
#
# Output (gitignored): windows\NACUT\native\<ARM64|x64>\nacut_ffi.dll and windows\NACUT\Core\nacut.cs.
# Needs Rust (rustup), Visual Studio Build Tools (C++), and on ARM64 LLVM's clang on PATH (for `ring`).
# Installs uniffi-bindgen-cs on first run; its version must match the core's UniFFI (0.28.3).
param([switch]$All)
# Native tools write progress to stderr, which Windows PowerShell 5.1 treats as errors with 'Stop';
# failures are caught through $LASTEXITCODE instead.
$ErrorActionPreference = 'Continue'

$root = Resolve-Path "$PSScriptRoot\..\.."
$core = Join-Path $root 'core'
$app = Join-Path $root 'windows\NACUT'
$bindgenTag = 'v0.9.2+v0.28.3'

$hostArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'ARM64' } else { 'x64' }
$targets = @{ 'ARM64' = 'aarch64-pc-windows-msvc'; 'x64' = 'x86_64-pc-windows-msvc' }
$archs = if ($All) { @('ARM64', 'x64') } else { @($hostArch) }

if (-not (Get-Command uniffi-bindgen-cs -ErrorAction SilentlyContinue)) {
    cargo install --locked uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag $bindgenTag
}

Push-Location $core
try {
    foreach ($arch in $archs) {
        $triple = $targets[$arch]
        rustup target add $triple 2>&1 | Out-Null
        cargo build --release --locked -p nacut-ffi --target $triple
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $triple" }
        $out = Join-Path $app "native\$arch"
        New-Item -ItemType Directory -Force $out | Out-Null
        Copy-Item "target\$triple\release\nacut_ffi.dll" $out -Force
    }
    # Bindings are identical for every architecture; generate from the host build.
    $dll = "target\$($targets[$archs[0]])\release\nacut_ffi.dll"
    New-Item -ItemType Directory -Force (Join-Path $app 'Core') | Out-Null
    uniffi-bindgen-cs --library $dll --out-dir (Join-Path $app 'Core')
    if ($LASTEXITCODE -ne 0) { throw 'uniffi-bindgen-cs failed' }
}
finally {
    Pop-Location
}
Write-Host "Core built for $($archs -join ', '); bindings in windows\NACUT\Core\nacut.cs"
