# Para quem compila o Jarvis: instala o native\target\release\jarvis.exe que acabaste de compilar, no mesmo sítio e da
# mesma forma que o instalador dos colegas (scripts\install.ps1), sem descarregar a voz (numa cópia do código ela é
# lida de vendor\ desta pasta).
# Corre-o num terminal normal, não a partir de processos dentro da app do Claude (as escritas em AppData e no registo
# ficam virtualizadas no contentor dela).
param([switch]$NoStart)
$ErrorActionPreference = 'Stop'
$jarvisDir = Split-Path $PSScriptRoot -Parent
$built = Join-Path $jarvisDir 'native\target\release\jarvis.exe'
if (-not (Test-Path $built)) { throw "Falta compilar: cargo build --release em $jarvisDir\native" }

& (Join-Path $PSScriptRoot 'install.ps1') -Exe $built -SemVoz

# Versões antigas instalavam em %USERPROFILE%\.jarvis: o exe de lá deixa de ser usado.
$old = Join-Path $env:USERPROFILE '.jarvis'
if (Test-Path (Join-Path $old 'jarvis.exe')) {
  Remove-Item (Join-Path $old 'jarvis.exe') -Force -ErrorAction SilentlyContinue
  Write-Host "Removido o exe antigo de $old"
}
if ($NoStart) { Get-Process jarvis -ErrorAction SilentlyContinue | Stop-Process -Force }
