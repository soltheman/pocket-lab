@echo off
rem Plays Kino der Toten on this PC with the Black Ops files staged for the iPhone
rem (scripts\stage-bo1-for-iphone.ps1 puts them in %USERPROFILE%\BO1-iPhone).
rem Build it first: cargo build --profile play -p launcher --bin iw4l
rem
rem Co-op: with "Pocket Lab Co-op.iw4l-server" (made by the co-op server) next to
rem iw4l.exe, the game joins an open co-op game on that server or hosts one.
setlocal
set "EXE=%~dp0..\target\play\iw4l.exe"
if not exist "%EXE%" (
    echo iw4l.exe is not built yet: run  cargo build --profile play -p launcher --bin iw4l
    pause
    exit /b 1
)
if not defined IW4L_GAMES set "IW4L_GAMES=%USERPROFILE%\BO1-iPhone"
for %%F in ("%~dp0..\target\play\co-op\*.iw4l-server") do copy /y "%%F" "%~dp0..\target\play\" >nul
"%EXE%" map t5:zombie_theater
