# Instala o Jarvis para este utilizador do Windows, sem compilar nada:
#   irm https://github.com/severino100/jarvis/releases/latest/download/install.ps1 | iex
# ou, com o ficheiro descarregado:
#   powershell -ExecutionPolicy Bypass -File install.ps1 [-SemVoz] [-Versao v0.2.0] [-Exe <jarvis.exe local>] [-Verificar]
#
# -Verificar so diz o que falta neste PC (git, motores de IA) e como o instalar, sem instalar nada.
# - descarrega o jarvis.exe do release (o mais recente, ou -Versao) para %LOCALAPPDATA%\Programs\Jarvis;
# - descarrega a voz (Whisper para ouvir, Piper com a voz pt-PT para falar, ~600 MB) das fontes oficiais para
#   %LOCALAPPDATA%\Jarvis\vendor, a nao ser com -SemVoz (o Jarvis funciona sem voz);
# - cria os atalhos "Jarvis" no menu Iniciar e no Ambiente de Trabalho, liga o arranque com o Windows e abre-o.
# Nao pede contas nem chaves: cada motor de IA (Claude Code, Antigravity, Codex) usa a sessao que ja tens nele.
param([switch]$SemVoz, [string]$Versao = '', [string]$Exe = '', [switch]$Verificar)
# PowerShell names are case-insensitive: $Exe is read here, before $exe below reuses the name for the destination.
$localExe = $Exe
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = 'severino100/jarvis'
$appDir = Join-Path $env:LOCALAPPDATA 'Programs\Jarvis'
$exe = Join-Path $appDir 'jarvis.exe'
$vendor = Join-Path $env:LOCALAPPDATA 'Jarvis\vendor'
$appId = 'jarvis.desktop'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

# Fontes oficiais da voz (versoes fixas, as que o Jarvis testou).
$whisperZip = 'https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip'
$whisperModel = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin'
$piperZip = 'https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip'
$voiceBase = 'https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/pt/pt_PT/tug%C3%A3o/medium/pt_PT-tug%C3%A3o-medium'

function Get-File($url, $dest) {
  $tmp = "$dest.partial"
  Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing
  Move-Item $tmp $dest -Force
}

# Este ficheiro fica so em ASCII: o PowerShell 5.1 le-o como ANSI (com -File) ou como ISO-8859-1 (com irm | iex), e
# com BOM o [scriptblock]::Create((irm ...)) falha no param(). Os acentos do texto mostrado escrevem-se com
# marcadores, trocados ao mostrar: {a/} agudo, {a~} til, {a^} circunflexo, {a\} grave, {c,} cedilha, {->} seta,
# {...} reticencias, {--} travessao.
$script:PtMarks = @(
  @('{a/}', 0xE1), @('{e/}', 0xE9), @('{i/}', 0xED), @('{o/}', 0xF3), @('{u/}', 0xFA), @('{A/}', 0xC1), @('{E/}', 0xC9),
  @('{a~}', 0xE3), @('{o~}', 0xF5), @('{a^}', 0xE2), @('{e^}', 0xEA), @('{o^}', 0xF4), @('{a\}', 0xE0), @('{c,}', 0xE7),
  @('{->}', 0x2192), @('{...}', 0x2026), @('{--}', 0x2014)
)
function Pt([string]$text) {
  foreach ($m in $script:PtMarks) { $text = $text.Replace($m[0], [string][char]$m[1]) }
  return $text
}
function Say([string]$text, [string]$ForegroundColor = '') {
  if ($ForegroundColor) { Write-Host (Pt $text) -ForegroundColor $ForegroundColor } else { Write-Host (Pt $text) }
}

function Step($text) { Write-Host (Pt "-> $text") -ForegroundColor Cyan }

# O que o Jarvis usa e nao instala: o git (pacotes de equipa e worktrees) e pelo menos um motor de IA, cada um com a
# conta de quem o usa. Procura nos mesmos sitios que o Jarvis (native/src/brain.rs).
function Find-First($paths, $command) {
  foreach ($p in $paths) { if ($p -and (Test-Path $p)) { return $p } }
  $c = Get-Command $command -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($c) { return $c.Source }
  return $null
}

function Show-Requisitos {
  $home_ = $env:USERPROFILE
  $git = Find-First @("$env:ProgramFiles\Git\cmd\git.exe", "$env:LOCALAPPDATA\Programs\Git\cmd\git.exe") 'git'
  $claude = Find-First @("$home_\.local\bin\claude.exe") 'claude'
  $agy = Find-First @("$home_\.agy\bin\agy.exe", "$env:LOCALAPPDATA\agy\bin\agy.exe") 'agy'
  $codexApp = $null
  try { $codexApp = (Get-AppxPackage OpenAI.Codex -ErrorAction SilentlyContinue).InstallLocation } catch { }
  $codex = Find-First @($codexApp, "$env:APPDATA\npm\node_modules\@openai\codex\package.json") 'codex'

  $items = @(
    @{ Nome = 'Git'; Ok = $git; Para = (Pt 'pacotes de equipa, reposit{o/}rios e worktrees')
       Como = @('winget install Git.Git', 'ou descarrega em https://git-scm.com/downloads/win') },
    @{ Nome = 'Claude Code'; Ok = $claude; Para = (Pt 'motor de IA (subscri{c,}{a~}o Claude Pro ou Max)')
       Como = @('irm https://claude.ai/install.ps1 | iex', (Pt 'depois, num terminal novo: claude  (faz o login com a conta, n{a~}o com chave de API)'), 'ajuda: https://code.claude.com/docs/en/setup') },
    @{ Nome = 'Antigravity'; Ok = $agy; Para = 'motor de IA (conta Google)'
       Como = @('irm https://antigravity.google/cli/install.ps1 | iex', 'depois, num terminal novo: agy  (faz o login com a conta Google)', 'ajuda: https://antigravity.google') },
    @{ Nome = 'Codex'; Ok = $codex; Para = 'motor de IA (conta ChatGPT)'
       Como = @('instala a app Codex: https://developers.openai.com/codex', 'ou, com o Node.js (https://nodejs.org/en/download): npm i -g @openai/codex', (Pt 'depois: codex login  (escolhe "Sign in with ChatGPT", n{a~}o chave de API)')) }
  )

  Write-Host ''
  Write-Host 'O que o Jarvis precisa neste PC:' -ForegroundColor Cyan
  foreach ($i in $items) {
    if ($i.Ok) {
      Write-Host ("  OK     {0}" -f $i.Nome) -ForegroundColor Green
    } else {
      Write-Host ("  FALTA  {0}: {1}" -f $i.Nome, $i.Para) -ForegroundColor Yellow
      foreach ($l in $i.Como) { Write-Host "         $l" }
    }
  }
  $motores = @($claude, $agy, $codex) | Where-Object { $_ }
  Write-Host ''
  if (-not $git) {
    Say 'Sem o git, o Jarvis n{a~}o consegue trazer o pacote da tua equipa nem preparar worktrees.' -ForegroundColor Yellow
  }
  if ($motores.Count -eq 0) {
    Say 'Precisas de pelo menos um motor de IA (Claude Code, Antigravity ou Codex) para as conversas. Instala um dos de cima e abre o Jarvis outra vez.' -ForegroundColor Yellow
  } elseif ($git) {
    Say 'Est{a/} tudo o que {e/} preciso. Os motores que faltam s{a~}o opcionais: a conversa passa para eles quando o plano de outro acaba.' -ForegroundColor Green
  }
}

if ($Verificar) { Show-Requisitos; return }

# 1. O programa
Step $(if ($localExe) { 'A copiar o Jarvis{...}' } else { 'A descarregar o Jarvis{...}' })
New-Item -ItemType Directory -Force $appDir | Out-Null
$release = if ($Versao) { "https://github.com/$repo/releases/download/$Versao" } else { "https://github.com/$repo/releases/latest/download" }
# Fecha o Jarvis que estiver aberto (o Windows nao deixa substituir o exe enquanto corre).
for ($i = 0; $i -lt 30; $i++) {
  $left = Get-Process jarvis -ErrorAction SilentlyContinue
  if (-not $left) { break }
  $left | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
}
Start-Sleep -Milliseconds 500
if ($localExe) { Copy-Item $localExe $exe -Force } else { Get-File "$release/jarvis.exe" $exe }

# 2. A voz
if (-not $SemVoz) {
  New-Item -ItemType Directory -Force (Join-Path $vendor 'models'), (Join-Path $vendor 'voices') | Out-Null
  $tmpDir = Join-Path $env:TEMP 'jarvis-instalar'
  New-Item -ItemType Directory -Force $tmpDir | Out-Null
  if (-not (Test-Path (Join-Path $vendor 'whisper\Release\whisper-cli.exe'))) {
    Step 'A descarregar o Whisper (reconhecimento de voz){...}'
    Get-File $whisperZip (Join-Path $tmpDir 'whisper.zip')
    Expand-Archive (Join-Path $tmpDir 'whisper.zip') (Join-Path $vendor 'whisper') -Force
  }
  if (-not (Test-Path (Join-Path $vendor 'models\ggml-small.bin'))) {
    Step 'A descarregar o modelo do Whisper (~460 MB, demora uns minutos){...}'
    Get-File $whisperModel (Join-Path $vendor 'models\ggml-small.bin')
  }
  if (-not (Test-Path (Join-Path $vendor 'piper\piper.exe'))) {
    Step 'A descarregar a Piper (voz){...}'
    Get-File $piperZip (Join-Path $tmpDir 'piper.zip')
    Expand-Archive (Join-Path $tmpDir 'piper.zip') $vendor -Force
  }
  if (-not (Test-Path (Join-Path $vendor 'voices\pt_PT-tugao-medium.onnx'))) {
    Step 'A descarregar a voz portuguesa{...}'
    Get-File "$voiceBase.onnx" (Join-Path $vendor 'voices\pt_PT-tugao-medium.onnx')
    Get-File "$voiceBase.onnx.json" (Join-Path $vendor 'voices\pt_PT-tugao-medium.onnx.json')
  }
  Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}

# 3. Atalhos (com o AppUserModelID das notificacoes) e arranque com o Windows
Step 'A criar os atalhos{...}'
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class JarvisAumid {
  [StructLayout(LayoutKind.Sequential, Pack = 4)] public struct PROPERTYKEY { public Guid fmtid; public uint pid; }
  [StructLayout(LayoutKind.Explicit, Size = 24)] public struct PROPVARIANT { [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr p; }
  [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IPropertyStore {
    int GetCount(out uint c); int GetAt(uint i, out PROPERTYKEY k); int GetValue(ref PROPERTYKEY k, out PROPVARIANT v);
    int SetValue(ref PROPERTYKEY k, ref PROPVARIANT v); int Commit();
  }
  [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  static extern void SHGetPropertyStoreFromParsingName(string path, IntPtr bc, int flags, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IPropertyStore store);
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
$shell = New-Object -ComObject WScript.Shell
foreach ($lnk in @((Join-Path ([Environment]::GetFolderPath('Programs')) 'Jarvis.lnk'), (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Jarvis.lnk'))) {
  $s = $shell.CreateShortcut($lnk)
  $s.TargetPath = $exe
  $s.WorkingDirectory = $appDir
  $s.IconLocation = "$exe,0"
  $s.Description = (Pt 'Jarvis {--} o teu assistente')
  $s.Save()
  [JarvisAumid]::Set($lnk, $appId)
}
Set-ItemProperty $runKey -Name $appId -Value "`"$exe`" --hidden"

Step 'A abrir o Jarvis{...}'
Start-Process -FilePath $exe -WorkingDirectory $appDir
Write-Host ''
Say 'Jarvis instalado. Em Defini{c,}{o~}es {->} Perfil e equipa: o teu nome, a pasta dos projetos e, se tiveres, o pacote da tua equipa.' -ForegroundColor Green
Say 'Os motores de IA usam as tuas pr{o/}prias sess{o~}es e subscri{c,}{o~}es: nenhum recebe chaves de API.'
Show-Requisitos
