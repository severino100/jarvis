# Installs Jarvis for this Windows user:
#  - stops a running Jarvis, copies native\target\release\jarvis.exe to %USERPROFILE%\.jarvis\jarvis.exe (so rebuilding
#    does not fight a running copy);
#  - points the Start menu and Desktop "Jarvis" shortcuts at it, with the AppUserModelID the notifications use;
#  - sets "start with Windows" (HKCU Run) to it, hidden in the tray;
#  - starts it.
# Run it from a normal terminal, not from processes inside the Claude app (their AppData/registry writes are
# virtualized into the app's container).
param([switch]$NoStart)
$ErrorActionPreference = 'Stop'
$jarvisDir = Split-Path $PSScriptRoot -Parent
$built = Join-Path $jarvisDir 'native\target\release\jarvis.exe'
$installDir = Join-Path $env:USERPROFILE '.jarvis'
$exe = Join-Path $installDir 'jarvis.exe'
$icon = Join-Path $jarvisDir 'assets\icon.ico'
$appId = 'jarvis.desktop'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$shortcuts = @(
  (Join-Path ([Environment]::GetFolderPath('Programs')) 'Jarvis.lnk'),
  (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Jarvis.lnk')
)

Add-Type @'
using System; using System.Runtime.InteropServices;
public static class Aumid {
  [StructLayout(LayoutKind.Sequential, Pack = 4)] public struct PROPERTYKEY { public Guid fmtid; public uint pid; }
  [StructLayout(LayoutKind.Explicit, Size = 24)] public struct PROPVARIANT { [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr p; }
  [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IPropertyStore {
    int GetCount(out uint c); int GetAt(uint i, out PROPERTYKEY k); int GetValue(ref PROPERTYKEY k, out PROPVARIANT v);
    int SetValue(ref PROPERTYKEY k, ref PROPVARIANT v); int Commit();
  }
  [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  static extern void SHGetPropertyStoreFromParsingName(string path, IntPtr bc, int flags, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IPropertyStore store);
  // Sets System.AppUserModel.ID on a .lnk, so toasts from that app id show with this shortcut's name and icon.
  public static void Set(string lnk, string id) {
    Guid iid = new Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99");
    IPropertyStore store;
    SHGetPropertyStoreFromParsingName(lnk, IntPtr.Zero, 2, ref iid, out store);
    PROPERTYKEY key = new PROPERTYKEY { fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), pid = 5 };
    PROPVARIANT v = new PROPVARIANT { vt = 31, p = Marshal.StringToCoTaskMemUni(id) };
    try { Marshal.ThrowExceptionForHR(store.SetValue(ref key, ref v)); Marshal.ThrowExceptionForHR(store.Commit()); }
    finally { Marshal.FreeCoTaskMem(v.p); Marshal.ReleaseComObject(store); }
  }
}
'@

function Write-Shortcut($path, $target, $workDir) {
  $shell = New-Object -ComObject WScript.Shell
  $s = $shell.CreateShortcut($path)
  $s.TargetPath = $target
  $s.Arguments = ''
  $s.WorkingDirectory = $workDir
  $s.IconLocation = "$icon,0"
  $s.Description = 'Jarvis — o teu assistente de desenvolvimento'
  $s.Save()
  [Aumid]::Set($path, $appId)
}

# Wait until every Jarvis process is really gone (Windows keeps the exe locked until then), up to 15 s.
for ($i = 0; $i -lt 30; $i++) {
  $left = Get-Process jarvis -ErrorAction SilentlyContinue
  if (-not $left) { break }
  $left | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
}
Start-Sleep -Milliseconds 500

if (-not (Test-Path $built)) { throw "Falta compilar: cargo build --release em $jarvisDir\native" }
New-Item -ItemType Directory -Force $installDir | Out-Null
$copied = $false
for ($i = 0; $i -lt 10 -and -not $copied; $i++) {
  try { Copy-Item $built $exe -Force -ErrorAction Stop; $copied = $true } catch { Start-Sleep -Seconds 1 }
}
if (-not $copied) { throw "Não consegui substituir $exe (ainda está em uso). Fecha o Jarvis pelo ícone da bandeja e corre isto outra vez." }
Write-Host "Copiado para $exe"
foreach ($p in $shortcuts) { Write-Shortcut $p $exe $installDir; Write-Host "Atalho: $p" }
Set-ItemProperty $runKey -Name $appId -Value "`"$exe`" --hidden"
Write-Host 'Arranque com o Windows: ligado (escondido na bandeja).'
if (-not $NoStart) { Start-Process -FilePath $exe -WorkingDirectory $installDir; Write-Host 'Jarvis arrancado.' }
