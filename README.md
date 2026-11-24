# Jarvis

Assistente de desktop para Windows para equipas de desenvolvimento: o teu board, as tuas conversas com a IA sobre cada item, a agenda, os PRs e a voz, numa interface HUD nativa (Rust, sem web).

O Jarvis não traz nada de nenhuma empresa. Cada equipa descreve o seu board, os seus repositórios, as suas skills e as suas regras num **pacote de equipa** (um repositório git privado), e cada pessoa usa as suas próprias contas.

## O que faz

- **Hoje**: o trabalho que tens e o que está livre para pegar, sprint, horas, agenda do dia, PRs, música e o que mudou no board.
- **Conversas por item**: escolhes o motor (Claude Code, Antigravity ou Codex) e o modelo; a IA lê o item, encontra a causa no código e propõe o plano. Quando aprovas, prepara as tarefas, o worktree e um pacote de passagem; a implementação abre numa sessão nova do Claude Code, no Codex ou noutra IA.
- **Voz**: Ctrl+Alt+J, com o Whisper (ouvir) e a Piper (falar), no PC.
- **Agenda** do Outlook, **PRs** do Azure Repos com comentários e "Corrigir", **tarefas** pessoais e do Notion, **limpeza** de worktrees, **horas** do sprint.

## Só subscrições, nunca pagamento por utilização

O Jarvis usa o Claude Code, o Antigravity e o Codex com a conta e a subscrição de quem o tem instalado, e recusa tudo o que custe dinheiro por utilização:

- nenhum motor recebe chaves de API nem credenciais de cloud (`ANTHROPIC_*`, `OPENAI_*`, `GEMINI_API_KEY`, `AWS_*`, Vertex…);
- o Claude Code só corre com o login da subscrição (Pro/Max): se estiver ligado por chave de API, ou se o limite do plano acabar e fosse continuar em utilização extra paga, o Jarvis para a resposta e diz porquê;
- o Codex só corre com o login ChatGPT (`codex login status`, `forced_login_method="chatgpt"`), nunca com chave de API;
- os custos em USD que os CLIs estimam não aparecem: numa subscrição não são dinheiro gasto;
- quando o plano de um motor chega ao limite, a conversa continua no motor seguinte que tenhas instalado (Claude Code → Antigravity → Codex), com o que já foi dito, e fica lá até mudares (Definições → Motor das conversas novas).

Compras feitas fora do Jarvis (créditos extra no ChatGPT ou na conta Google) dependem das definições de cada conta: deixa-as desligadas.

## Do que precisas

- Windows 10/11.
- Pelo menos um motor, com a tua conta: [Claude Code](https://claude.com/claude-code), o Antigravity CLI (`agy`, conta Google) ou o Codex (app Codex ou `npm i -g @openai/codex`, conta ChatGPT).
- Git com acesso aos repositórios da tua equipa.
- Para o board: Azure DevOps e um token pessoal (Work Items Read & Write, Code Read).

## Instalar

Descarrega o **[Jarvis-Setup.exe](https://github.com/severino100/jarvis/releases/latest/download/Jarvis-Setup.exe)** e abre-o. Não precisa de administrador. Se o Windows avisar ("O Windows protegeu o computador"), carrega em **Mais informações → Executar mesmo assim**: o instalador ainda não tem assinatura digital.

O instalador põe o Jarvis em `%LOCALAPPDATA%\Programs\Jarvis`, descarrega a voz (Whisper e Piper com a voz pt-PT, cerca de 555 MB, das fontes oficiais, com verificação do conteúdo) para `%LOCALAPPDATA%\Jarvis\vendor` se a escolheres, cria os atalhos e liga o arranque com o Windows. No fim mostra o que falta no PC (git, Claude Code, Antigravity, Codex), com o comando e o link para instalar cada um. Atualiza-se sozinho: ao arrancar, de hora a hora e quando abres a janela vê se há um release novo e, se houver, aparece uma caixa no fundo do menu; carregas em **Instalar** e o Jarvis fecha, instala a versão nova e volta a abrir, com as conversas e as definições como estavam (a voz já descarregada não volta a descer). Desinstala-se em Definições do Windows → Aplicações.

Alternativa sem o instalador, numa janela do PowerShell (`-SemVoz` salta a voz; `-Verificar` só diz o que falta):

```powershell
irm https://github.com/severino100/jarvis/releases/latest/download/install.ps1 | iex
```

Depois, no Jarvis, **Definições → Perfil e equipa**:

1. O teu nome e a pasta onde estão os teus projetos.
2. **Adicionar do git** com o link do pacote da tua equipa (o git usa a tua conta).
3. O teu token do board (fica cifrado no PC).

Sem pacote, o Jarvis funciona sem board: conversas, agenda, tarefas, música e voz.

## Pacotes de equipa

Um pacote é uma pasta com um `jarvis.toml` (board, repositórios, convenção de branches e worktrees), as skills da equipa (o pacote é também um plugin do Claude Code) e guias (regras para a IA, como preparar uma implementação, como testar no browser). Toda a equipa usa as mesmas skills: o Jarvis faz `git pull` dos pacotes ao arrancar. O formato está em [native/README.md](native/README.md#perfil-e-pacotes-de-equipa).

## Desenvolvimento

O código está em [`native/`](native/) e a documentação técnica em [native/README.md](native/README.md). As regras da conversa estão em [`docs/`](docs/). Para compilar: Rust estável e Visual Studio Build Tools 2022 com C++; `cargo build --release` em `native/` e `scripts\install-native.ps1` instala o que compilaste no mesmo sítio que o instalador.

Convenções de commits, branches, PRs e releases (para pessoas e agentes): [CONTRIBUTING.md](CONTRIBUTING.md).

Cada push e pull request para o `main` compila, corre os testes e compila o instalador (Actions → build). Releases: uma tag `v*` (ou "Run workflow" em Actions → release) compila no Windows e publica o `Jarvis-Setup.exe`, o `jarvis.exe` e o `install.ps1` num release do GitHub; é daí que os Jarvis instalados se atualizam.
