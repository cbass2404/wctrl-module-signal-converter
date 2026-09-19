' run-hidden.vbs - start the DCS Signal Converter daemon with no console window.
'
' Not something to double-click: it is the shim the DCS hook uses. DCS's
' os.execute() would otherwise flash a console window on every mission start.
'
' Takes one optional argument, the number of seconds of export-stream silence
' after which the daemon clears the panels and exits. The hook passes it; a
' missing or unreadable value falls back to a sane default rather than leaving
' the daemon running forever with the panels lit.
Option Explicit
Dim shell, fso, here, exePath, idle, cmd
Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

here = fso.GetParentFolderName(WScript.ScriptFullName)
exePath = here & "\dcs-signal.exe"
If Not fso.FileExists(exePath) Then WScript.Quit 1

idle = 20
If WScript.Arguments.Count > 0 Then
    If IsNumeric(WScript.Arguments(0)) Then idle = CLng(WScript.Arguments(0))
End If

' Run from the install folder, so the daemon's own relative data paths resolve.
shell.CurrentDirectory = here

cmd = """" & exePath & """ run --exit-when-idle " & idle
' 0 = hidden window, False = do not wait
shell.Run cmd, 0, False
