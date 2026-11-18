# Quick check of the Rust Jarvis install (run from a normal terminal): process, start-with-Windows entry, shortcut
# target and the Rust data files.
Get-Process jarvis -ErrorAction SilentlyContinue | Select-Object Name, Id, @{n = 'MB'; e = { [int]($_.WorkingSet64 / 1MB) } }, Path | Format-Table -AutoSize
(Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run').'jarvis.desktop'
$s = (New-Object -ComObject WScript.Shell).CreateShortcut("$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Jarvis.lnk")
"atalho -> $($s.TargetPath)"
foreach ($f in 'chats-native.json', 'chats.json') {
  $p = Join-Path "$env:APPDATA\Jarvis" $f
  if (Test-Path $p) { "${f}: $((Get-Content $p -Raw | ConvertFrom-Json).chats.Count) conversas, $((Get-Item $p).LastWriteTime)" }
}
