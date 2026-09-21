# Hooks locais

Esta entrega implementa comandos nativos em seis momentos do ciclo do agente. É uma parte do sistema de hooks do plano completo; eventos de sessão, planejamento e multiagentes, eventos específicos de arquivo/checkpoint/comando, webhooks, plugins, MCP e retornos estruturados de decisão continuam pendentes.

## Uso

Abra um projeto e selecione **Hooks → Criar hook**. Defina nome, momento, comando, diretório relativo e timeout. Eventos de ferramenta também exigem uma seleção de ferramentas acionadoras. O daemon fornece o catálogo de eventos exibido pela interface. Um exemplo é executar `pnpm test` depois de `fs.apply_patch`. Salvar não executa o comando.

Hooks são executados em Construir e Revisão. Para eventos de ferramenta, a ferramenta acionadora deve ter sido permitida pela política e pelo usuário. Consulta e Planejamento não executam hooks nativos. Cada acionamento exige aprovação individual; a autorização por sessão de uma ferramenta não autoriza seus hooks. A aprovação exibe nome, comando, diretório, momento e ferramenta acionadora.

Todos os hooks desta entrega interrompem a execução em caso de erro, timeout ou negação. Um hook anterior impede a ferramenta de iniciar. Um hook posterior interrompe os próximos passos, preservando o resultado e as alterações da ferramenta já executada. Uma saída zero permite continuar, mas nunca supera uma negação da política. Texto impresso pelo comando é saída não confiável, não uma instrução para modificar permissões.

## Momentos disponíveis

| Evento | Quando é acionado | Efeito de uma falha ou negação |
| --- | --- | --- |
| before_model_request | Antes de cada consulta, incluindo novas rodadas após ferramentas | Nenhuma requisição dessa rodada é enviada ao provedor |
| after_model_response | Depois de persistir uma resposta completa e o estado do protocolo | Chamadas propostas ficam registradas como não executadas; o modelo não é consultado novamente nessa execução |
| before_final | Depois de uma resposta sem chamadas de ferramentas, antes de concluir a execução | A execução não recebe estado completed |
| before_tool | Após a política/aprovação, antes de iniciar a ferramenta selecionada | A ferramenta não inicia |
| after_tool | Depois de persistir o resultado da ferramenta selecionada, inclusive falhas | Próximos passos são interrompidos, preservando os efeitos anteriores |
| tool_error | Após after_tool, se a ferramenta executada falhou | Próximos passos são interrompidos; uma negação da ferramenta não aciona este hook |

O streaming continua visível durante a consulta. after_model_response e before_final controlam os próximos passos e a conclusão; não ocultam texto já recebido nem desfazem dados enviados ao provedor. Respostas incompletas não acionam after_model_response. Eventos de modelo e conclusão exigem uma lista tools vazia e não recebem uma ferramenta fictícia.

Um comando com código de saída diferente de zero, timeout ou interrupção é registrado como ferramenta sem sucesso. A indicação isError de um resultado MCP também é preservada como falha. Um hook tool_error que termina com sucesso não transforma a ferramenta anterior em sucesso: ele apenas permite que o agente observe o erro e decida o próximo passo dentro da política.

## Configuração e ciclo

- Configurações ficam no banco local, associadas ao workspace; arquivos do repositório e skills não são importados automaticamente como hooks.
- Cada execução usa uma cópia das configurações, registrada em `hooks.snapshot`. Novos hooks se aplicam somente a execuções futuras. Os botões Subir/Descer definem a ordem entre hooks do mesmo momento. Reordenar afeta apenas execuções futuras: a sequência já carregada e suas aprovações permanecem válidas. Empates de posição em configurações antigas usam o identificador como desempate.
- Editar, desativar ou remover um hook revoga suas aprovações pendentes da versão anterior e impede acionamentos ainda não iniciados. Hooks desativados não entram na cópia de configuração de novas execuções. A existência e o hash são verificados novamente após a espera por aprovação. Um comando já iniciado deve ser interrompido pelo cancelamento da execução.
- Hooks não recebem argumentos interpolados do modelo, não disparam outros hooks e não são publicados como ferramentas do modelo.
- Comandos têm limite de 16 KB, timeout de 1 a 300 segundos e saída limitada pelo executor. Há até 16 hooks por workspace. O diretório deve estar dentro da raiz autorizada e existir para ativar o hook. É possível desativar uma configuração cujo diretório deixou de existir.
- O executor usa Job Objects no Windows para encerrar a árvore de processos controlada no cancelamento. O modo offline bloqueia hooks nativos. O executor continua tendo **isolamento reduzido**: aprovação de um shell não equivale a um sandbox de arquivos ou rede.
- Scripts e executáveis referenciados pelo comando podem mudar no disco. Esta entrega exige nova aprovação em cada acionamento; não promete fixar o conteúdo de dependências transitivas.

## API e evidências

`GET /v1/hooks/events` publica os eventos disponíveis, seus nomes em português e a necessidade de filtro de ferramentas. `GET /v1/workspaces/:id/hooks` lista configurações em ordem; `POST` cria um hook com identificador atribuído pelo daemon e revisão 1. As rotas usam a autenticação local existente.

`PUT /v1/workspaces/:id/hooks/:hook_id` recebe `{hook, expected_revision}` e incrementa a revisão após validar a configuração. O campo enabled ativa/desativa sem apagar o hook. `DELETE` no mesmo caminho exige `{expected_revision}`. Uma versão antiga é recusada sem sobrescrever ou remover a configuração atual.

`POST /v1/workspaces/:id/hooks/order` recebe `{ids, expected}`, com todos os identificadores do workspace exatamente uma vez e a lista esperada de `{id, revision, position}` na ordem anterior. A operação ocorre em uma transação SQLite: não há ordem parcialmente aplicada, e uma lista desatualizada é recusada. Reordenar altera apenas position, preservando revisão e hash de autorização.

A interface preserva o texto de uma edição recusada e oferece carregar explicitamente a versão atual, descartando a edição local. Configurações anteriores sem os novos campos usam enabled=true, position=0 e revision=1. Novos hooks são adicionados ao fim da lista.

Os eventos `hook.started`, `hook.output` e `hook.completed` registram identificador do acionamento, evento acionador, vínculo à chamada de ferramenta quando aplicável e resultado. Aprovações de eventos do modelo/conclusão têm trigger_tool e trigger_call_id nulos; a sequência persistida associa o hook à rodada. `tool.approval_required` usa o nome interno `hook.exec`, que não é aceito pelo catálogo de ferramentas do modelo. A conversa mostra os resultados dos hooks e os logs preservam os eventos. O evento approval.revoked informa a revogação na conversa e no histórico. Aprovações de uma revisão nova não são revogadas por uma invalidação tardia da revisão antiga. O replay não executa comandos novamente.

Se um hook interrompe um lote de chamadas, as chamadas restantes recebem resultados explícitos de cancelamento no histórico. O daemon não solicita outra resposta do modelo nessa execução e não repete efeitos automaticamente.

`crates/daemon/tests/hooks_flow.rs` valida dez cenários com modelo determinístico e processos Node reais: sucesso, negação, falha anterior, falha posterior, revogação durante aprovação, modo Plano, ferramenta negada, cancelamento, timeout e ativação do modo offline durante a aprovação. O teste também confere ordem, preservação de trabalho anterior e fechamento do lote no histórico.

`crates/daemon/tests/hooks_lifecycle.rs` verifica a ordem em duas rodadas de modelo, 15 combinações de evento com negação/cancelamento/revogação/offline/falha, os modos Consulta e Plano e falhas reais de processo/leitura e resposta MCP isError via HTTP de teste. Uma negação anterior ao modelo deixa o contador do servidor HTTP em zero; uma negação posterior impede ferramentas e fecha o lote no histórico.

Os testes de gestão verificam conflito entre dois escritores, compatibilidade de configurações antigas, remoção com versão desatualizada, isolamento por workspace, rollback de reordenação inválida e revogação pela API. Um teste com processos Node reais reordena durante a aprovação e comprova que a execução ativa preserva a ordem antiga, enquanto a próxima usa a nova.
