# Build the Windows MorseLink MSI + NSIS EXE installer.
# Prerequisites on Windows:
#   * Rust stable toolchain + MSVC
#   * Node.js 18+
#   * WebView2 (usually preinstalled on Windows 10/11)
#   * WiX Toolset and NSIS (Tauri pulls these in automatically)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Set-Location "$PSScriptRoot\..\pc-app"

Write-Host ">> Installing frontend dependencies"
npm install

Write-Host ">> Building Tauri app (MSI + NSIS EXE)"
npm run tauri build

Write-Host ""
Write-Host ">> MSI: src-tauri\target\release\bundle\msi\*.msi"
Write-Host ">> EXE: src-tauri\target\release\bundle\nsis\*.exe"
