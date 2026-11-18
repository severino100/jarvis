# Shows whether Windows lets Jarvis show notifications (run from a normal terminal). Reads only.
$push = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\PushNotifications' -ErrorAction SilentlyContinue
"Notificações do Windows (geral): " + $(if ($push.ToastEnabled -eq 0) { 'DESLIGADAS' } else { 'ligadas' })
$app = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\jarvis.desktop' -ErrorAction SilentlyContinue
"Notificações do Jarvis: " + $(if ($app -and $app.Enabled -eq 0) { 'DESLIGADAS' } else { 'ligadas (ou ainda sem preferência)' })
$lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Jarvis.lnk'
"Atalho no menu Iniciar: " + $(if (Test-Path $lnk) { $lnk } else { 'EM FALTA' })
