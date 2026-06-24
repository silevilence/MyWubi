@echo off
REM MyWubi TIP Register/Unregister Script
REM Run as Administrator: right-click cmd.exe "Run as administrator", then run this script
REM Usage:
REM   register_tip.bat           - Register
REM   register_tip.bat unregister - Unregister

setlocal enabledelayedexpansion

set CLSID={C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8}
set PROFILE={C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD9}
set DLL=%~dp0im_engine.dll
set NAME=MyWubi
set LANGID=0x00000804

if /I "%1"=="unregister" goto :unregister

echo === Register MyWubi TIP ===
echo.

REM 1. HKCR\CLSID\{CLSID}
reg add "HKLM\SOFTWARE\Classes\CLSID\%CLSID%" /ve /d "%NAME%" /f >nul 2>&1
if errorlevel 1 (echo [FAIL] CLSID & goto :error)

REM 2. InprocServer32
reg add "HKLM\SOFTWARE\Classes\CLSID\%CLSID%\InprocServer32" /ve /d "%DLL%" /f >nul 2>&1
reg add "HKLM\SOFTWARE\Classes\CLSID\%CLSID%\InprocServer32" /v ThreadingModel /d "Apartment" /f >nul 2>&1

REM 3. ProgID
reg add "HKLM\SOFTWARE\Classes\CLSID\%CLSID%\ProgID" /ve /d "MyWubi.TextService.1" /f >nul 2>&1

REM 3.5. Implemented Categories (CATID_TIP) — 必需！否则灰显"仅桌面"
reg add "HKLM\SOFTWARE\Classes\CLSID\%CLSID%\Implemented Categories\{34745C63-B2F0-4784-8B67-5E12C8701A31}" /ve /d "" /f >nul 2>&1

REM 4. HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%" /ve /d "%NAME%" /f >nul 2>&1
if errorlevel 1 (echo [FAIL] CTF TIP & goto :error)

REM 5. CLSID subkey
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\CLSID" /ve /d "%CLSID%" /f >nul 2>&1

REM Display Description — 在键盘列表中显示的友好名称
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%" /v "Display Description" /d "%NAME%" /f >nul 2>&1

REM EnableCompatibleTsf — 避免"仅桌面"标记
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%" /v EnableCompatibleTsf /t REG_DWORD /d 1 /f >nul 2>&1

REM TIP Categories — 声明为键盘输入法
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\Category\Category\{34745C63-B2F0-4784-8B67-5E12C8701A31}" /ve /d "" /f >nul 2>&1
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\Category\Category\{3640E571-E878-4FE7-B341-35D393003EAB}" /ve /d "" /f >nul 2>&1

REM 6. LanguageProfile (zh-CN, Enable=1: allow adding from keyboard list)
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\LanguageProfile\%LANGID%\%PROFILE%" /v Description /d "%NAME%" /f >nul 2>&1
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\LanguageProfile\%LANGID%\%PROFILE%" /v IconFile /d "%DLL%" /f >nul 2>&1
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\LanguageProfile\%LANGID%\%PROFILE%" /v IconIndex /t REG_DWORD /d 0 /f >nul 2>&1
reg add "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%\LanguageProfile\%LANGID%\%PROFILE%" /v Enable /t REG_DWORD /d 1 /f >nul 2>&1
if errorlevel 1 (echo [FAIL] LanguageProfile & goto :error)

echo [ OK ] MyWubi TIP registered
echo.
echo Next steps:
echo   1. Settings ^> Time ^& Language ^> Language ^> Chinese ^> Options
echo   2. Add keyboard ^> "MyWubi"
echo   3. Win+Space to switch input method
echo.
echo Uninstall: %~nx0 unregister
goto :end

:unregister
echo === Unregister MyWubi TIP ===
reg delete "HKLM\SOFTWARE\Classes\CLSID\%CLSID%" /f >nul 2>&1
reg delete "HKLM\SOFTWARE\Microsoft\CTF\TIP\%CLSID%" /f >nul 2>&1
echo [ OK ] Unregistered
goto :end

:error
echo.
echo [ERROR] Registration failed. Are you running as Administrator?
echo Right-click cmd.exe ^> "Run as administrator" ^> retry.
echo.
pause
exit /b 1

:end
endlocal
