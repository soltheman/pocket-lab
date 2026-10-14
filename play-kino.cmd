@echo off
rem Walk around Kino der Toten in the BO1 Pocket engine (PC dev build, no zombies yet).
rem The games root is the Black Ops folder alone: pointing it at the whole Steam library
rem makes every zone lookup scan BO2/BO3 too (about a minute per load).
set "IW4L_GAMES=C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Black Ops"
set "IW4L_SOUND=off"
start "" "%~dp0target\play\iw4l.exe" map t5:zombie_theater
