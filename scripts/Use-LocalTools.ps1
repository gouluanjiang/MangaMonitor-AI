$phaseRoot = Split-Path -Parent $PSScriptRoot
$env:CARGO_HOME = Join-Path $phaseRoot '.tools\cargo'
$env:RUSTUP_HOME = Join-Path $phaseRoot '.tools\rustup'
$env:GH_CONFIG_DIR = Join-Path $phaseRoot '.tools\gh-config'
$env:PATH = "$phaseRoot\.tools\w64devkit\bin;$phaseRoot\.tools\cargo\bin;$phaseRoot\.tools\gh\bin;" + $env:PATH
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "$phaseRoot\.tools\rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained\x86_64-w64-mingw32-gcc.exe"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = '-C link-self-contained=yes'
