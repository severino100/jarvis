# Testar no browser (guia genérico)

Guia para a IA que implementa (Antigravity, Claude Code ou outra). O Jarvis copia-o para `.jarvis/TESTAR.md` no worktree; o pacote da equipa pode trazer um mais específico. O "Como testar" do `.jarvis/HANDOFF.md` diz o que verificar neste item; este guia diz como.

1. **Antes do browser:** compila no worktree. Com erros, corrige primeiro. Vê que portas já estão ocupadas: as que já estão a correr são de {nome}, não lhes toques.
2. **Arrancar:** só o que o item precisa, no worktree e numa porta livre. Não pares servidores que já estavam a correr; se for preciso trocar um, pergunta a {nome}.
3. **Entrar:** nunca escrevas utilizador nem palavra-passe. Se aparecer um login, para e pede a {nome} que entre na janela do browser.
4. **Verificar:** cada critério de aceitação, o caminho feliz e pelo menos um caso de erro; a consola do browser sem erros novos; a rede sem respostas 4xx/5xx inesperadas; o que já funcionava à volta continua a funcionar.
5. **Provas e limpeza:** capturas em `.jarvis/test/` e o resultado em `.jarvis/test/RESULTADO.md` (o que passou, o que falhou, os dados que criaste). No fim, para os servidores que arrancaste. Nunca digas que está testado o que não abriste no browser.
