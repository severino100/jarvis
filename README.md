# Jarvis

Assistente de desktop para Windows para equipas de desenvolvimento: o teu board, as tuas conversas com a IA sobre cada item, a agenda, os PRs e a voz, numa interface HUD nativa (Rust, sem web).

O Jarvis não traz nada de nenhuma empresa. Cada equipa descreve o seu board, os seus repositórios, as suas skills e as suas regras num **pacote de equipa** (um repositório git privado), e cada pessoa usa as suas próprias contas.

## O que faz

- **Hoje**: o trabalho que tens e o que está livre para pegar, sprint, horas, agenda do dia, PRs, música e o que mudou no board.
- **Conversas por item**: escolhes o motor (Claude Code ou Antigravity) e o modelo; a IA lê o item, encontra a causa no código e propõe o plano. Quando aprovas, prepara as tarefas, o worktree e um pacote de passagem; a implementação abre numa sessão nova do Claude Code ou noutra IA.
- **Voz**: Ctrl+Alt+J, com o Whisper (ouvir) e a Piper (falar), no PC.
- **Agenda** do Outlook, **PRs** do Azure Repos com comentários e "Corrigir", **tarefas** pessoais e do Notion, **limpeza** de worktrees, **horas** do sprint.

## Do que precisas

- Windows 10/11.
- [Claude Code](https://claude.com/claude-code) com a tua conta e/ou o Antigravity CLI (`agy`) com a tua conta Google.
- Git com acesso aos repositórios da tua equipa.
- Para o board: Azure DevOps e um token pessoal (Work Items Read & Write, Code Read).
- Para compilar: Rust estável e Visual Studio Build Tools 2022 com C++.
- Para a voz: o Whisper e a Piper em `vendor/` (não vêm no repositório).

## Começar

```powershell
cd native
cargo build --release
powershell -ExecutionPolicy Bypass -File ..\scripts\install-native.ps1
```

Depois, no Jarvis, **Definições → Perfil e equipa**:

1. O teu nome e a pasta onde estão os teus projetos.
2. **Adicionar do git** com o link do pacote da tua equipa (o git usa a tua conta).
3. O teu token do board (fica cifrado no PC).

Sem pacote, o Jarvis funciona sem board: conversas, agenda, tarefas, música e voz.

## Pacotes de equipa

Um pacote é uma pasta com um `jarvis.toml` (board, repositórios, convenção de branches e worktrees), as skills da equipa (o pacote é também um plugin do Claude Code) e guias (regras para a IA, como preparar uma implementação, como testar no browser). Toda a equipa usa as mesmas skills: o Jarvis faz `git pull` dos pacotes ao arrancar. O formato está em [native/README.md](native/README.md#perfil-e-pacotes-de-equipa).

## Desenvolvimento

O código está em [`native/`](native/) e a documentação técnica em [native/README.md](native/README.md). As regras da conversa estão em [`docs/`](docs/).
