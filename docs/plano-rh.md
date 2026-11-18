# Jarvis para Recursos Humanos — plano

Estado: proposta. Nada disto está implementado.

## Objetivo

Levar o Jarvis à equipa de RH com a mesma lógica que já tem para o desenvolvimento: um ecrã **Hoje** com o que há para fazer, conversas com a IA sobre cada item e o estado do que foi pedido. Aqui o item deixa de ser um work item e passa a ser uma **vaga** ou um **candidato**.

1. **Ver o que há para fazer; se não houver, criar o que foi pedido.** Um pedido de recrutamento chega (normalmente por email). O Jarvis diz se já existe uma vaga para ele ou propõe criá-la, com a descrição escrita pela IA.
2. **Ver o estado do que fizemos.** Por exemplo: "dos 14 candidatos contactados ontem, 5 responderam".
3. **Um sítio para tudo:** vagas abertas, quem foi contactado, respostas, entrevistas e estatísticas.

## Princípio: o assistente de cada uma

O Jarvis de RH não é um programa de recrutamento com um processo fixo. É **o assistente pessoal de cada pessoa de RH**, o "ChatGPT dela", que já sabe o que é uma vaga, um candidato e um contacto. Cada pessoa usa-o como preferir e vai-o ensinando.

Consequências para o desenho:

- **A conversa vem primeiro.** Tudo o que os ecrãs fazem também se pede por escrito ou por voz: "quantos responderam ontem?", "escreve um InMail para este perfil", "cria a vaga do pedido da Ana". Os ecrãs (Vagas, Contactos, Estatísticas) são atalhos, não o único caminho.
- **As vagas, os candidatos e os contactos são ferramentas da IA.** A IA lê e escreve os dados de recrutamento (com um comando do próprio Jarvis, por exemplo `jarvis.exe --rh ...`, ou uma skill), por isso pode responder a qualquer pergunta sobre eles e registar o que foi feito na conversa.
- **Cada pessoa ensina o seu Jarvis.** Não é treino de modelo: é memória e instruções que a IA lê em todas as conversas, guardadas no PC de cada pessoa (`%APPDATA%\Jarvis\eu\`):
  - **Instruções pessoais** (`instrucoes.md`): o tom, como assina, o que nunca fazer. Editáveis num ecrã simples.
  - **Memória:** quando a pessoa diz "lembra-te que…" ou corrige a IA, a IA guarda a regra e mostra-a ("Guardei: os InMails para devs são curtos e sem emojis"). A pessoa pode ver, mudar e apagar tudo o que está guardado.
  - **Exemplos de estilo:** quando a pessoa edita um rascunho antes de enviar, a versão final fica como exemplo para as próximas mensagens.
  - **Modelos e atalhos pessoais:** "a partir de agora, quando eu disser *triagem*, avalia os perfis colados contra a vaga aberta e faz uma tabela". A IA cria a skill pessoal, que passa a ser um comando dela.
- **O pacote da equipa dá o ponto de partida, a pessoa ajusta.** O pacote de RH traz modelos, fases e regras comuns (RGPD, tom da empresa). As instruções pessoais de cada uma acrescentam-se por cima. As regras de RGPD e de "nada sai sem confirmação" não se podem desligar.
- **Cada pessoa escolhe o que vê.** O que aparece no Hoje (respostas, vagas, agenda, tarefas, música) e que ecrãs ficam no menu é escolha de cada uma.
- **Também serve para o resto do trabalho.** Escrever um email difícil, resumir uma reunião, preparar uma avaliação de desempenho, rever uma política interna: é um assistente geral, com o recrutamento como especialidade.

### O que isto pede a mais

- **Uma persona geral de RH** (`docs/persona-rh.md`), diferente da de dev: assistente pessoal, não "só análise e preparação"; propõe e escreve, mas não envia nem apaga sem confirmação.
- **Instalação sem compilar.** A equipa de RH não vai correr `cargo build`. É preciso um instalador pronto (o `.exe` compilado e um instalador que também instala e liga o Claude Code), e um primeiro arranque guiado: nome, contas (IA, email), pacote da equipa, e o que quer ver no Hoje.
- **Conta de IA de cada pessoa.** O Jarvis usa o Claude Code (conta Claude) ou o Antigravity (conta Google) de quem o usa. Cada pessoa de RH precisa de uma, e é preciso decidir o plano e quem paga.

## Ponto de partida

- **Não há ATS** (*Applicant Tracking System*: o programa de RH onde ficam vagas e candidatos). Hoje as vagas vivem em Excel, em emails e no LinkedIn Recruiter. O Jarvis passa a fazer esse papel, numa versão simples.
- **Há licença de LinkedIn Recruiter.**
- **Contactos por LinkedIn (InMail) e por email.**
- **Dados locais em cada PC**, para já.
- **Utilizadores:** a equipa de RH. O processo é partilhado; os dados, para já, não.

## Regras que o desenho tem de respeitar

### LinkedIn

O LinkedIn não permite automatizar a pesquisa nem o envio de mensagens: nada de scraping, robôs ou extensões que cliquem por nós (é contra as regras deles e dá bloqueio da conta). Também não há API de mensagens fora dos parceiros oficiais. Por isso:

- **A pesquisa e os InMails são feitos por uma pessoa, no Recruiter.** O Jarvis prepara as pesquisas booleanas e as mensagens personalizadas, e a pessoa copia-as.
- **Os perfis entram no Jarvis por uma pessoa:** link e texto colado, ou o PDF que o Recruiter deixa descarregar.
- **As respostas detetam-se pelo Outlook:** o Recruiter manda um email quando um InMail tem resposta, e o Jarvis lê esses emails. As respostas a contactos por email lêem-se diretamente.
- A sincronização automática do Recruiter só existe com ATS parceiros do LinkedIn (o "Recruiter System Connect"). Fica para mais tarde, se a empresa adotar um.

### RGPD

Para cada candidato regista-se de onde veio (fonte e data), quando foi informado do tratamento dos dados (no primeiro contacto) e até quando os dados podem ficar guardados. O Jarvis avisa antes do prazo e apaga quando a pessoa de RH confirma. A pessoa pode também pedir para ser apagada: um botão "Apagar candidato" remove tudo dela.

### A IA propõe, a pessoa decide

Como no modo dev, nada é enviado, criado ou apagado sem um "sim" claro de quem usa o Jarvis. A IA escreve rascunhos (descrições de vaga, mensagens, pesquisas, avaliações); quem envia e decide é a pessoa.

## Ligações de cada pessoa (email e LinkedIn)

Cada pessoa de RH liga as **suas** contas em Definições → Contas, como hoje cada pessoa guarda o seu token do board. As credenciais ficam cifradas no PC (DPAPI), e nada é partilhado.

### Email (Microsoft 365)

Há dois caminhos:

| | Outlook clássico (COM) | Microsoft Graph (início de sessão Microsoft) |
| --- | --- | --- |
| Como liga | Nada a fazer: usa o Outlook aberto no PC, como a agenda já faz | Botão "Ligar conta Microsoft", que abre o login da empresa |
| Precisa de | Outlook clássico instalado e aberto | Uma aplicação registada no Entra ID (Azure AD) da empresa, feita uma vez pela IT, com as permissões `Mail.Read`, `Mail.Send` e `Calendars.Read` |
| Funciona com o Outlook novo / web | Não | Sim |
| Robustez | Depende do Outlook estar aberto | Funciona sempre, mesmo com o Outlook fechado |

Proposta: começar pelo COM (já está provado na agenda e não precisa da IT) e passar ao Graph quando a IT registar a aplicação. Se a equipa de RH já usa o Outlook novo, ir direto ao Graph.

O que a ligação ao email faz: ler pedidos de recrutamento, ler respostas de candidatos, ler os avisos do LinkedIn sobre respostas a InMails, e enviar emails a candidatos depois de a pessoa confirmar.

### LinkedIn Recruiter

O LinkedIn não dá a apps como o Jarvis acesso à pesquisa, aos perfis nem às mensagens do Recruiter. A única integração oficial (Recruiter System Connect) é reservada aos ATS parceiros do LinkedIn. "Iniciar sessão com o LinkedIn" só dá o nome e o email de quem entra. Automatizar o browser é contra as regras do LinkedIn e põe em risco a licença.

Por isso, "ligar o LinkedIn" no Jarvis quer dizer:

- **Respostas a InMails pelo email:** o LinkedIn avisa por email quando um candidato responde. Cada pessoa tem de confirmar que estes avisos estão ligados nas definições do LinkedIn, e o Jarvis lê-os pela ligação ao email.
- **Abrir no Recruiter com um clique:** o Jarvis copia a pesquisa booleana ou a mensagem e abre o Recruiter (ou o perfil do candidato) no browser. A pessoa só tem de colar.
- **Trazer perfis para o Jarvis:** colar o texto do perfil, ou arrastar o PDF que o Recruiter deixa descarregar.

A ligação direta só chega se a empresa adotar um ATS parceiro do LinkedIn (Teamtailor, Workable, Greenhouse e outros). Aí o Recruiter sincroniza com o ATS, e o Jarvis lê do ATS.

## Fluxo

```
pedido → vaga → descrição da vaga → pesquisa → avaliação → contacto → seguimento → entrevista → decisão
```

| Passo | O que o Jarvis faz | O que a pessoa faz |
| --- | --- | --- |
| Pedido | Deteta emails de pedido de recrutamento; procura uma vaga parecida já aberta | Confirma "é nova" ou "é esta" |
| Vaga | Escreve a descrição (requisitos, faixa salarial, linguagem inclusiva) e aponta requisitos irrealistas | Revê e aprova |
| Pesquisa | Gera pesquisas booleanas para o Recruiter e os critérios de avaliação | Corre a pesquisa no Recruiter |
| Avaliação | Lê o perfil importado, dá uma nota de 1 a 5 e explica-a (o que encaixa, o que falta, riscos); avisa se já foi contactado | Decide quem contactar |
| Contacto | Escreve a mensagem personalizada (InMail ou email), com variantes A/B | Envia (o email pode sair pelo Outlook com um clique de confirmação) |
| Seguimento | Lembra aos 3 e aos 7 dias quem não respondeu, com o texto pronto | Envia ou dispensa |
| Entrevista | Propõe horários a partir da agenda; depois, ficha de avaliação a partir de notas ditadas por voz | Entrevista e dita as notas |
| Decisão | Resumo para quem pediu a vaga | Decide |

## Ecrãs

| Ecrã | O que mostra |
| --- | --- |
| **Hoje (RH)** | Pedidos sem vaga; respostas novas; seguimentos para hoje; entrevistas do dia; a linha "ontem: 14 contactados, 5 responderam (36%)" |
| **Vagas** | Vagas abertas com o funil (contactados → responderam → interessados → entrevista → proposta → contratado), dias em aberto e quem pediu |
| **Vaga** | Descrição, candidatos por fase e uma conversa com a IA sobre a vaga ("mais 10 perfis como este", "porque é que esta vaga não tem respostas?") |
| **Candidato** | Perfil, nota e razões, contactos e respostas, entrevistas, notas, RGPD |
| **Contactos** | Quem foi contactado, quando, por que canal, com que mensagem e o estado; filtros por dia, vaga e canal |
| **Estatísticas** | Taxa de resposta por vaga, por canal e por mensagem; tempo até à resposta; conversão entre fases; tempo até contratar; de onde vêm os contratados |

## Dados (locais)

Ficheiro `%APPDATA%\Jarvis\recrutamento.json`, gravado como os outros (`tasks.json`, `chats-native.json`).

```
Vaga        id, título, equipa, pedida_por, pedido (id do email), descrição, critérios, faixa salarial,
            estado (aberta / pausada / fechada), aberta_em, fechada_em, pesquisas[]
Candidato   id, nome, cargo atual, empresa, local, linkedin, email, perfil (texto), fonte, importado_em,
            rgpd { informado_em, guardar_até, apagar_pedido }
Candidatura vaga, candidato, fase, nota, razões, notas[], atualizada_em
Contacto    candidatura, canal (inmail / email), enviado_em, mensagem, variante,
            respondeu_em, resposta (resumo), interesse (sim / não / talvez)
Entrevista  candidatura, quando, com quem, ficha, decisão
Pedido      id do email, de, assunto, recebido_em, vaga (se já ligado), estado (novo / ligado / ignorado)
```

As estatísticas calculam-se a partir de `Contacto` e de `Candidatura`; não se guardam.

**Duplicados:** o candidato identifica-se pelo link do LinkedIn ou pelo email. Antes de contactar, o Jarvis avisa se a pessoa já foi contactada nos últimos 6 meses (prazo configurável).

**Limite de serem dados locais:** cada pessoa só vê os seus contactos, e duas pessoas podem contactar o mesmo candidato sem saber. O passo seguinte barato é pôr o ficheiro numa pasta partilhada da equipa (OneDrive ou SharePoint), com cuidado com gravações ao mesmo tempo. Nunca no git do pacote, por serem dados pessoais.

## Importar o que existe hoje

- **Excel:** importar as vagas e os candidatos que já estão em folhas de cálculo. Na primeira importação a pessoa escolhe que coluna é o quê; o mapeamento fica guardado. Crate `calamine` para ler `.xlsx`.
- **Emails antigos:** procurar no Outlook pedidos e respostas dos últimos 30 dias para começar com histórico.

## Como encaixa no código

| O quê | Onde |
| --- | --- |
| Vagas, candidatos, contactos e estatísticas (dados e regras) | `recruit.rs` (novo), no estilo de `tasks.rs` |
| Ecrãs de RH | `recruit_view.rs` (novo); novas entradas em `View` no `app.rs` |
| Ligações de cada pessoa | `accounts.rs` (novo): email (COM ou Graph) com o token cifrado com DPAPI, como o token do board em `profile.rs`; ecrã em `settings_view.rs` |
| Ler emails (pedidos, respostas, avisos do Recruiter) | `inbox.rs` (novo), no estilo de `agenda.rs`: `scripts/mail.ps1` por COM primeiro, Microsoft Graph depois |
| Enviar emails | pela mesma ligação, só depois de confirmação na interface |
| Abrir no Recruiter | copia o texto e abre o browser no Recruiter ou no perfil |
| Bloco de RH no ecrã Hoje | `dashboard.rs` |
| Conversa por vaga | `chat.rs` e `brain.rs`: a conversa recebe a vaga e os candidatos, como hoje recebe o item do board |
| Regras da IA para RH | `docs/persona-rh.md` (novo) |
| Processo da equipa | secção `[recruiting]` no `jarvis.toml` do pacote de RH (`profile.rs`) |
| Ligar o modo | perfil: `modes = ["dev", "rh"]`; o menu e o Hoje mostram o que estiver ligado |

### `[recruiting]` no `jarvis.toml`

```toml
name = "RH"

[recruiting]
stages = ["Contactado", "Respondeu", "Interessado", "Entrevista", "Proposta", "Contratado"]
followups = [3, 7]                 # dias sem resposta até lembrar
recontact_months = 6               # aviso de "já contactado"
retention_months = 24              # RGPD: prazo para guardar
request_keywords = ["recrutamento", "nova vaga", "contratar", "headcount"]
linkedin_reply_senders = ["linkedin.com"]

[recruiting.templates]             # modelos de mensagem, caminhos dentro do pacote
inmail = "modelos/inmail.md"
email = "modelos/email.md"
followup = "modelos/seguimento.md"
privacy = "modelos/aviso-rgpd.md"

[guides]
persona = "guias/persona-rh.md"
```

As skills do pacote de RH (avaliar perfil, escrever descrição de vaga, gerar pesquisa booleana) ficam no pacote, como as do board ficam no pacote de dev. Toda a equipa usa as mesmas.

## Fases

0. **O assistente pessoal.** Instalador sem compilar e primeiro arranque guiado; persona de RH; conversa geral; instruções pessoais, memória ("lembra-te que…") com ecrã para ver e apagar; escolher o que aparece no Hoje e no menu.
1. **Vagas, pedidos e ligações.** Ecrã Contas (email e LinkedIn); modelo de dados e gravação local; ecrã Vagas (criar, editar, fechar); `mail.ps1` a ler pedidos; "já há vaga?" e rascunho da descrição; importação de vagas do Excel; persona de RH.
2. **Candidatos e pesquisa.** Pesquisa booleana gerada; importar perfil (texto, link ou PDF); nota explicada; aviso de duplicados; ecrã Candidato.
3. **Contactos e respostas.** Mensagens personalizadas; registo de contactos; deteção de respostas (notificações do Recruiter e emails diretos); seguimentos; ecrã Contactos. No fim desta fase já se responde a "dos de ontem, quantos responderam".
4. **Hoje de RH e estatísticas.** Bloco no Hoje; ecrã Estatísticas; resumo de voz de manhã.
5. **Entrevistas.** Horários a partir da agenda; ficha por voz; resumo para quem pediu.
6. **Afinar.** Testes A/B de mensagens; alertas ("vaga sem respostas há 5 dias"); relatório semanal; recuperar candidatos antigos para vagas novas.

## Outras ideias

- **Rever a descrição da vaga:** requisitos a mais, linguagem que afasta candidatos, falta de faixa salarial (a diretiva europeia de transparência salarial vai torná-la obrigatória).
- **Recuperar candidatos antigos:** numa vaga nova, procurar primeiro nos candidatos que ficaram em segundo lugar.
- **Referências internas:** pedir indicações à equipa, com uma mensagem por vaga.
- **Vagas técnicas:** a IA usa os repositórios da equipa de desenvolvimento para propor um exercício técnico realista.
- **Resumo de voz de manhã:** "Tens 3 respostas novas, 2 entrevistas, e a vaga de Frontend está parada há 5 dias."

## Em aberto

- Que conta de IA tem cada pessoa de RH (Claude ou Google) e quem paga.

- Se a equipa de RH usa o Outlook clássico ou o novo (decide entre COM e Graph), e quem na IT pode registar a aplicação no Entra ID.
- Formato dos pedidos de recrutamento hoje (email livre, formulário, Teams?): decide como os detetamos.
- Que colunas têm os Excel atuais: decide a importação.
- Quem pode ver o quê quando os dados passarem a ser partilhados.
- Adotar um ATS mais tarde (Teamtailor, Workable, Recruitee): o Jarvis passaria a ler e a escrever nele em vez do ficheiro local, e o Recruiter sincronizaria com ele.
