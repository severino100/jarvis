# Contribuir para o Jarvis

Este documento é para pessoas **e para agentes de IA** (Claude Code, Codex, Antigravity…). As regras abaixo são
verificadas por hooks locais e pelo CI: se não as seguires, o commit ou o PR falha.

## Fluxo em resumo

1. Parte do `main` atualizado e cria um branch (`<tipo>/<descrição-curta>`).
2. Faz commits pequenos, em Conventional Commits.
3. Abre um PR para o `main` com **título em Conventional Commits** (é o título que decide a versão).
4. Quando o PR é integrado, o release é **automático**. Nunca subas a versão nem cries tags à mão.

## Preparação (uma vez depois de clonar)

```powershell
scripts\setup-hooks.ps1
```

Ativa os hooks de `.githooks/`: `commit-msg` (valida a mensagem) e `pre-push` (corre `cargo check --locked`).
Não os contornes com `--no-verify`.

## Mensagens de commit: Conventional Commits

```
<tipo>(<âmbito opcional>): <descrição>

<corpo opcional: o porquê, não o quê>

<rodapé opcional>
```

| Tipo | Quando usar | Efeito no release |
|---|---|---|
| `feat` | funcionalidade nova para quem usa o Jarvis | sobe a **minor** (0.2.x → 0.3.0) |
| `fix` | correção de um erro | sobe a **patch** (0.2.4 → 0.2.5) |
| `perf` | melhoria de desempenho | **patch** |
| `refactor` | reestruturar código sem mudar o comportamento | **patch** |
| `chore` | manutenção (dependências, versões, limpeza) | sem release |
| `docs` | só documentação | sem release |
| `ci` | workflows, hooks, scripts de build | sem release |
| `test` | só testes | sem release |
| `style` | formatação, sem mudar lógica | sem release |
| `build` | sistema de build, instalador | sem release |

- **Quebra de compatibilidade** (major, 0.x → 1.0.0 ou 1.x → 2.0.0): põe `!` antes de `:` (`feat!: muda o formato do
  perfil`) ou um rodapé `BREAKING CHANGE: <o que quebra>`.
- **Âmbito** (opcional) é a área tocada, em minúsculas: `feat(voz): …`, `fix(prs): …`.
- **Descrição**: em **português de Portugal**, no imperativo ou descritivo, minúscula inicial, **sem ponto final**,
  título com no máximo 100 caracteres. Descreve o efeito para quem usa, não os ficheiros editados.
- Um commit, uma mudança lógica. Não juntes uma funcionalidade e uma correção não relacionada.
- `Merge …`, `Revert …`, `fixup!` e `squash!` são aceites sem validação.
- Exemplos reais do histórico: `feat(voz): escolher o microfone nas Definições`,
  `feat: botão em cada cartão e layout de dashboard nos separadores de pacote`.

Exemplos inválidos (o hook rejeita): `Atualiza coisas`, `feat adiciona botão`, `Feat: Botão.`, `fix:` (sem descrição).

## Branches

`<tipo>/<descrição-curta-em-kebab-case>`, com o mesmo tipo do commit principal: `feat/botao-no-cartao`,
`fix/caminho-do-repositorio`, `ci/release-automatico`, `docs/contributing`. Nunca faças push direto para o `main`:
tudo entra por PR.

## Pull requests

- **O título do PR segue as mesmas regras dos commits** (o workflow `pr-title` falha se não seguir). É ele que decide
  a versão: por exemplo, um PR `feat: …` gera uma release minor.
- Escolhe o tipo pelo efeito do PR no seu todo. Se juntas uma `feat` e várias `fix`, o título é `feat: …`.
- Descreve o **quê**, o **porquê**, o que foi **testado** e o que ficou **de fora**.
- O CI (`build`) compila, corre os testes e compila o instalador; o `commitlint` valida cada commit do PR. Tudo tem de
  passar antes de integrar.
- Os PRs integram-se com merge commit.

## Versões e releases (automáticos)

- A versão vive em `native/Cargo.toml` (e `native/Cargo.lock`) e segue [SemVer](https://semver.org/lang/pt-BR/).
- Quando um PR é integrado no `main`, o workflow `release` lê o título do PR, calcula a nova versão (tabela acima),
  atualiza os dois ficheiros, compila, faz o commit `chore: versão X.Y.Z` com a tag `vX.Y.Z` e publica o release no
  GitHub com o `Jarvis-Setup.exe`, o `jarvis.exe` e o `install.ps1`. Os Jarvis instalados atualizam-se a partir daí.
- **Não** edites a versão, não cries tags `v*` e não faças commits `chore: versão …` à mão: o workflow ignora esse
  prefixo de propósito para não entrar em ciclo.
- Tipos sem release (`chore`, `docs`, `ci`, `test`, `style`, `build`) integram-se no `main` sem versão nova.
- Para forçar uma versão excecional: Actions → release → "Run workflow" (major, minor ou patch).

## Código

O código está em `native/` (Rust, Windows). Para compilar e testar:

```powershell
cd native
cargo build --release --locked
cargo test --locked
```

Mais detalhes técnicos em [native/README.md](native/README.md). Mantém o `Cargo.lock` consistente (`--locked`).

## Para agentes de IA: regras que não podes falhar

1. Antes de commitar, confirma o branch (`git branch --show-current`); se estiveres no `main`, cria um branch primeiro.
2. Escreve **todas** as mensagens de commit e títulos de PR em Conventional Commits, em português de Portugal, como
   acima. Escolhe o tipo certo: `feat`/`fix`/`perf`/`refactor` geram release, o resto não.
3. Não corras `git push` para o `main`, não uses `--no-verify`, não reescrevas o histórico partilhado.
4. Não alteres a versão, não crias tags, não executes o workflow de release: é automático.
5. Só commitas, fazes push ou abres PRs quando a pessoa pedir.
6. Se um hook rejeitar o commit, corrige a mensagem; não contornes o hook.
7. Termina cada commit com a linha de atribuição que o teu ambiente exigir (por exemplo `Co-Authored-By: …`), numa
   linha própria depois de uma linha em branco.
