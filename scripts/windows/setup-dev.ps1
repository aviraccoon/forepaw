# setup-dev.ps1 -- Install dev tools on the Windows VM for forepaw testing.
#
# Usage (from macOS via winrun):
#   winrun scripts/windows/setup-dev.ps1
#
# Installs:
#   - Visual Studio Build Tools 2022 (C++ workload, for Rust MSVC toolchain)
#   - Rust via rustup
#   - JDK 21 (Temurin, for Java/Swing testing)
#   - VS Code (Electron UIA test target)
#
# This is a DEV ENVIRONMENT script, not something end users need.
# The final forepaw binary requires no runtime dependencies beyond what
# Windows ships (UIA, SendInput, Windows.Media.Ocr, VC++ redistributable).
#
# Note: VS Build Tools install takes ~10-15 minutes. Rust and JDK installs
# are faster (~2 min each). The script retries winget installs with backoff.

$ErrorActionPreference = "Continue"

function Install-App($Id, $Name) {
    Write-Host "Installing $Name ($Id)..."
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        $result = winget install --id $Id --source winget --accept-package-agreements --accept-source-agreements --silent 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  OK: $Name installed"
            return $true
        }
        Write-Host "  Attempt $attempt failed (exit $LASTEXITCODE): $result"
        if ($attempt -lt 3) { Start-Sleep -Seconds 10 }
    }
    Write-Host "  WARNING: Failed to install $Name after 3 attempts"
    return $false
}

Write-Host "=== forepaw dev environment setup ==="
Write-Host ""

# 1. VS Build Tools 2022 with C++ workload
Write-Host "--- Visual Studio Build Tools 2022 ---"
# Install the Build Tools with the C++ build tools workload
# This provides MSVC linker needed for Rust on Windows
Install-App "Microsoft.VisualStudio.2022.BuildTools" "VS Build Tools"
# Add C++ workload via winget modify (the base install may not include it)
Write-Host "Adding C++ workload..."
$vsInstaller = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vs_installer.exe"
if (Test-Path $vsInstaller) {
    Start-Process -FilePath $vsInstaller -ArgumentList "modify --installPath `"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`" --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --passive --wait" -Wait
    Write-Host "  C++ workload added"
} else {
    Write-Host "  WARNING: VS Installer not found"
}

# 2. Rust
Write-Host ""
Write-Host "--- Rust ---"
Install-App "Rustlang.Rustup" "Rustup"
# Set stable default and add MSVC target
Write-Host "Configuring Rust..."
& "$env:USERPROFILE\.cargo\bin\rustup.exe" default stable 2>&1 | Write-Host
& "$env:USERPROFILE\.cargo\bin\rustup.exe" target add aarch64-pc-windows-msvc 2>&1 | Write-Host
& "$env:USERPROFILE\.cargo\bin\rustc.exe" --version 2>&1 | Write-Host

# 3. JDK 21 (Temurin)
Write-Host ""
Write-Host "--- JDK 21 (Temurin) ---"
Install-App "EclipseAdoptium.Temurin.21.JDK" "Temurin JDK 21"

# 4. VS Code (Electron UIA test target)
Write-Host ""
Write-Host "--- VS Code ---"
Install-App "Microsoft.VisualStudioCode" "VS Code"

Write-Host ""
Write-Host "=== Setup complete ==="
Write-Host ""
Write-Host "Verify installations:"
Write-Host "  rustc --version"
Write-Host "  java -version"
Write-Host "  code --version"
