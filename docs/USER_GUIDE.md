# Guia do usuário

## Primeiro uso

1. Abra o FORJA e escolha **Criar novo projeto**, **Abrir pasta existente** ou **Clonar repositório**.
2. Abra **Configurações → Modelos e provedores**.
3. Cadastre um provedor, descubra os modelos e crie um perfil com janela de contexto e capacidades verificadas.
4. Escolha explicitamente o executor. Revisores são opcionais.
5. Crie uma conversa e descreva o objetivo.

O FORJA não escolhe automaticamente um provedor local ou remoto. Um perfil sem limite de contexto conhecido permanece inativo até receber um valor manual.

## Projetos e conversas

Cada conversa pertence a um único projeto. Ao abrir um workspace, a IDE lista somente suas conversas e reabre a mais recente válida. **Nova conversa** preserva as anteriores. A área global de tarefas pode mostrar vários projetos, mas abre primeiro o workspace correto.

No navegador de desenvolvimento, o seletor web não fornece ao daemon um caminho absoluto confiável; o FORJA apresenta o campo de caminho manual. No desktop Tauri, os três pontos de entrada usam o seletor nativo.

## Modos

- **Consulta:** leitura e resposta, sem mutações.
- **Planejamento:** análise, perguntas interativas e plano estruturado. Escritas normais e comandos mutáveis continuam bloqueados.
- **Construir:** aplicação do plano com aprovações, checkpoints e validação.
- **Revisão:** análise do trabalho e riscos, sem conceder permissões adicionais.

A troca de modo cria uma nova execução dentro da mesma conversa. Histórico, respostas, plano e resumo continuam disponíveis, mas permissões são reavaliadas para a nova execução.

## Planejar e implementar

No Planejamento, o modelo pode apresentar escolhas de seleção única, múltipla ou texto. Uma pergunta pendente é persistida e restaurada após reinício.

Quando o plano termina, o FORJA cria um documento canônico e uma cópia Markdown em `docs/forja-plans/`. O card mostra revisão, etapas, riscos e critérios de aceite. **Implementar Plano**:

1. valida a revisão e o executor;
2. captura o estado inicial do Git e dos arquivos relevantes;
3. cria o primeiro checkpoint operacional;
4. muda para Construir na mesma conversa;
5. seleciona a primeira etapa pronta.

A aba **Progresso** mostra concluído, em andamento, pendente, arquivos, validações, bloqueios e próxima ação. O arquivo `.progress.md` é uma projeção legível; o banco e os eventos permanecem a fonte canônica.

## Chat técnico

Markdown, tabelas e blocos de código são renderizados de forma sanitizada. HTML do modelo não é executado. Blocos de código podem ser copiados e abertos no editor quando possuem uma referência válida. Ferramentas, terminal, patches, busca, arquivos e screenshots têm cards próprios.

Uma proposta de patch não altera o arquivo. **Aplicar ao arquivo** confere workspace e hash-base, cria checkpoint e usa a autorização da sessão. Se o arquivo mudou, a proposta entra em conflito e nenhuma versão é sobrescrita.

## Modelos, reasoning e revisores

O gerenciador separa provedores de perfis. Em cada perfil confira:

- origem da janela de contexto e do limite de saída;
- texto, visão, ferramentas, saída estruturada e reasoning;
- níveis de reasoning aceitos pelo adaptador;
- estado habilitado e revisão.

O controle de reasoning aparece apenas quando o perfil declara suporte. O FORJA salva o nível efetivo da execução, mas não exibe nem persiste cadeia privada de pensamento.

Há exatamente um executor e até três revisores. Somente o executor recebe ferramentas. Revisores produzem parecer consultivo, podem falhar independentemente e não autorizam ações.

## Contexto

O medidor mostra uso, limite, porcentagem e origem da contagem. Abra-o para ver sistema, ferramentas, histórico, anexos, plano e checkpoint. A compactação automática começa em aproximadamente 75% da entrada útil e tenta voltar a 55%. A partir de 90%, uma nova chamada é suspensa até compactar, remover anexos ou escolher uma janela maior; histórico e ações de recuperação continuam acessíveis.

Use `@caminho/arquivo` para incluir contexto do projeto. A indexação FTS5 e os símbolos Tree-sitter ajudam a busca. LSP de TypeScript/JavaScript e Python exige autorização e os servidores instalados pelo workspace de desenvolvimento.

## Agentes

Um agente pertence ao projeto e define papel, instruções, modelo, reasoning, ferramentas, escrita e budgets. O orquestrador também pode propor/criar perfis dentro das capacidades que já possui. Agentes escritores usam worktrees em repositórios Git; projetos sem Git serializam escrita. Conflitos precisam ser revistos antes da integração.

## Busca web e Computer Use

Cadastre Brave, SearXNG ou REST declarativo em **Configurações**. Credenciais vão para o cofre do sistema. Endpoints locais exigem escolha explícita.

Uma tarefa de navegador exige origens autorizadas. O worker cria uma sessão efêmera, registra snapshot acessível, console, rede e screenshots, e bloqueia downloads. Cookies não são compartilhados entre tarefas. Uploads, digitação de credenciais, pagamentos e publicação não fazem parte do grant amplo atual.

## Skills, hooks e MCP

Skills são lidas progressivamente e por hash; scripts são apenas inspecionados. Hooks executam comandos nativos após aprovação individual e interrompem o fluxo em falha, timeout ou negação. Servidores MCP precisam de conexão revisada e cada chamada exige autorização própria. Consulte [SKILLS.md](SKILLS.md) e [HOOKS.md](HOOKS.md).

## Offline, backup e recuperação

O modo offline bloqueia inferência e executores cuja rede não pode ser contida, incluindo terminal nativo, hooks, MCP, LSP e navegador. Ele não transforma um backend não verificado em local.

O backup cria uma pasta nova com banco, blobs e manifesto. Ele não inclui credenciais, `daemon.json` nem todos os arquivos do projeto. Verifique antes de restaurar. Para ativar uma restauração, encerre app e daemon e inicie com `FORJA_DATA_DIR` apontando para o destino restaurado.

## Solução de problemas

| Sintoma | Ação |
| --- | --- |
| Provedor retorna credencial inválida | Edite a chave; ela não aparece em logs ou banco |
| Ollama/LM Studio indisponível | Inicie o servidor e confirme o endpoint loopback |
| Perfil não habilita | Defina uma janela de contexto maior que zero |
| Patch em conflito | Reabra o arquivo, revise a mudança do usuário e gere uma nova proposta |
| Contexto acima de 90% | Use **Compactar agora**, reduza anexos ou troque o executor |
| Pergunta/plano pendente após reinício | Reabra a conversa; o estado é restaurado do daemon |
| Daemon de desenvolvimento não compila no Windows | Encerre o processo existente, pois o executável pode estar bloqueado |
| Browser worker não inicia | Instale dependências e Chromium do Playwright ou configure `FORJA_NODE`/`FORJA_CHROMIUM` |
