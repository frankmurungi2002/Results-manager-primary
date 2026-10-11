@echo off
rem Termbook: make a licence key for a school that has paid. Run it every payment.
setlocal
cd /d "%~dp0"
if not exist termbook-issuer.key goto nokey

echo.
echo  Termbook: make a licence key for a school
echo  ------------------------------------------
echo  The school code is on the school's Termbook: Settings, Subscription.
echo.
set /p SCHOOL= School code (like 9F3AC-2B1E0): 
set /p NAME= School name: 
set /p PLAN= Plan, type term or year: 
set /p FROM= Starts on, like 2026-09-01: 
set REF=
set /p REF= Payment reference (or just press Enter): 
echo.

if "%REF%"=="" goto noref
cargo run --release --quiet -- issue --school "%SCHOOL%" --name "%NAME%" --plan "%PLAN%" --from "%FROM%" --id "%REF%" > last-key.txt
goto done
:noref
cargo run --release --quiet -- issue --school "%SCHOOL%" --name "%NAME%" --plan "%PLAN%" --from "%FROM%" > last-key.txt

:done
if errorlevel 1 goto failed
type last-key.txt
start notepad last-key.txt
echo  The key is also open in Notepad: copy the line starting TBK1. and send it to the school.
echo.
pause
exit /b 0

:failed
echo.
echo  No key was made. Read the message above, then try again.
echo.
pause
exit /b 1

:nokey
echo.
echo  There is no signing key in this folder yet (termbook-issuer.key).
echo  Run make-signing-key.bat first - once, ever. If you made one before,
echo  copy termbook-issuer.key back into this folder from your USB stick.
echo.
pause
exit /b 1
