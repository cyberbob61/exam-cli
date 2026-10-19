@echo off
rem Builds exam-cli.exe with the rustup GNU toolchain (no Visual Studio needed).
rem The bundled GNU dlltool needs an assembler that rustup does not ship, so we use
rem llvm-dlltool (= llvm-ar from the llvm-tools component, renamed) instead, and link with
rem rust-lld: GNU ld mis-links llvm-dlltool import libs (the exe segfaults at startup).
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set "BIN=%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin"

if not exist "%BIN%\llvm-ar.exe" rustup component add llvm-tools || exit /b 1
if not exist tools mkdir tools
if not exist tools\llvm-dlltool.exe copy /y "%BIN%\llvm-ar.exe" tools\llvm-dlltool.exe >nul

set "RUSTFLAGS=-Cdlltool=%CD%\tools\llvm-dlltool.exe -Clinker=rust-lld -Clinker-flavor=ld.lld -Clink-self-contained=yes"
cargo test --release || exit /b 1
cargo build --release || exit /b 1
copy /y target\release\exam-cli.exe . >nul || (echo exam-cli.exe is running - close it and run build.cmd again & exit /b 1)
echo.
echo Done: %CD%\exam-cli.exe
