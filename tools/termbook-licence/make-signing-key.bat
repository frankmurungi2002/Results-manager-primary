@echo off
rem Termbook: make YOUR signing key. Do this ONCE, ever, on your own computer.
cd /d "%~dp0"
echo.
echo  Making your Termbook signing key (one time only)...
echo.
cargo run --release --quiet -- keygen
echo.
echo  NEXT: copy the "pub const ISSUER_PUBLIC_KEY_HEX" line above into
echo  src-tauri\src\licence.rs (replace the empty one), then build Termbook again.
echo  Copy termbook-issuer.key onto a USB stick and keep it somewhere safe.
echo.
pause
