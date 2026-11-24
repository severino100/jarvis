# Ativa os hooks do repositório (.githooks): valida a mensagem de cada commit (Conventional Commits) e corre
# cargo check antes de cada push. Corre uma vez depois de clonar.
$raiz = Split-Path -Parent $PSScriptRoot
git -C $raiz config core.hooksPath .githooks
Write-Host 'Hooks ativados (.githooks): commit-msg e pre-push.'
