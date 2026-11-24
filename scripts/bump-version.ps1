# Decide e aplica a subida de versão (SemVer) a partir do título de um PR / commit em Conventional Commits.
#   feat!: ... ou "BREAKING CHANGE" -> major | feat: -> minor | fix/perf/refactor: -> patch
#   chore/docs/ci/test/style/build:  -> nenhuma versão (não há release)
# Escreve native/Cargo.toml e native/Cargo.lock (o Cargo.lock é atualizado pelo cargo) e imprime a nova versão,
# ou nada se não houver subida. -Forcar major|minor|patch ignora o título. -SoCalcular não escreve nada.
param(
    [string]$Titulo = '',
    [string]$Corpo = '',
    [ValidateSet('', 'major', 'minor', 'patch')][string]$Forcar = '',
    [switch]$SoCalcular
)
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
$toml = Join-Path $raiz 'native\Cargo.toml'

$tipo = $Forcar
if (-not $tipo) {
    if ($Titulo -match '^\s*\w+(\([^)]*\))?!:' -or $Corpo -match '(?m)^BREAKING[ -]CHANGE') { $tipo = 'major' }
    elseif ($Titulo -match '^\s*feat(\([^)]*\))?:') { $tipo = 'minor' }
    elseif ($Titulo -match '^\s*(fix|perf|refactor)(\([^)]*\))?:') { $tipo = 'patch' }
}
if (-not $tipo) { return }

$texto = [IO.File]::ReadAllText($toml)
if ($texto -notmatch '(?m)^version = "(\d+)\.(\d+)\.(\d+)"') { throw 'Não encontrei a versão em native/Cargo.toml.' }
$v = [int[]]@($Matches[1], $Matches[2], $Matches[3])
switch ($tipo) {
    'major' { $v = @(($v[0] + 1), 0, 0) }
    'minor' { $v = @($v[0], ($v[1] + 1), 0) }
    'patch' { $v = @($v[0], $v[1], ($v[2] + 1)) }
}
$nova = $v -join '.'
if ($SoCalcular) { return $nova }

$re = [regex]'(?m)^version = "\d+\.\d+\.\d+"'
[IO.File]::WriteAllText($toml, $re.Replace($texto, "version = `"$nova`"", 1))
Push-Location (Join-Path $raiz 'native')
try {
    # O cargo escreve o progresso no stderr; isso não é um erro.
    $ErrorActionPreference = 'Continue'
    cargo update --workspace --offline 2>&1 | Out-Null
    if ($LASTEXITCODE) { cargo update --workspace 2>&1 | Out-Null }
    if ($LASTEXITCODE) { throw 'cargo update falhou.' }
} finally { Pop-Location }
$nova
