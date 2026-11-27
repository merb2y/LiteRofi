Set WshShell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
scriptDir = fso.GetParentFolderName(WScript.ScriptFullName)
For Each studioExe In Array(fso.BuildPath(scriptDir, "..\target\release\literofi-studio.exe"), WshShell.ExpandEnvironmentStrings("%LOCALAPPDATA%\LiteRofi\literofi-studio.exe"))
    If fso.FileExists(studioExe) Then
        WshShell.Run Chr(34) & studioExe & Chr(34), 0, False
        WScript.Quit
    End If
Next
serverPy = fso.BuildPath(scriptDir, "server.py")

pyw = WshShell.ExpandEnvironmentStrings("%LOCALAPPDATA%\Programs\Python\Python312\pythonw.exe")
If Not fso.FileExists(pyw) Then
    pyw = "pythonw.exe"
End If

WshShell.Run Chr(34) & pyw & Chr(34) & " " & Chr(34) & serverPy & Chr(34), 0, False
