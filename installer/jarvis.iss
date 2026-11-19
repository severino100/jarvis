; Jarvis-Setup.exe: o instalador que se descarrega do release e se passa às pessoas.
; Instala para o utilizador (sem administrador) em %LOCALAPPDATA%\Programs\Jarvis, descarrega a voz das fontes
; oficiais (opcional), cria os atalhos com o AppUserModelID das notificações, liga o arranque com o Windows e, no fim,
; diz o que falta neste PC (git, motores de IA) com o comando e o link de cada um.
;
; Compilar (Inno Setup 6.5 ou mais recente):
;   ISCC.exe installer\jarvis.iss                          (usa native\target\release\jarvis.exe)
;   ISCC.exe /DExePath=C:\...\jarvis.exe installer\jarvis.iss

#ifndef ExePath
  #define ExePath "..\native\target\release\jarvis.exe"
#endif
#define AppVersion GetStringFileInfo(ExePath, "ProductVersion")
#if AppVersion == ""
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6F1E2C1A-3B7D-4E4A-9C5B-4A1D2E7F9B30}
AppName=Jarvis
AppVersion={#AppVersion}
AppVerName=Jarvis {#AppVersion}
AppPublisher=Jarvis
AppPublisherURL=https://github.com/severino100/jarvis
AppSupportURL=https://github.com/severino100/jarvis
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\Jarvis
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=no
OutputDir=Output
OutputBaseFilename=Jarvis-Setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\jarvis.exe
UninstallDisplayName=Jarvis
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
ArchiveExtraction=full
CloseApplications=yes
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0

[Languages]
Name: "pt"; MessagesFile: "compiler:Languages\Portuguese.isl"

[Messages]
pt.WelcomeLabel2=Isto instala o Jarvis, o teu assistente de desktop, só para o teu utilizador (não precisa de administrador).%n%nNo fim digo-te o que falta neste PC (git e um motor de IA) e como o instalar.

[Tasks]
Name: "voz"; Description: "Descarregar a voz (Whisper para ouvir e Piper para falar em português, cerca de 555 MB)"
Name: "arranque"; Description: "Abrir o Jarvis quando o Windows arranca"
Name: "ambiente"; Description: "Atalho no Ambiente de Trabalho"

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "jarvis.exe"; Flags: ignoreversion

; A voz, das fontes oficiais, nas versões que o Jarvis testou. Só desce o que ainda não existe.
Source: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip"; DestName: "whisper-bin-x64.zip"; \
  DestDir: "{localappdata}\Jarvis\vendor\whisper"; ExternalSize: 8_928_640; \
  Hash: "6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb"; \
  Flags: external download extractarchive recursesubdirs createallsubdirs ignoreversion; Tasks: voz; \
  Check: Falta('whisper\Release\whisper-cli.exe')
Source: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"; DestName: "ggml-small.bin"; \
  DestDir: "{localappdata}\Jarvis\vendor\models"; ExternalSize: 487_601_967; \
  Hash: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"; \
  Flags: external download ignoreversion; Tasks: voz; Check: Falta('models\ggml-small.bin')
Source: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip"; DestName: "piper_windows_amd64.zip"; \
  DestDir: "{localappdata}\Jarvis\vendor"; ExternalSize: 22_477_236; \
  Hash: "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea"; \
  Flags: external download extractarchive recursesubdirs createallsubdirs ignoreversion; Tasks: voz; \
  Check: Falta('piper\piper.exe')
Source: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/pt/pt_PT/tug%C3%A3o/medium/pt_PT-tug%C3%A3o-medium.onnx"; \
  DestName: "pt_PT-tugao-medium.onnx"; DestDir: "{localappdata}\Jarvis\vendor\voices"; ExternalSize: 63_201_294; \
  Hash: "223a7aaca69a155c61897e8ada7c3b13bc306e16c72dbb9c2fed733e2b0927d4"; \
  Flags: external download ignoreversion; Tasks: voz; Check: Falta('voices\pt_PT-tugao-medium.onnx')
Source: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/pt/pt_PT/tug%C3%A3o/medium/pt_PT-tug%C3%A3o-medium.onnx.json"; \
  DestName: "pt_PT-tugao-medium.onnx.json"; DestDir: "{localappdata}\Jarvis\vendor\voices"; ExternalSize: 5_026; \
  Hash: "fe0918dfc0f1a89264a6eea4afe8e95d8e9fed3cc6c81b5c2f87fcb2b50c7320"; \
  Flags: external download ignoreversion; Tasks: voz; Check: Falta('voices\pt_PT-tugao-medium.onnx.json')

; O Whisper e a Piper precisam do Microsoft Visual C++ Redistributable, que um Windows acabado de instalar não tem.
; Só desce quando falta; instala-se no fim (pede administrador uma vez).
Source: "https://aka.ms/vs/17/release/vc_redist.x64.exe"; DestName: "vc_redist.x64.exe"; DestDir: "{tmp}"; \
  ExternalSize: 25_635_768; Flags: external download ignoreversion deleteafterinstall; Tasks: voz; Check: FaltaVc

[Icons]
Name: "{autoprograms}\Jarvis"; Filename: "{app}\jarvis.exe"; WorkingDir: "{app}"; Comment: "Jarvis, o teu assistente"; \
  AppUserModelID: "jarvis.desktop"
Name: "{autodesktop}\Jarvis"; Filename: "{app}\jarvis.exe"; WorkingDir: "{app}"; Comment: "Jarvis, o teu assistente"; \
  AppUserModelID: "jarvis.desktop"; Tasks: ambiente

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "jarvis.desktop"; \
  ValueData: """{app}\jarvis.exe"" --hidden"; Flags: uninsdeletevalue; Tasks: arranque

[Run]
Filename: "{tmp}\vc_redist.x64.exe"; Parameters: "/install /passive /norestart"; StatusMsg: "A instalar o Microsoft Visual C++ (para a voz)…"; \
  Flags: shellexec waituntilterminated; Tasks: voz; Check: FaltaVc
Filename: "{app}\jarvis.exe"; WorkingDir: "{app}"; Description: "Abrir o Jarvis"; Flags: nowait postinstall skipifsilent
; A atualização feita pelo próprio Jarvis (update.rs) corre o setup em silêncio com /relancar=1: abre-o no fim.
Filename: "{app}\jarvis.exe"; WorkingDir: "{app}"; Flags: nowait; Check: Relancar

[Code]
const
  Itens = 4;

var
  Requisitos: TWizardPage;
  Links: array[0..Itens - 1] of String;

function Relancar: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:relancar|0}') = '1');
end;

// Um ficheiro da voz que ainda não está em %LOCALAPPDATA%\Jarvis\vendor (o que já lá está não volta a descer).
function Falta(Relativo: String): Boolean;
begin
  Result := not FileExists(ExpandConstant('{localappdata}\Jarvis\vendor\') + Relativo);
end;

// As DLL do Visual C++ que o Whisper e a Piper usam; o vcomp140 só vem com o Redistributable.
function FaltaVc: Boolean;
var
  Sys: String;
begin
  Sys := ExpandConstant('{sys}\');
  Result := not (FileExists(Sys + 'msvcp140.dll') and FileExists(Sys + 'vcruntime140.dll')
    and FileExists(Sys + 'vcruntime140_1.dll') and FileExists(Sys + 'vcomp140.dll'));
end;

function NoPath(Nome: String): Boolean;
begin
  Result := FileSearch(Nome, GetEnv('PATH')) <> '';
end;

// Os mesmos sítios onde o Jarvis procura cada coisa (native/src/brain.rs).
function TemGit: Boolean;
begin
  Result := NoPath('git.exe') or FileExists(ExpandConstant('{commonpf64}\Git\cmd\git.exe'))
    or FileExists(ExpandConstant('{localappdata}\Programs\Git\cmd\git.exe'));
end;

function TemClaude: Boolean;
begin
  Result := FileExists(GetEnv('USERPROFILE') + '\.local\bin\claude.exe') or NoPath('claude.exe') or NoPath('claude.cmd');
end;

function TemAgy: Boolean;
begin
  Result := FileExists(GetEnv('USERPROFILE') + '\.agy\bin\agy.exe') or FileExists(ExpandConstant('{localappdata}\agy\bin\agy.exe'));
end;

function TemCodex: Boolean;
var
  Codigo: Integer;
begin
  Result := DirExists(ExpandConstant('{userappdata}\npm\node_modules\@openai\codex'));
  if not Result then
    Result := Exec('powershell.exe', '-NoProfile -NonInteractive -Command "if (Get-AppxPackage OpenAI.Codex) { exit 0 } else { exit 1 }"',
      '', SW_HIDE, ewWaitUntilTerminated, Codigo) and (Codigo = 0);
end;

procedure AbrirLink(Sender: TObject);
var
  Codigo: Integer;
begin
  ShellExec('open', Links[TComponent(Sender).Tag], '', '', SW_SHOWNORMAL, ewNoWait, Codigo);
end;

// Uma linha por requisito: OK, ou o que falta com o comando (selecionável, para copiar) e um botão para o link.
procedure Linha(Indice: Integer; var Y: Integer; Tem: Boolean; Nome, Para, Comando, Link: String);
var
  Titulo, Texto: TNewStaticText;
  Caixa: TNewEdit;
  Botao: TNewButton;
begin
  Links[Indice] := Link;
  Titulo := TNewStaticText.Create(Requisitos);
  Titulo.Parent := Requisitos.Surface;
  Titulo.Top := Y;
  Titulo.Font.Style := [fsBold];
  if Tem then begin
    Titulo.Caption := 'OK   ' + Nome;
    Titulo.Font.Color := $00308030;
    Y := Y + ScaleY(22);
    exit;
  end;
  Titulo.Caption := 'FALTA   ' + Nome + ': ' + Para;
  Titulo.Font.Color := $000070C0;
  Y := Y + ScaleY(18);

  Caixa := TNewEdit.Create(Requisitos);
  Caixa.Parent := Requisitos.Surface;
  Caixa.Top := Y;
  Caixa.Left := ScaleX(14);
  Caixa.Width := Requisitos.SurfaceWidth - ScaleX(14) - ScaleX(110);
  Caixa.ReadOnly := True;
  Caixa.Text := Comando;

  Botao := TNewButton.Create(Requisitos);
  Botao.Parent := Requisitos.Surface;
  Botao.Top := Y - ScaleY(1);
  Botao.Width := ScaleX(100);
  Botao.Height := Caixa.Height + ScaleY(2);
  Botao.Left := Requisitos.SurfaceWidth - Botao.Width;
  Botao.Caption := 'Como instalar';
  Botao.Tag := Indice;
  Botao.OnClick := @AbrirLink;
  Y := Y + Caixa.Height + ScaleY(10);
end;

procedure PreencherRequisitos;
var
  Y: Integer;
  Git, Claude, Agy, Codex: Boolean;
  Nota: TNewStaticText;
begin
  Git := TemGit;
  Claude := TemClaude;
  Agy := TemAgy;
  Codex := TemCodex;
  Y := 0;
  Linha(0, Y, Git, 'Git', 'pacotes de equipa, repositórios e worktrees',
    'winget install Git.Git', 'https://git-scm.com/downloads/win');
  Linha(1, Y, Claude, 'Claude Code', 'motor de IA, com a subscrição Claude Pro ou Max',
    'irm https://claude.ai/install.ps1 | iex', 'https://code.claude.com/docs/en/setup');
  Linha(2, Y, Agy, 'Antigravity', 'motor de IA, com a conta Google',
    'irm https://antigravity.google/cli/install.ps1 | iex', 'https://antigravity.google');
  Linha(3, Y, Codex, 'Codex', 'motor de IA, com a conta ChatGPT',
    'npm i -g @openai/codex   (ou a app Codex)', 'https://developers.openai.com/codex');

  Nota := TNewStaticText.Create(Requisitos);
  Nota.Parent := Requisitos.Surface;
  Nota.Top := Y + ScaleY(4);
  Nota.Width := Requisitos.SurfaceWidth;
  Nota.WordWrap := True;
  Nota.AutoSize := True;
  if not (Claude or Agy or Codex) then
    Nota.Caption := 'Precisas de pelo menos um motor de IA para as conversas. Os comandos correm-se no PowerShell; ' +
      'depois abre o motor uma vez para fazeres login com a tua conta (nunca com chave de API).'
  else
    Nota.Caption := 'Os comandos correm-se no PowerShell. Os motores que faltam são opcionais: a conversa passa para eles ' +
      'quando o plano de outro acaba. Cada motor usa a tua conta, nunca chaves de API.';
  if not Git then
    Nota.Caption := Nota.Caption + #13#10#13#10 + 'Sem o git, o Jarvis não traz o pacote da tua equipa nem prepara worktrees.';
end;

procedure InitializeWizard;
begin
  Requisitos := CreateCustomPage(wpInfoAfter, 'O que falta neste PC',
    'O Jarvis está instalado. Para as conversas e o board precisa disto, cada um com a tua conta.');
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = Requisitos.ID) and (Requisitos.Surface.ControlCount = 0) then
    PreencherRequisitos;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    Log(Format('Requisitos: git=%d claude=%d agy=%d codex=%d', [Ord(TemGit), Ord(TemClaude), Ord(TemAgy), Ord(TemCodex)]));
end;

// Fecha o Jarvis que estiver aberto (o Windows não deixa substituir o exe enquanto corre).
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Codigo: Integer;
begin
  Exec('taskkill.exe', '/IM jarvis.exe /F', '', SW_HIDE, ewWaitUntilTerminated, Codigo);
  Sleep(500);
  Result := '';
end;

function InitializeUninstall: Boolean;
var
  Codigo: Integer;
begin
  Exec('taskkill.exe', '/IM jarvis.exe /F', '', SW_HIDE, ewWaitUntilTerminated, Codigo);
  Result := True;
end;
