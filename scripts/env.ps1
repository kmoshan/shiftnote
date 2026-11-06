# ShiftNote 开发环境变量（本机工具链安装在 D: 盘）
#
# 用法：在 PowerShell 中先执行
#   . .\scripts\env.ps1
# 再运行 npm / cargo / tauri 命令。
#
# 说明：Rust 的 msvc 目标需要 link.exe 与 Windows SDK 的 LIB/INCLUDE，
# 这里等价于自动执行 vcvars64.bat 后固化下来的环境。

$env:RUSTUP_HOME = 'D:\dev\rust\rustup'
$env:CARGO_HOME = 'D:\dev\rust\cargo'
$env:CARGO_TARGET_DIR = 'D:\dev\target'

$msvcRoot = 'D:\dev\vsbt\VC\Tools\MSVC\14.44.35207'
$sdkRoot = 'C:\Program Files (x86)\Windows Kits\10'
$sdkVer = '10.0.22621.0'

$env:INCLUDE = @(
  "$msvcRoot\include",
  'D:\dev\vsbt\VC\Auxiliary\VS\include',
  "$sdkRoot\include\$sdkVer\ucrt",
  "$sdkRoot\include\$sdkVer\um",
  "$sdkRoot\include\$sdkVer\shared",
  "$sdkRoot\include\$sdkVer\winrt",
  "$sdkRoot\include\$sdkVer\cppwinrt"
) -join ';'

$env:LIB = @(
  "$msvcRoot\lib\x64",
  "$sdkRoot\lib\$sdkVer\ucrt\x64",
  "$sdkRoot\lib\$sdkVer\um\x64"
) -join ';'

$pathParts = @(
  'D:\dev\rust\cargo\bin',
  "$msvcRoot\bin\HostX64\x64"
) + ($env:Path -split ';')
$env:Path = ($pathParts | Where-Object { $_ } | Select-Object -Unique) -join ';'

Write-Host "[shiftnote] Rust/MSVC 环境已加载 (target: $env:CARGO_TARGET_DIR)" -ForegroundColor DarkGray
