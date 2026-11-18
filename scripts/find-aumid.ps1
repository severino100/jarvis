# Lists Start menu shortcuts that carry Jarvis's AppUserModelID (jarvis.desktop), and where they point.
# Windows names and draws the notifications from one of these. Reads only.
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class AumidRead {
  [StructLayout(LayoutKind.Sequential, Pack = 4)] public struct PROPERTYKEY { public Guid fmtid; public uint pid; }
  [StructLayout(LayoutKind.Explicit, Size = 24)] public struct PROPVARIANT { [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr p; }
  [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IPropertyStore { int GetCount(out uint c); int GetAt(uint i, out PROPERTYKEY k); int GetValue(ref PROPERTYKEY k, out PROPVARIANT v); int SetValue(ref PROPERTYKEY k, ref PROPVARIANT v); int Commit(); }
  [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  static extern void SHGetPropertyStoreFromParsingName(string path, IntPtr bc, int flags, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IPropertyStore store);
  public static string Get(string lnk) {
    try {
      Guid iid = new Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"); IPropertyStore s;
      SHGetPropertyStoreFromParsingName(lnk, IntPtr.Zero, 0, ref iid, out s);
      PROPERTYKEY k = new PROPERTYKEY { fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), pid = 5 }; PROPVARIANT v;
      s.GetValue(ref k, out v); string r = v.vt == 31 ? Marshal.PtrToStringUni(v.p) : null; Marshal.ReleaseComObject(s); return r;
    } catch { return null; }
  }
}
'@
$roots = @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('CommonPrograms'), [Environment]::GetFolderPath('Desktop'))
$shell = New-Object -ComObject WScript.Shell
foreach ($root in $roots) {
  Get-ChildItem $root -Recurse -Filter *.lnk -ErrorAction SilentlyContinue | ForEach-Object {
    $id = [AumidRead]::Get($_.FullName)
    if ($id -like '*jarvis*' -or $_.Name -like '*lectron*' -or $_.Name -like '*arvis*') {
      "{0}`n   id: {1}`n   ->  {2}" -f $_.FullName, $id, $shell.CreateShortcut($_.FullName).TargetPath
    }
  }
}
