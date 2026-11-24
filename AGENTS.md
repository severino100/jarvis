# Instruções para agentes

Lê **[CONTRIBUTING.md](CONTRIBUTING.md)** antes de commitar ou abrir um PR. O essencial:

- Commits e títulos de PR em **Conventional Commits**, em português de Portugal: `feat`, `fix`, `perf`, `refactor`,
  `chore`, `docs`, `ci`, `test`, `style`, `build` (`!` ou `BREAKING CHANGE` para major). Exemplo: `fix(prs): caminho do repositório`.
- `feat` sobe a minor; `fix`/`perf`/`refactor` sobem a patch; os restantes não geram release.
- Trabalha num branch `<tipo>/<descrição-curta>`; nunca faças push direto para o `main`; tudo entra por PR.
- O release é automático ao integrar o PR: **não** mudes a versão em `native/Cargo.toml`, não cries tags e não uses `--no-verify`.
- Só commitas, fazes push ou abres PRs quando a pessoa pedir.
- Código em `native/` (Rust, Windows): `cargo build --release --locked` e `cargo test --locked`.
