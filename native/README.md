# Jarvis — código (Rust)

Interface nativa em Rust com **egui/eframe 0.37** (motor OpenGL `glow`), sem web.

```bash
cargo run                     # debug (escreve na consola)
cargo build --release         # target\release\jarvis.exe (~16 MB)
JARVIS_SHOT_RS=<ficheiro.png> JARVIS_SHOT_VIEW=today|now|review|refined|hours|chat|agenda|tasks|prs|cleanup|settings target\release\jarvis.exe   # captura e sai
JARVIS_SHOT_DELAY=9 ...       # espera mais antes da captura (a agenda e os PRs demoram uns segundos)
JARVIS_SHOT_VIEW=chat JARVIS_SHOT_WINDOW=1 ...   # vista Chat com uma conversa de exemplo, também numa janela à parte
JARVIS_SHOT_VOICE=1 ...       # abre o painel da voz com uma troca de exemplo (sem microfone)
JARVIS_SHOT_VIEW=chat-rh ...  # uma conversa de RH de exemplo (com "Guardei" e "Desfeito"), num ficheiro temporário
JARVIS_SHOT_VIEW=eu ...       # Definições → O meu Jarvis
JARVIS_SHOT_MODES=rh ...      # finge estes modos (rh, dev ou dev,rh) sem mexer no perfil guardado
JARVIS_SHOT_NO_LOGIN=1 ...    # mostra os motores instalados como sem sessão iniciada (com Iniciar sessão)
JARVIS_SHOT_EDIT=1 ...        # abre os rascunhos de email em edição
JARVIS_DATA_DIR=<pasta> JARVIS_SHOT_VIEW=welcome JARVIS_SHOT_STEP=0..5 ...   # o primeiro arranque, num perfil descartável
target\debug\jarvis.exe --profile                    # o perfil, o pacote ativo, branches, worktrees e a persona preenchida
target\debug\jarvis.exe --dump                       # lê o board uma vez e escreve um resumo
target\debug\jarvis.exe --ask claude|gemini|codex "texto" [modelo]  # uma volta de conversa, sem janela (JARVIS_ASK_MODE=dev|rh|geral; grava os [[LEMBRAR:]])
target\debug\jarvis.exe --voice-test "texto"         # a Piper diz o texto para um WAV e o Whisper lê-o (nada toca)
target\debug\jarvis.exe --mic-test                   # abre o microfone 1 s e diz o que ouviu
target\debug\jarvis.exe --update-test                # lê o último release e descarrega o setup, sem o correr
JARVIS_UPDATE_FROM=0.1.0 ...  # finge ser essa versão e estar instalado (mostra a caixa da versão nova)
```

As opções `--…` escrevem na consola só na compilação de debug (a de release não tem consola).

Requisitos: Rust estável (`rustup`) e Visual Studio Build Tools 2022 com C++ (o `build.rs` embute o ícone no executável).

O `jarvis.exe` não precisa de nada ao lado: os guias genéricos (`docs/preparacao.md`, `docs/testar-no-browser.md`) e o `scripts/calendar.ps1` entram no executável e são escritos em `%APPDATA%\Jarvis\recursos` ao arrancar (`settings::resources_dir`). A voz é lida de `%LOCALAPPDATA%\Jarvis\vendor` (posta lá pelo instalador) ou, num PC com o código, de `../vendor/` (`settings::vendor_dir`; `JARVIS_VENDOR` sobrepõe-se).

## Dados (em `%APPDATA%\Jarvis`)

| Ficheiro | O quê |
| --- | --- |
| `perfil.json` | nome, modos (`dev`, `rh`), blocos escondidos do Hoje, pasta dos projetos, pacotes de equipa e o token do board cifrado com a DPAPI do Windows |
| `eu\` | o que a pessoa ensina ao Jarvis (ver abaixo) |
| `packs\` | os pacotes de equipa clonados do git |
| `state-native.json` | definições e o que já foi visto no board e nos PRs |
| `chats-native.json` | conversas |
| `tasks.json` | tarefas pessoais |
| `notion-native.json` | páginas do Notion e o token cifrado com a DPAPI |
| `recursos\` | guias e scripts que vêm dentro do executável |
| `planos\` | os planos de preparação que o Antigravity passa ao `apply` da skill |

A voz fica em `%LOCALAPPDATA%\Jarvis\vendor` (local, para os ~600 MB não irem no perfil móvel do Windows).

As regras da conversa estão em `../docs/persona*.md` (entram no executável com `include_str!`; ao mudá-las, recompila): `persona.md` e `persona-gemini.md` para o modo dev, `persona-rh.md` e `persona-rh-gemini.md` para o modo RH, `persona-geral.md` para a conversa livre.

## Modos

`Profile.modes` diz para que a pessoa usa o Jarvis: `dev` (o board, os PRs, as horas e a limpeza) e/ou `rh` (o assistente pessoal de RH). Um perfil antigo sem o campo conta como `["dev"]`. Sem o modo dev, o menu não mostra Agora, PRs, Revisão, Refined, Horas nem Limpeza, a barra lateral não mostra o sprint e o Hoje não espera pelo board (`app::shown`, `dashboard::personal`). Os blocos do Hoje que cada pessoa desliga ficam em `Profile.today_hidden` (chaves de `dashboard::BLOCKS`) e os ecrãs que tira do menu em `Profile.menu_hidden` (chaves de `app::menu_entries`; Hoje e Definições ficam sempre).

Cada conversa tem um modo (`Chat.mode`): as de item e de PR são dev; as outras são rh quando o modo RH está ligado (`profile::default_mode`), e com os dois modos escolhe-se por cima da caixa de escrita (muda para uma sessão nova, porque as regras entram no início da sessão). No modo rh o motor arranca em `eu\`, pode ler a pasta da pessoa (`%USERPROFILE%`) em vez dos projetos, o Claude não tem `Bash`, e as regras da equipa vêm de `[guides] persona_rh` do pacote (ou de `persona`, para quem só tem o modo RH).

## A pasta "eu" (`%APPDATA%\Jarvis\eu`)

O que cada pessoa ensina ao seu Jarvis, só neste PC (`eu.rs`):

| Ficheiro | O quê |
| --- | --- |
| `instrucoes.md` | instruções pessoais escritas pela pessoa (o tom, como assina, o que nunca fazer) |
| `memoria.md` | uma regra por linha, com a data: `- 2026-11-18: assina os emails como Rita` |
| `exemplos.md` | exemplos de estilo: a versão final dos rascunhos de email que a pessoa editou antes de os levar (só os 5 mais recentes) |
| `skills\` | skills pessoais do Claude Code; a pasta é um plugin (`.claude-plugin\plugin.json`) e entra com `--plugin-dir`, como o pacote ativo (comandos `/eu:<skill>`) |

As instruções e a memória juntam-se a todas as conversas (dev, rh e geral) depois das regras do Jarvis e da equipa, com a indicação explícita de que essas ficam por cima: a confirmação antes de enviar, criar ou apagar e as regras de dados pessoais não se desligam (`eu::prompt_block`). No Claude vão no `--append-system-prompt` de cada volta; no Antigravity e no Codex em cada volta, para uma regra nova contar logo na seguinte. Tudo se vê e muda em Definições → O meu Jarvis.

**`[[LEMBRAR: <regra>]]`**: quando a pessoa diz "lembra-te que…" ou corrige uma preferência, a IA termina a resposta com esta linha (as personas explicam-no), como já faz com `[[IMPLEMENTAR:…]]` e `[[ABRIR_NO_CLAUDE]]`. O `chat.rs` não a mostra (nem enquanto a resposta chega), grava a regra em `memoria.md` (a mesma regra só uma vez) e mostra na conversa "Guardei: <regra>" com "Desfazer". Teste: `jarvis.exe --ask claude "lembra-te que assino sempre como Rita"` e depois outra pergunta.

**`[[SKILL: nome | quando usar]] … [[/SKILL]]`**: quando a pessoa pede um atalho ("a partir de agora, quando eu disser *triagem*, faz…"), a IA escreve as instruções neste bloco. O Jarvis grava-as em `eu\skills\<nome>\SKILL.md` (o comando `/eu:<nome>`; `eu::save_skill`), esconde o bloco e mostra "Criei o teu comando" com "Desfazer" (apaga o comando, ou volta à versão anterior se já existia). Os atalhos entram também em texto nas instruções, porque o Antigravity e o Codex não carregam skills do Claude Code.

**Exemplos de estilo**: o cartão de um rascunho `[[EMAIL]]` tem "Editar". Quando uma versão editada sai (Abrir no Outlook ou Copiar), o corpo final vai para `exemplos.md` (`eu::add_example`) e as conversas seguintes recebem-no como exemplo de tom, comprimento e assinatura, com a indicação de nunca reutilizar os nomes nem os dados. Definições → O meu Jarvis mostra-os, com Apagar. Testes: `cargo test` (em `eu.rs`).

## Primeiro arranque

Sem `perfil.json`, o Jarvis abre um assistente curto (`welcome.rs`): o nome, para que o usa (RH, dev ou os dois), a conta de IA (verifica se o Claude Code, o Antigravity e o Codex estão instalados, com o comando e o link para o que falta, `requisitos.rs`, e se cada motor tem a conta iniciada, sem gastar utilização: `claude auth status`, `codex login status` e o login Google guardado do Antigravity (`brain::signed_in`). Um motor sem sessão aparece a âmbar com "Iniciar sessão", que abre um terminal com o comando de login), o pacote da equipa (opcional), os blocos do Hoje e uma primeira instrução pessoal. "Saltar" fica com o que já foi escolhido.

## Instalar

Tudo fica em `%LOCALAPPDATA%\Programs\Jarvis` (o `jarvis.exe`), a voz em `%LOCALAPPDATA%\Jarvis\vendor` e os dados em `%APPDATA%\Jarvis`, seja qual for o caminho de instalação.

**`Jarvis-Setup.exe`** (o que se passa às pessoas): `installer/jarvis.iss`, Inno Setup 6.5 ou mais recente. Instala só para o utilizador (sem administrador); as tarefas opcionais são a voz (descarregada das fontes oficiais com o hash verificado, só o que ainda falta), o arranque com o Windows e o atalho no Ambiente de Trabalho; o atalho do menu Iniciar leva o AppUserModelID `jarvis.desktop`. No fim, uma página mostra o que falta no PC (git, Claude Code, Antigravity, Codex) com o comando para copiar e o link. Desinstala-se em Definições do Windows → Aplicações. Compilar: `ISCC.exe installer\jarvis.iss` (usa `native\target\release\jarvis.exe`; `/DExePath=<exe>` para outro); a versão do setup vem do `ProductVersion` do exe. Com `/relancar=1` (e `/VERYSILENT`), abre o Jarvis no fim: é assim que a atualização automática o corre.

**`install.ps1`**, alternativa sem o setup: `irm https://github.com/severino100/jarvis/releases/latest/download/install.ps1 | iex` (ver o README principal). O ficheiro é só ASCII (o PowerShell 5.1 lê-o mal de outra forma e um BOM parte o `[scriptblock]::Create`); os acentos do que mostra escrevem-se com marcadores (`{a~}`, `{c,}`…) que a função `Pt` troca. `-Verificar` só diz o que falta. `scripts/install.ps1` descarrega o `jarvis.exe` do release para `%LOCALAPPDATA%\Programs\Jarvis`, a voz das fontes oficiais (whisper.cpp `b5454`, `ggml-small.bin`, Piper `2023.11.14-2`, voz `pt_PT-tugão-medium` v1.0.0) para `%LOCALAPPDATA%\Jarvis\vendor` (`-SemVoz` salta-a), cria os atalhos "Jarvis" (menu Iniciar e Ambiente de Trabalho) com o AppUserModelID das notificações (`jarvis.desktop`) e põe o arranque com o Windows (`HKCU\...\Run`, com `--hidden`). `-Versao v0.2.0` instala um release concreto; `-Exe <ficheiro>` instala um exe local.

Quem compila, depois de `cargo build --release`, num terminal normal:

```powershell
powershell -ExecutionPolicy Bypass -File ..\scripts\install-native.ps1   # instala o que compilaste, no mesmo sítio, sem descarregar a voz
powershell -ExecutionPolicy Bypass -File ..\scripts\check-native.ps1     # vê o estado
```

### Release

`.github/workflows/release.yml` corre numa tag `v*` (ou à mão, em Actions → release → Run workflow, com a tag): compila no Windows, compila o setup e publica um release do GitHub com três ficheiros: `Jarvis-Setup.exe`, `jarvis.exe` (o que o `install.ps1` descarrega) e `install.ps1`. Antes da tag, sobe a `version` no `Cargo.toml`: é ela que o setup e a atualização comparam.

`.github/workflows/build.yml` corre em cada push e pull request para o `main`: `cargo build --release --locked`, `cargo test --locked` e a compilação do setup, para apanhar erros no `.iss`. Não publica nada.

### Atualização automática

`update.rs`: só o Jarvis instalado (o exe em `%LOCALAPPDATA%\Programs\Jarvis`) se atualiza; um compilado a partir do código não. Um minuto depois de arrancar, de hora a hora e quando a janela vem para a frente (se a última vez foi há mais de 15 minutos), lê o último release (`api.github.com/repos/severino100/jarvis/releases/latest`); se for mais novo do que a `version` do `Cargo.toml`, aparece uma caixa no fundo do menu lateral (e em Definições), com uma notificação uma vez por versão. **Instalar** descarrega o `Jarvis-Setup.exe` desse release para a pasta temporária e corre-o com `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /relancar=1`: o Jarvis fecha, o setup instala por cima (com as opções escolhidas da primeira vez) e volta a abri-lo. As conversas e as definições ficam.

Testar:

```powershell
$env:JARVIS_UPDATE_FROM='0.1.0'; target\debug\jarvis.exe    # finge ser a 0.1.0 e estar instalado: verifica ao fim de 2 s e mostra a caixa
target\debug\jarvis.exe --update-test                        # lê o último release e descarrega o setup, sem o correr
```

## Perfil e pacotes de equipa

Nada de uma empresa está no código do Jarvis. Há três camadas:

| Camada | Onde | O quê |
| --- | --- | --- |
| Jarvis | este repositório | a app, a persona genérica (`docs/persona*.md`) e os guias genéricos de preparação e de testes |
| Pacote de equipa | uma pasta, normalmente um repositório git privado da equipa | `jarvis.toml` (board, repositórios, branches, worktrees, guias), as skills (o pacote é um plugin do Claude Code) e os guias da equipa |
| Perfil | `%APPDATA%\Jarvis\perfil.json` | nome, pasta dos projetos, pacotes instalados e ativo, token do board cifrado com DPAPI |

Definições → Perfil e equipa: adicionar um pacote do git (clonado com a conta git de cada pessoa para `%APPDATA%\Jarvis\packs`) ou de uma pasta, escolher o ativo, guardar o token. Ao arrancar, o Jarvis faz `git pull --ff-only` de cada pacote com remote: quem muda uma skill no pacote muda-a para a equipa toda.

O que o pacote ativo decide (`profile.rs`): a organização, o projeto, a área e as colunas do board (`azdo.rs`); a pasta onde o Claude arranca e os repositórios lidos (`main_dir`, `repos`); o branch de cada item (`branch_for`, com `{id}`, `{sprint}`, `{kind}`); os worktrees (`{repo}-{id}`) que a limpeza conhece e que a preparação no Antigravity pode criar (`prep_commands`); a skill do board (o comando `/<skill>` e o script que o Antigravity pode correr); e os guias, juntados à persona. As personas e os guias aceitam `{nome}`, `{projetos}`, `{principal}`, `{repos}`, `{base}`, `{ramo_bug}`, `{ramo}`, `{worktree}`, `{fluxo}` e `{guia_preparacao}`.

O Claude Code e o Antigravity recebem `AZDO_ORG`, `AZDO_PROJECT`, `AZDO_AREA`, `AZDO_PAT` e `JARVIS_PROJECTS` no ambiente, para as skills do pacote usarem o board e o token de quem está a usar o Jarvis. Sem token guardado, o Jarvis usa o ficheiro `~/.config/azdo/pat` (ou `AZDO_PAT_FILE`).

Um `jarvis.toml` mínimo:

```toml
name = "A minha equipa"
skill = "skills/board"            # opcional: a skill do board, também o comando /board
skill_cli = "skills/board/azdo.mjs"

[board]
kind = "azure-devops"
org = 'https://dev.azure.com/<organização>'
project = '<projeto>'
area = '<projeto>\<equipa>'       # opcional

[branches]
base = "main"
bug = "fix/{id}"
other = "feature/{id}"
worktree = "{repo}-{id}"

[guides]                          # opcionais, caminhos dentro do pacote
persona = "guias/persona.md"
prepare = "guias/preparacao.md"
test = "guias/testar-no-browser.md"

[[repo]]
side = "APP"
dir = "o-meu-front"
label = "front"
main = true
link_node_modules = true

[[repo]]
side = "API"
dir = "o-meu-back"
label = "back"
branch_only = true                # opcional: sem worktree, o item é um branch na pasta principal
```

`branch_only = true` serve para um repositório cuja pasta principal é o que corre (por exemplo um back em Docker Compose, onde um worktree nunca chega aos containers). Na preparação pelo Antigravity, esse repositório não recebe o `git worktree add`: só os comandos `fetch`, `status --porcelain`, `branch --show-current` e `branch <ramo> origin/<base>` (criar o branch sem mudar para ele). Quem implementa muda para o branch, com a pasta limpa. A limpeza (`cleanup.rs`) continua a listar worktrees antigos desse repositório pelo padrão `[branches] worktree`; deixa de haver novos.

## Tokens

Medido com a mesma pergunta (`--ask`), o contexto fixo que o Claude Code relê em cada chamada de uma resposta passou de 34,5 mil para 18,2 mil tokens (uma resposta com uma ferramenta: 69 mil → 37 mil). O `brain.rs` arranca o Claude com:

- `--strict-mcp-config`: sem os servidores MCP da conta, que o Jarvis não usa;
- `--tools Read,Grep,Glob,Edit,Write,Bash,Skill,TodoWrite`: sem as outras ferramentas (web, notebooks, agentes…);
- `--setting-sources project,local` + `--plugin-dir <pacote ativo>`: sem plugins, skills e hooks de utilizador; as skills da equipa vêm do pacote, que é um plugin do Claude Code;
- `--settings {"effortLevel":"medium"}`.

Conversas longas: cada mensagem relê a conversa inteira. O Jarvis mostra o contexto no cabeçalho (medido nas respostas do Claude, estimado no Antigravity) e, a partir de 80 mil tokens (`CONTEXT_WARN` no `chat.rs`), oferece "Continuar numa sessão nova": o motor escreve um resumo, as sessões dos dois motores são largadas e o resumo segue com a mensagem seguinte. O histórico continua visível. Teste: `jarvis.exe --compact-test` (conversa pequena no Haiku).

O resto que já poupa: sem subagentes, o Jarvis lê o item antes (a primeira volta não gasta uma chamada no `show`), respostas rápidas de voz sem o Claude, e o Gemini (subscrição Google) quando o Claude fica sem utilização.

## Ecrã principal e som

`dashboard.rs` é o ecrã Hoje: relógio, núcleo com três anéis (sprint decorrido, horas feitas/estimadas, dia de trabalho 9h–18h), foco agora com a linha do tempo de hoje, sistema (`sysmon.rs`: CPU com um minuto de histórico, memória, disco C:), conversas a trabalhar, PRs, revisão, tarefas, música e o fio de atividade (as novidades do board). Três colunas acima de 1100 pt de largura, duas acima de 700, uma abaixo. Os blocos entram em sequência ao abrir o ecrã.

`sound.rs`: som de arranque original, sintetizado, ou um ficheiro do utilizador (Definições → Som ao arrancar). `jarvis.exe --sound-wav <ficheiro>` grava o som original num WAV sem o tocar; uma cópia está em `../assets/arranque.wav`.

`music.rs`: o que está a tocar no Windows (Spotify, Apple Music ou outro), pelos controlos de multimédia do sistema; sem contas nem login.

## Codex

`brain.rs` (`ask_codex`) corre `codex exec --json` com o pedido pela entrada padrão (com o item, o histórico e as regras passa do limite de 32 mil caracteres da linha de comandos do Windows) e retoma a conversa com `codex exec resume <thread>`. Usa o CLI da app Codex quando está instalada (`Get-AppxPackage OpenAI.Codex`, atualizado pela app) e, se não, o do npm. Corre na sandbox `workspace-write` com escrita só na pasta dos projetos e rede ligada, com `windows.sandbox="unelevated"` (a sandbox elevada precisa de uma consola e, com a consola escondida do Jarvis, os comandos nunca voltavam) e `notify=[]` (o aviso da app Codex não corre a cada resposta). Os modelos vêm de `codex debug models`; numa conta ChatGPT nem todos os da API são aceites. Implementar no Codex abre o Codex interativo numa consola própria, no worktree, já com o pedido.

## Antigravity e comandos

As regras de permissão do `agy` só aceitam comandos exatos (`command(git status)` não deixa passar `git status --short`; não há prefixos) ou tudo (`command(*)`). Por isso, numa conversa de item, o `brain.rs` (`prep_commands`) escreve por extenso os comandos da preparação desse item (o `show`/`apply` da skill do pacote com o plano em `../plans/<id>.json`, `git fetch`, `git worktree add -b <branch>` em cada repositório do pacote, `mklink` do node_modules) e só esses correm. Commit, push, reset ou checkout continuam bloqueados. Teste: `jarvis.exe --prep-test <id>` (só corre o `show`).

## Notas

- Ctrl+Alt+J só funciona quando nenhuma outra app tem o atalho. No menu da bandeja há "Falar com o Jarvis".
- As notificações usam o AppUserModelID `jarvis.desktop`, registado pelo atalho do menu Iniciar; clicar abre a conversa certa.
- Quando o Jarvis é arrancado a partir de uma app empacotada (MSIX, por exemplo a app do Claude), o Windows guarda o `%APPDATA%` dele no contentor dessa app: para uso real, arranca-o pelo menu Iniciar ou por um atalho.
- Os `with(...)` (board, PRs, conversas, voz) seguram um `Mutex` enquanto se desenha: nada dentro deles pode voltar a trancar o mesmo estado. `Board::refresh` e `Prs::refresh` só trancam na sua thread, por isso podem ser chamados de qualquer lado; as outras ações devolvem-se ao `app.rs` e correm depois do `with`.
- Armadilhas do egui já resolvidas: `set_max_width` também alarga (usar `available_width().min(…)`); etiquetas com `truncate()` dentro de `Grid` precisam de largura explícita; o `glow` sem fontes do sistema não tem glifos como ◆ (os ícones são desenhados).
- Com o `wgpu` gastava ~250 MB; o `glow` fica em ~140 MB, a maior parte do driver da placa gráfica.
