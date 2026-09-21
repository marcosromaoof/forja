# Skills: seleção e recursos

O FORJA descobre `SKILL.md` em `skills/<nome>` e `.forja/skills/<nome>` do projeto. O frontmatter YAML exige nome e descrição; o nome deve corresponder à pasta. Skills são conteúdo não confiável e nunca alteram a política de permissões.

## Escolher para um envio

Com um projeto aberto, use **Selecionar skills** no compositor do início ou da IDE. Busque pelo nome, descrição ou caminho, inspecione o conteúdo e marque até oito entradas. **Usar seleção** aplica a escolha; **Cancelar** preserva a seleção anterior. Os chips do compositor permitem remover uma skill.

O envio inclui as instruções das skills escolhidas. A escolha vale para aquele envio, é limpa após um envio aceito e é removida ao trocar de projeto. Texto do objetivo, anexos e seleção permanecem quando o envio falha. O conteúdo já entregue continua no histórico da sessão, assim como mensagens anteriores; remover um chip não apaga o histórico.

Cada escolha contém caminho relativo e SHA-256. Antes de criar a execução, o daemon verifica formato, raiz, conteúdo, duplicatas e limites. Se uma skill mudou ou desapareceu, a execução não é criada. Abra o seletor e atualize explicitamente a seleção para aceitar o conteúdo atual, ou remova a entrada. Atualizar o catálogo não aceita silenciosamente a nova versão.

O conteúdo validado é capturado em memória ao iniciar e persistido no evento `skills.selected` antes da primeira consulta ao modelo. O evento registra origem `user_selection`, fonte, hash, metadados, conteúdo e `permissions_granted=false`. A execução também guarda as referências em `selected_skills`. Alterações posteriores no disco não substituem esse conteúdo. Replay e recuperação leem o histórico persistido, sem reler a skill nem executar seus scripts.

A seleção não é uma lista de exclusão: o agente continua recebendo o catálogo de metadados e pode carregar outras skills quando pertinentes, por `skills.read`. As instruções das skills não escolhidas e seus recursos continuam sendo carregados progressivamente. Modos Consulta/Plano e aprovações de ferramentas continuam sendo impostos pelo daemon, incluindo quando uma skill declara `allowed-tools`.

## Inspecionar recursos

No seletor ou no painel **Skills**, expanda uma skill e escolha **Explorar recursos**. A lista mostra caminhos relativos e tamanhos. Clique em um arquivo para ler texto UTF-8 e ver seu SHA-256. Scripts são apenas texto: essa ação nunca os executa. Arquivos binários, conteúdo com NUL e arquivos maiores que 64 KB não têm prévia de texto.

Listagem e leitura verificam o hash do `SKILL.md`. Recursos têm seu próprio hash calculado ao ler; esta entrega não fixa previamente o conteúdo de toda a árvore de referências. Se a skill mudou, atualize o catálogo antes de abrir recursos. Recursos vistos na interface não são automaticamente anexados ao envio.

Links simbólicos, junctions, caminhos fora da skill e arquivos protegidos pela política são recusados. A listagem omite caminhos recusados, não lê o conteúdo dos arquivos e visita no máximo 512 entradas, com até 128 arquivos e seis níveis de pastas a partir da raiz. Uma lista parcial é indicada na interface. Esses limites evitam varreduras ilimitadas; não representam contenção de programas arbitrários.

## API

Todas as rotas usam autenticação local existente e resolvem o workspace no daemon:

- `GET /v1/workspaces/:id/skills`: catálogo de inspeção da interface.
- `POST /v1/workspaces/:id/skills/resources`: `{path, hash}`, retorna nomes, tamanhos, limites e `truncated`.
- `POST /v1/workspaces/:id/skills/read`: `{path, hash, resource?}`, lê o corpo ou um recurso de texto.
- `POST /v1/sessions/:id/runs`: `{goal, selected_skills?: [{path, hash}]}`. O workspace vem da sessão, não do corpo da requisição. Campos adicionais dentro de uma seleção são recusados.

Limites: até oito skills por envio, até 128 KB de contexto serializado das skills escolhidas, até 64 KB por arquivo, 128 skills descobertas e 32 KB de metadados no catálogo enviado ao modelo. Requisições anteriores sem `selected_skills` e execuções antigas continuam válidas, com seleção vazia.

## Evidências e pendências

Os testes de core verificam limites, duplicatas, hashes, arquivos protegidos, prévia de scripts sem execução e listagem parcial. Os testes da API verificam entradas malformadas, seleção antiga, isolamento entre workspaces, ausência de execução criada quando há erro e leitura de recursos. Um servidor de modelo determinístico recebe o conteúdo original mesmo após uma alteração no disco, e a reabertura do banco preserva a seleção sem outra consulta ao modelo.

O fluxo de skill hostil é testado com descoberta progressiva e seleção explícita, nos modos Plano e Construir: nenhuma das duas formas libera comandos sem a política e a aprovação. Testes de modelo são fixtures, não validação de um provedor pago.

Instalação de bibliotecas externas, edição/remoção de skills pelo gerenciador, anexar recursos individualmente e empacotamento de dependências de scripts continuam pendentes. A interface permite criar skills e inspecionar os arquivos existentes; mudanças adicionais podem ser feitas pelo editor ou pelas ferramentas autorizadas.
