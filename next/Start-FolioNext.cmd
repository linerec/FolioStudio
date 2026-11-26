@echo off
rem Folio Studio (new engine). Opens the window; use 열기 (Ctrl+O) to open an INDD/IDML.
rem The INDD reader only reads the original file; nothing is written next to it.
cd /d "%~dp0app"
if not exist "..\engine\target\release\folio-engine.exe" (
  echo Engine not built. Run: cd next\app ^&^& npm run build
  pause
  exit /b 1
)
if not exist "dist\index.html" call npx vite build
start "" "node_modules\electron\dist\electron.exe" . %*
