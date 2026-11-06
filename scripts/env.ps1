# Dot-source before cargo / npm run tauri on Windows:
#   . .\scripts\env.ps1
# Resolves the installed MSVC toolchain; no machine-specific drive letters.
$ErrorActionPreference = 'Stop'

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path -LiteralPath $cargoBin) {
    $env:Path = $cargoBin + ';' + $env:Path
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) {
    throw '未找到 Visual Studio Build Tools，请安装“使用 C++ 的桌面开发”和 Windows SDK。'
}
$visualStudioPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $visualStudioPath) {
    throw '未找到 MSVC x64 工具链，请安装 Visual Studio C++ Build Tools。'
}
$devShell = Join-Path $visualStudioPath 'Common7\Tools\Launch-VsDevShell.ps1'
& $devShell -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw '未找到 cargo，请安装 Rust MSVC stable 工具链。'
}
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-msvc'

# Keep compiler/test temporary files within this checkout. This also supports
# machines where native child processes cannot write the user TEMP directory.
$buildTemp = Join-Path (Split-Path -Parent $PSScriptRoot) 'audit.local\build-temp'
New-Item -ItemType Directory -Path $buildTemp -Force | Out-Null
$env:TMP = $buildTemp
$env:TEMP = $buildTemp
Write-Host '[shiftnote] MSVC x64 环境已加载' -ForegroundColor DarkGray
