Option Explicit

Dim fileSystem
Dim shell
Dim runtimeDirectory
Dim watcherPath
Dim configPath
Dim command
Dim exitCode

Set fileSystem = CreateObject("Scripting.FileSystemObject")
Set shell = CreateObject("WScript.Shell")
runtimeDirectory = fileSystem.GetParentFolderName(WScript.ScriptFullName)
watcherPath = fileSystem.BuildPath(runtimeDirectory, "clearra-local-services-watchdog.ps1")
configPath = fileSystem.BuildPath(runtimeDirectory, "clearra-local-services-watchdog.json")

command = "powershell.exe -NoLogo -NoProfile -NonInteractive " & _
    "-ExecutionPolicy Bypass -WindowStyle Hidden -File """ & watcherPath & """ " & _
    "-ConfigPath """ & configPath & """"

' Window style 0 is mandatory: the launcher and child PowerShell stay hidden.
' Waiting keeps Task Scheduler bound to the watchdog lifetime and propagates a
' crash so the registered one-minute restart policy can actually run.
exitCode = shell.Run(command, 0, True)
WScript.Quit exitCode
