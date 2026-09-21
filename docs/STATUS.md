# Estado da implementação — 20/09/2026

O plano aprovado continua sendo o escopo completo. O aplicativo está em desenvolvimento; as entregas abaixo não representam aceite integral das oito etapas.

## Funcionalidades implementadas

- Monorepo Cargo/pnpm, core Rust, daemon Axum, CLI e interface React/Tauri.
- SQLite WAL, eventos persistidos, replay, recuperação sem repetir ferramentas, checkpoints e blobs SHA-256.
- Política determinística, validação de schemas, aprovação por sessão e aprovação individual de operações externas.
- Validação de caminhos, nomes reservados Windows, arquivos protegidos, hash-base e criação de arquivo sem substituir uma versão concorrente.
- Terminal nativo e PTY; Job Objects para acompanhar e encerrar processos. O executor declara isolamento reduzido.
- Ciclo de agente com ferramentas, aprovação, pausa, cancelamento e histórico.
- Conversas isoladas por workspace, restauração da conversa mais recente e replay paginado por `session_sequence` monotônica através de várias execuções.
- Plano estruturado por `ui.present_plan`, exigido explicitamente pela instrução do modo Planejamento; Markdown comum não encerra o fluxo. O daemon cria documento canônico e Markdown em `docs/forja-plans/`, baseline anterior à primeira mutação, checkpoints operacionais imutáveis, projeção `.progress.md`, revisão aprovada e contexto fixado após compactação ou reinício.
- Transição **Implementar Plano** para Construir dentro da mesma conversa, com captura idempotente do baseline e retomada de implementação interrompida.
- Chat técnico com Markdown GFM sanitizado, realce de código, renderers de terminal/ferramentas/patch/screenshot, propostas aplicáveis com hash-base e cards de plano, progresso e revisão.
- Perfis de modelo separados de provedores, proveniência de capacidades/limites, controle de raciocínio por protocolo, executor e até três revisores, medidor de contexto e compactação automática com histórico bruto preservado.
- Início e IDE com Monaco, explorador, conversa, aprovações, terminal, logs e painel Problemas.
- LSP real para TypeScript/JavaScript e Python: programa autorizado por workspace, texto não salvo, hover, completion, definição e diagnósticos. Modelos Monaco separados por workspace.
- Contexto com referências @arquivo, origem e hash, FTS5 e símbolos Tree-sitter. Reindexação transacional remove entradas de arquivos excluídos. Símbolos abrem na linha correta.
- Provedores Ollama, OpenAI-compatible, LM Studio, Responses, Anthropic e Gemini. Descoberta nativa paginada, indicação conservadora de localidade e probe de texto/ferramentas.
- Streaming incremental de UTF-8 e SSE/NDJSON, incluindo registro final sem newline. Uma resposta incompleta não libera chamadas de ferramenta.
- Skills com parser YAML, validação de metadados, criação com checkpoint, catálogo e carregamento progressivo. Instruções e referências são lidas pelo hash esperado. allowed-tools não amplia a política; scripts não são executados ao carregar uma skill.
- Seleção explícita de até oito skills por envio no compositor, validação por hash antes de criar a execução e conteúdo capturado para histórico/replay. Prévia de recursos de texto com limites e sem execução de scripts.
- Hooks nativos antes/depois do modelo, antes da conclusão e antes/depois/erro das ferramentas, configurados pela interface por workspace, com aprovação individual, snapshot por execução, revogação e interrupção em caso de falha. Saídas e chamadas não executadas ficam registradas no histórico.
- Gestão de hooks: edição com revisão esperada, ativação/desativação, remoção protegida e reordenação transacional. Mudanças de conteúdo revogam aprovações antigas; a nova ordem só vale para execuções futuras. Compatibilidade de configurações anteriores mantida.
- MCP HTTP/stdio com versões explicitamente configuradas, catálogo limitado, validação de argumentos e aprovação vinculada ao hash do catálogo. Cancelamento desconecta sem repetir chamadas de resultado incerto.
- Modo offline bloqueia inferência e executores cujo acesso à rede não pode ser contido. Conexões MCP e LSP são encerradas.
- Agentes persistentes por projeto, criados pela interface ou pelo orquestrador, com papel, modelo, raciocínio, ferramentas, turnos, tempo e escrita em worktree. Limites de quatro tarefas ativas e dois escritores são aplicados; falha e timeout viram estados terminais persistidos.
- Busca web por Brave, SearXNG e REST declarativo, com painel de configuração, credenciais no cofre, catálogo seguro `web.providers`, validação de destino e opção explícita para endpoints locais. Computer Use usa worker Playwright separado, contexto efêmero, grant de origens, downloads bloqueados, snapshots acessíveis, console, rede e screenshots em blobs.
- Screenshots podem alimentar automaticamente um agente visual, no máximo três ciclos por execução. Os bytes são enviados somente a perfis que declaram visão, com corpos nativos para OpenAI Responses, Anthropic, Gemini, Ollama e APIs compatíveis; outros modelos recebem referência textual.
- Catálogo de modelos normaliza endpoints sem duplicar `/v1`, `/models` ou `/api`; falhas de credencial, indisponibilidade, rate limit, configuração e resposta inválida retornam códigos acionáveis por provedor.
- Backup consistente do banco e blobs, manifesto e verificação SHA-256, integridade SQLite, restauração em pasta nova. CLI verifica/restaura sem daemon. Dados em uso não são substituídos.
- Backup antes da migração inicial de banco existente, recusa de schema futuro, recusa de restauração incompleta e verificação de integridade ao ler blobs.

## Validação registrada

- **73 testes Rust aprovados**: `cargo test --workspace`, incluindo núcleo, daemon, CLI e integrações reais controladas.
- **1 teste de replay da interface aprovado**: pnpm test.
- **TypeScript e build web aprovados**: pnpm typecheck e pnpm build.
- **Desktop, daemon e CLI compilados no Windows**. A compilação de desenvolvimento inclui as entregas de skills e hooks.
- Fluxo vertical: leitura → aprovação → patch → aprovação → node --test → replay, preservando alteração anterior do usuário.
- LSP: TypeScript Language Server e Pyright reais verificaram hover, completion, definição e diagnóstico de texto não salvo. Acesso fora do workspace e executeCommand foram recusados.
- Provedores: HTTP fragmentado em bytes, UTF-8, EOF, término inválido e probe com nonce/schema. Fixtures nativas preservam IDs e estado opaco.
- Skills: modelo determinístico recebe primeiro metadados, depois corpo e referência; conteúdo hostil não libera comando no modo Plano nem contorna aprovação e negação no modo Agente.
- Seleção de skills: API recusa versões antigas, duplicatas, campos inesperados e caminhos de outro workspace sem criar execução. Alterar o arquivo após o início preserva os bytes originais enviados ao modelo; reabrir o banco preserva a seleção sem nova consulta. Skill hostil testada com seleção explícita e descoberta progressiva em Plano/Construir.
- Recursos de skills: listagem limitada, arquivos protegidos omitidos, hash verificado, prévia de scripts somente como texto e traversal recusado.
- MCP: contratos HTTP e stdio dos dois protocolos, schema hostil, argumentos inválidos, catálogo alterado, UTF-8 fragmentado, resposta malformada e cancelamento sem repetição. O teste stdio inicia Node real e comprova que o Job Object também encerra seu processo filho no Windows.
- Hooks: dez cenários com processos Node reais verificam sucesso/ordem, negação, falhas anterior/posterior, revogação durante aprovação, Plano, ferramenta negada, cancelamento, timeout e mudança para offline. Histórico fecha o lote pendente; trabalho anterior é preservado.
- Hooks do modelo: ordem em duas rodadas, 15 combinações de negação/cancelamento/revogação/offline/falha, modos Consulta/Plano e encerramento dos lotes pendentes. Negação antes da consulta comprova zero requisições no servidor HTTP.
- Resultados de ferramentas: código de saída de processo diferente de zero e resultado MCP isError são exibidos como falha e acionam tool_error; negação de permissão não aciona hooks de erro.
- Gestão de hooks: dois escritores concorrentes têm uma única atualização aceita; requisições com revisão antiga não sobrescrevem nem removem a configuração. Edição, desativação e remoção pela API revogam aprovações pendentes sem iniciar o comando. Uma invalidação antiga preserva aprovações da revisão atual. Reordenação durante uma execução conserva sua sequência; a próxima usa a nova ordem.
- Interface de skills: seleção, cancelamento, atualização explícita após alteração do arquivo, preservação do objetivo, remoção de chips e prévia UTF-8 com hash verificadas no navegador. Modal com ações visíveis em 1366×768; captura forja-skills-selection-stale-1366.png. Prévia de recurso em forja-skills-resources-1672.png. Fixture temporária removida; nenhuma inferência real foi iniciada na interface.
- Interface de gestão: criação, edição, ativação/desativação, reordenação e remoção verificadas no navegador. Um conflito preservou o texto local e a ação explícita carregou a versão atual. Configurações temporárias removidas. Capturas forja-hooks-management-1672.png e forja-hooks-conflict-1366.png.
- Interface dos novos eventos: seleção de antes do modelo, ocultação do filtro de ferramentas, criação/persistência/remoção verificadas em 1366×768; captura forja-hooks-lifecycle-1366.png. Configuração temporária removida.
- Interface de hooks: criação, persistência ao sair/voltar ao painel e remoção verificadas no projeto forja; a configuração temporária foi removida. Capturas em forja-hooks-1366.png e forja-hooks-form-1366.png.
- Backup: cópia e restauração de blobs/eventos; corrupção e traversal recusados; CLI sem daemon; destino existente preservado; versão futura do banco não alterada.
- Interface: backup real de 14,4 MB criado, verificado e restaurado em .forja/recuperacao-ui-20260919. Os testes unitários verificam também cópias contendo blobs; essa cópia de desenvolvimento tinha apenas o banco.
- Interface: skill revisar-forja listada com formato válido. LSP iniciou pelo painel; Problemas foi consultado no editor. A prévia atual está sem arquivos editados não salvos; o texto temporário da validação anterior não existe no repositório.
- Plano/checkpoint: teste persistente comprova materialização do Markdown, igualdade com o blob, baseline do workspace, cadeia `previous_checkpoint_id`, liberação da etapa dependente e reconstrução de contexto fixado.
- Visão: testes de contrato comprovam a codificação multimodal distinta nos cinco protocolos suportados. O worker real abriu a prévia local, produziu snapshot acessível, screenshot PNG de 251.496 bytes e bloqueou navegação a uma origem não autorizada.
- Interface de agentes: papéis, provedor, modelo, raciocínio, escrita, budgets e ferramentas aparecem com nomes acessíveis; o diálogo foi verificado sem transbordamento horizontal em 1366×768, 1672×941 e 1920×1080.
- Interface de busca: Configurações oferece cadastro e edição de Brave, SearXNG e REST, preserva credenciais existentes e explica o acesso à rede local. O fluxo SearXNG foi verificado no navegador com endpoint loopback e botão de gravação funcional, sem persistir configuração temporária.
- Início e IDE inspecionados nas resoluções 1366×768, 1672×941 e 1920×1080. Sem transbordamento horizontal da página. Capturas forja-home-{largura}.png e forja-ide-{largura}.png, ignoradas no Git.
- A composição mantém as referências de cor e distribuição. A fidelidade final, acessibilidade completa e escalas Windows 125%/150% não foram aprovadas.
- O build web mantém um aviso de tamanho do chunk Monaco (~3,3 MB antes de gzip). Não foi ocultado.

## Matriz de etapas

| Etapa | Estado | Trabalho necessário antes do aceite integral |
| --- | --- | --- |
| 0 — Fundação | Parcial, fluxo testado | Tipos TS gerados dos contratos Rust, evolução das migrações, configuração TOML e proveniência, testes ampliados de junctions e processos Windows |
| 1 — Agente | Fluxo vertical, plano e retomada aprovados | Progresso/cancelamento de clone e reconciliação ampliada de efeitos incertos |
| 2 — IDE/contexto | IDE, chat estruturado, compactação e browser worker funcionais | Empacotamento do LSP/Chromium/Node, memória revisável e restauração completa de layouts |
| 3 — Skills/hooks/MCP | Skills progressivas, hooks nativos e MCP parcial | Eventos/tipos adicionais de hooks, OAuth e extensões MCP; instalação/edição de bibliotecas de skills e anexação individual de recursos |
| 4 — Plugins | Inspeção de manifesto | Host AppContainer, SDK, instalação, atualização/rollback, assinatura, SBOM e teste de plugin hostil |
| 5 — Multiagentes | Perfis, DAG, limites, worktrees, artefatos e revisão visual parciais | Integração automática de branches, resolução assistida de conflitos e steering de tarefas em segundo plano |
| 6 — Provedores/mídia | Perfis, reasoning, revisores e visão por protocolo | Integrações autenticadas reais, ComfyUI e controle de recursos de GPU |
| 7 — Distribuição | Build de desenvolvimento | Empacotamento do daemon e runtimes, NSIS, runner TLS, ativação de restauração pela interface, atualização assinada |

## Limites operacionais

- Nenhuma credencial paga foi usada. Testes determinísticos não substituem validação autenticada de provedores reais.
- Hooks cobrem seis eventos com comando nativo e falha interrompendo a execução. Eventos de sessão/planejamento/multiagentes e eventos específicos de arquivo/checkpoint/comando, webhooks, plugins/MCP e decisões estruturadas permanecem pendentes. before_final controla o encerramento, sem ocultar texto já transmitido. Aprovação é individual, scripts não têm conteúdo transitivo fixado, e o executor tem isolamento reduzido. Consulte [HOOKS.md](HOOKS.md).
- MCP moderno ainda não implementa MRTR, OAuth ou Tasks/Skills/Apps. Capacidades não suportadas são recusadas.
- Skills: descoberta limitada a 128 entradas, catálogo de até 32 KB, arquivo/recurso até 64 KB. Recursos são texto UTF-8 e carregados sob demanda. Links e junctions no caminho da skill são recusados. A implementação usa nomes ASCII minúsculos. Seleção explícita limitada a oito skills e 128 KB; recursos são listados com até 128 arquivos/512 entradas visitadas. Referências continuam sob demanda e não são fixadas transitivamente. Consulte [SKILLS.md](SKILLS.md).
- LSP exige Node >=22.22.2 e os pacotes instalados por pnpm. Limite de 100 documentos por conexão; rename e code actions pendentes. Empacotamento e recuperação automática do servidor ainda pendentes.
- Backup copia dados internos, não a árvore completa dos projetos, chaves do cofre ou token de conexão. Ativação requer encerrar o app/daemon e definir FORJA_DATA_DIR para a pasta restaurada. Não há troca automática de pasta em uso.
- O executor nativo tem isolamento reduzido; a validação de caminhos não contém um programa arbitrário.
- O worker Playwright usa Node e Chromium já instalados no ambiente de desenvolvimento. O instalador ainda precisa empacotar versões fixadas desses runtimes.
- O gerenciador de agentes configura ferramentas web/browser, mas uma sessão de navegador só nasce depois de aprovação do grant de origens pelo orquestrador. Downloads continuam bloqueados; uploads, credenciais, pagamentos e publicação não são automatizados por esse grant.
- Simuladores de modelos e MCP estão restritos aos testes.

Referência do formato de skills: [Agent Skills](https://agentskills.io/specification). As declarações de ferramentas no arquivo são metadados; a política do FORJA continua soberana.
