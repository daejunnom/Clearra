Option Explicit

Dim shell
Dim command
Dim exitCode

If WScript.Arguments.Count <> 4 Then
    WScript.Quit 2
End If

Set shell = CreateObject("WScript.Shell")
command = Quote(WScript.Arguments(0)) & _
    " -B " & Quote(WScript.Arguments(1)) & _
    " runtime run --producer management --profile local-service --timeout 7200 -- " & _
    "powershell.exe -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -File " & _
    Quote(WScript.Arguments(2)) & " -ConfigPath " & Quote(WScript.Arguments(3))

' Window style 0 keeps the launcher, Python supervisor, PowerShell watchdog,
' and descendants hidden. Waiting keeps Task Scheduler bound to the complete
' 120-minute lease and lets a typed timeout trigger the configured restart.
exitCode = shell.Run(command, 0, True)
WScript.Quit exitCode

Function Quote(value)
    Quote = Chr(34) & Replace(value, Chr(34), Chr(34) & Chr(34)) & Chr(34)
End Function
