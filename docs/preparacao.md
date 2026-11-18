# Preparar a implementação (Jarvis, guia genérico)

O pacote da equipa pode trazer o seu próprio guia; este vale quando não traz. Lê isto só quando {nome} aprovar a implementação ("sim", "avança", "implementa"). Faz por esta ordem:

1. **Tarefas no board**, se a skill do board o permitir, com as estimativas do plano.
2. **Worktree por repositório tocado**, a partir de `origin/{base}`, com o branch `{ramo_bug}` (Bug) ou `{ramo}` (o resto). A pasta do worktree segue `{worktree}` dentro de `{projetos}`. Nunca mudes de branch nem alteres ficheiros na pasta principal de um repositório.
3. **Pacote de passagem** no worktree principal:
   - imagens do item em `.jarvis/img/`;
   - `.jarvis/HANDOFF.md`, que tem de bastar a outra IA sem esta conversa: título com o id e o link do item; problema e critérios de aceitação; imagens com uma frase sobre cada uma; causa com caminhos e linhas; plano por repositório e ficheiros a mudar; tarefas criadas; branch e worktree(s); como testar (o que arrancar, onde ir, passos e resultado esperado); as regras do repositório e que não há commit nem push sem {nome} pedir.
   - O Jarvis põe `.jarvis/` no exclude do git: nunca entra em commits.
4. Termina a resposta com a linha `[[IMPLEMENTAR:<caminho absoluto do worktree>]]` sozinha. O Jarvis pergunta a {nome} se implementa no Claude Code ou no Antigravity.
