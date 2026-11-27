<#
.SYNOPSIS
    Builds a self-contained Windows portable bundle, Explorer context-menu
    scripts, ZIP archive, and NSIS installer for ImageOptim-Rust.

.EXAMPLE
    pwsh ./scripts/package-windows.ps1 release
#>
param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release",
    [string]$Target = "",
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($Version)) {
    $CargoToml = Get-Content -Path (Join-Path $Root "Cargo.toml") -Raw
    if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    } else {
        $Version = "0.1.0"
    }
}

$ArchSuffix = "x64"
if ($Target -like "aarch64*" -or ([string]::IsNullOrWhiteSpace($Target) -and $env:PROCESSOR_ARCHITECTURE -eq "ARM64")) {
    $ArchSuffix = "arm64"
}

if ([string]::IsNullOrWhiteSpace($Target)) {
    $TargetDir = Join-Path $Root "target\$Profile"
} else {
    $TargetDir = Join-Path $Root "target\$Target\$Profile"
}

$Stage = Join-Path $Root "dist\ImageOptim-Rust-windows-$ArchSuffix"
$ToolsDir = Join-Path $Stage "tools"
$ZipPath = Join-Path $Root "dist\ImageOptim-Rust-$Version-windows-$ArchSuffix.zip"

Push-Location $Root
try {
    $CargoArgs = @("build", "--workspace")
    if ($Profile -eq "release") {
        $CargoArgs += "--release"
    }
    if (-not [string]::IsNullOrWhiteSpace($Target)) {
        $CargoArgs += @("--target", $Target)
    }
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    if (Test-Path $Stage) {
        Remove-Item -Recurse -Force $Stage
    }
    New-Item -ItemType Directory -Force -Path $ToolsDir | Out-Null

    $BinPath = Join-Path $TargetDir "ImageOptim-Rust.exe"
    Copy-Item $BinPath (Join-Path $Stage "ImageOptim-Rust.exe")

    # Closed-source PNGOUT is never redistributed in public builds unless BUNDLE_PNGOUT=1.
    $PngoutWin = Join-Path $Root "third_party\bin\windows-x86_64\pngout.exe"
    if ($env:BUNDLE_PNGOUT -eq "1" -and (Test-Path $PngoutWin)) {
        Copy-Item -Path $PngoutWin -Destination (Join-Path $ToolsDir "pngout.exe") -Force
    }
    $BuiltTools = Join-Path $TargetDir "tools"
    if (Test-Path $BuiltTools) {
        Get-ChildItem -Path $BuiltTools -Filter "*.exe" -File | Copy-Item -Destination $ToolsDir -Force
    }
    $PngquantExe = Join-Path $TargetDir "pngquant.exe"
    if (Test-Path $PngquantExe) {
        Copy-Item -Path $PngquantExe -Destination (Join-Path $ToolsDir "pngquant.exe") -Force
    }

    # Copy canonical multi-resolution uncompressed BMP-DIB icon for Explorer and NSIS MUI2.
    $IcoPath = Join-Path $Stage "ImageOptim-Rust.ico"
    Copy-Item (Join-Path $Root "crates\app\assets\app-icon.ico") $IcoPath -Force

    # Per-user Explorer context menu installer (no Administrator UAC prompt needed).
    $InstallMenu = @'
$ErrorActionPreference = "Stop"
$Exe = Join-Path $PSScriptRoot "ImageOptim-Rust.exe"
$Ico = Join-Path $PSScriptRoot "ImageOptim-Rust.ico"
$Extensions = @(".png", ".jpg", ".jpeg", ".gif", ".svg")

foreach ($Ext in $Extensions) {
    $Key = "HKCU:\Software\Classes\SystemFileAssociations\$Ext\shell\ImageOptim-Rust"
    New-Item -Path "$Key\command" -Force | Out-Null
    Set-ItemProperty -Path $Key -Name "(Default)" -Value "Optimize with ImageOptim-Rust"
    Set-ItemProperty -Path $Key -Name "Icon" -Value "`"$Ico`""
    Set-ItemProperty -Path "$Key\command" -Name "(Default)" -Value "`"$Exe`" `"%1`""
}

$DirKey = "HKCU:\Software\Classes\Directory\shell\ImageOptim-Rust"
New-Item -Path "$DirKey\command" -Force | Out-Null
Set-ItemProperty -Path $DirKey -Name "(Default)" -Value "Optimize with ImageOptim-Rust"
Set-ItemProperty -Path $DirKey -Name "Icon" -Value "`"$Ico`""
Set-ItemProperty -Path "$DirKey\command" -Name "(Default)" -Value "`"$Exe`" `"%V`""
Write-Host "Registered Explorer right-click menu for PNG, JPEG, GIF, SVG, and folders."
'@
    Set-Content -Path (Join-Path $Stage "install-context-menu.ps1") -Value $InstallMenu -Encoding UTF8

    $UninstallMenu = @'
$ErrorActionPreference = "SilentlyContinue"
foreach ($Ext in @(".png", ".jpg", ".jpeg", ".gif", ".svg")) {
    Remove-Item -Recurse -Force "HKCU:\Software\Classes\SystemFileAssociations\$Ext\shell\ImageOptim-Rust"
}
Remove-Item -Recurse -Force "HKCU:\Software\Classes\Directory\shell\ImageOptim-Rust"
Write-Host "Removed ImageOptim-Rust Explorer right-click menu entries."
'@
    Set-Content -Path (Join-Path $Stage "uninstall-context-menu.ps1") -Value $UninstallMenu -Encoding UTF8

    # Generate NSIS installer script (`installer.nsi`) and build Setup .exe when `makensis` is installed.
    $NsiPath = Join-Path $Root "dist\installer.nsi"
    $SetupOut = Join-Path $Root "dist\ImageOptim-Rust-$Version-windows-$ArchSuffix-setup.exe"
    $NsiScript = @"
Unicode True
!include "MUI2.nsh"
Name "ImageOptim-Rust $Version"
OutFile "$SetupOut"
InstallDir "`$LOCALAPPDATA\Programs\ImageOptim-Rust"
RequestExecutionLevel user
!define MUI_ICON "$Stage\ImageOptim-Rust.ico"
!define MUI_UNICON "$Stage\ImageOptim-Rust.ico"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"

Section "Install"
  SetOutPath "`$INSTDIR"
  File "$Stage\ImageOptim-Rust.exe"
  File "$Stage\ImageOptim-Rust.ico"
  File "$Stage\install-context-menu.ps1"
  File "$Stage\uninstall-context-menu.ps1"
  CreateDirectory "`$SMPROGRAMS\ImageOptim-Rust"
  CreateShortcut "`$SMPROGRAMS\ImageOptim-Rust\ImageOptim-Rust.lnk" "`$INSTDIR\ImageOptim-Rust.exe" "" "`$INSTDIR\ImageOptim-Rust.ico"
  SetOutPath "`$INSTDIR\tools"
  File /nonfatal "$Stage\tools\*.exe"
  WriteUninstaller "`$INSTDIR\Uninstall.exe"

  ; Explorer right-click menu for PNG, JPEG, GIF, SVG and folders (HKCU, no UAC needed)
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.png\shell\ImageOptim-Rust" "" "Optimize with ImageOptim-Rust"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.png\shell\ImageOptim-Rust" "Icon" '"`$INSTDIR\ImageOptim-Rust.ico"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.png\shell\ImageOptim-Rust\command" "" '"`$INSTDIR\ImageOptim-Rust.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.jpg\shell\ImageOptim-Rust" "" "Optimize with ImageOptim-Rust"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.jpg\shell\ImageOptim-Rust" "Icon" '"`$INSTDIR\ImageOptim-Rust.ico"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.jpg\shell\ImageOptim-Rust\command" "" '"`$INSTDIR\ImageOptim-Rust.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.jpeg\shell\ImageOptim-Rust" "" "Optimize with ImageOptim-Rust"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.jpeg\shell\ImageOptim-Rust" "Icon" '"`$INSTDIR\ImageOptim-Rust.ico"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.jpeg\shell\ImageOptim-Rust\command" "" '"`$INSTDIR\ImageOptim-Rust.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.gif\shell\ImageOptim-Rust" "" "Optimize with ImageOptim-Rust"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.gif\shell\ImageOptim-Rust" "Icon" '"`$INSTDIR\ImageOptim-Rust.ico"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.gif\shell\ImageOptim-Rust\command" "" '"`$INSTDIR\ImageOptim-Rust.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.svg\shell\ImageOptim-Rust" "" "Optimize with ImageOptim-Rust"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.svg\shell\ImageOptim-Rust" "Icon" '"`$INSTDIR\ImageOptim-Rust.ico"'
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.svg\shell\ImageOptim-Rust\command" "" '"`$INSTDIR\ImageOptim-Rust.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\Directory\shell\ImageOptim-Rust" "" "Optimize with ImageOptim-Rust"
  WriteRegStr HKCU "Software\Classes\Directory\shell\ImageOptim-Rust" "Icon" '"`$INSTDIR\ImageOptim-Rust.ico"'
  WriteRegStr HKCU "Software\Classes\Directory\shell\ImageOptim-Rust\command" "" '"`$INSTDIR\ImageOptim-Rust.exe" "%V"'

  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust" "DisplayName" "ImageOptim-Rust $Version"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust" "Publisher" "Hex"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust" "URLInfoAbout" "https://github.com/hex-ci/imageoptim-rust"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust" "UninstallString" '"`$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust" "DisplayIcon" "`$INSTDIR\ImageOptim-Rust.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust" "DisplayVersion" "$Version"
SectionEnd

Section "Uninstall"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.png\shell\ImageOptim-Rust"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.jpg\shell\ImageOptim-Rust"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.jpeg\shell\ImageOptim-Rust"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.gif\shell\ImageOptim-Rust"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.svg\shell\ImageOptim-Rust"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\ImageOptim-Rust"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ImageOptim-Rust"
  RMDir /r "`$SMPROGRAMS\ImageOptim-Rust"
  RMDir /r "`$INSTDIR\tools"
  Delete "`$INSTDIR\ImageOptim-Rust.exe"
  Delete "`$INSTDIR\ImageOptim-Rust.ico"
  Delete "`$INSTDIR\install-context-menu.ps1"
  Delete "`$INSTDIR\uninstall-context-menu.ps1"
  Delete "`$INSTDIR\Uninstall.exe"
  RMDir "`$INSTDIR"
SectionEnd
"@
    Set-Content -Path $NsiPath -Value $NsiScript -Encoding UTF8

    if (Test-Path $ZipPath) {
        Remove-Item -Force $ZipPath
    }
    Compress-Archive -Path "$Stage\*" -DestinationPath $ZipPath
    Write-Host "built $ZipPath"

    if (Get-Command makensis -ErrorAction SilentlyContinue) {
        & makensis $NsiPath
        if ($LASTEXITCODE -ne 0) {
            throw "makensis failed with exit code $LASTEXITCODE"
        }
        Write-Host "built $SetupOut"
    }
} finally {
    Pop-Location
}
