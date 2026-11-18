# Shows one test notification as "Jarvis" (same AppUserModelID the app uses). Run from a normal terminal.
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml('<toast><visual><binding template="ToastGeneric"><text>Jarvis</text><text>Teste: as notificações estão a funcionar.</text></binding></visual></toast>')
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('jarvis.desktop').Show([Windows.UI.Notifications.ToastNotification]::new($xml))
'notificação enviada'
