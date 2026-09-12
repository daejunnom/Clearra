@echo off
node "%~dp0clearra-rustc-guard.mjs" %*
exit /b %errorlevel%
