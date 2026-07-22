@echo off
if "%CLEARRA_CORE_C_LIB_DIR%"=="" (
    echo CLEARRA_CORE_C_LIB_DIR is required for native Clearra linking. 1>&2
    exit /b 2
)
%* -L "native=%CLEARRA_CORE_C_LIB_DIR%"
