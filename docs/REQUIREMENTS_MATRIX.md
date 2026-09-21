# Matriz de requisitos

Estados: **Implementado** significa presente e coberto por alguma validação; **Parcial** exige trabalho antes do aceite; **Pendente** ainda não existe de forma utilizável.

| Área | Estado | Evidência principal | Falta para aceite |
| --- | --- | --- | --- |
| Conversas por projeto | Implementado | Sessões filtradas por workspace, `session_sequence`, replay e restauração da mais recente | E2E desktop ampliado e arquivamento completo |
| Planejamento → Construir | Implementado | `ui.present_plan`, card, baseline e início idempotente na mesma sessão | Mais cenários reais de revisão do plano |
| Documento de plano | Implementado | Plano canônico, blob e Markdown em `docs/forja-plans/` | UI completa para importar divergência externa |
| Checkpoint operacional | Implementado | Cadeia imutável, `.progress.md`, contexto fixado e recuperação | Reconciliação de todo efeito externo incerto |
| Chat técnico | Implementado | GFM sanitizado, código, terminal, patch, screenshots e propostas | Virtualização/performance em históricos extremos e acessibilidade final |
| Modelos e capacidades | Parcial | Provedor/perfil, discovery, probe, reasoning e revisores | Testes autenticados de todos os provedores e SDK declarativo completo |
| Contexto | Implementado | Medidor, proveniência, preflight, compactação e histórico bruto | Tokenizers/contadores adicionais e E2E de janelas muito pequenas |
| IDE | Parcial | Monaco, xterm, explorer, diff, problemas, símbolos e LSP TS/Python | Runtimes empacotados, layouts completos e mais operações LSP |
| Skills | Parcial | Validação, catálogo, seleção e leitura por hash | Instalação, edição e recursos individuais na UI |
| Hooks | Parcial | Seis eventos, aprovação, ordenação, revisão e cancelamento | Mais eventos/destinos e conteúdo transitivo fixado |
| MCP | Parcial | HTTP/stdio, duas versões, catálogo, schema, aprovação e cancelamento | OAuth, Tasks, Skills, Apps e renovação de sessão |
| Agentes | Parcial | Perfis por projeto, DAG, worktrees, budgets e revisão visual | Integração de branches, conflitos e steering completo |
| Busca web | Implementado | Brave, SearXNG, REST, SSRF/DNS/redirect e cofre | Testes reais dos provedores e políticas de quota |
| Computer Use | Parcial | Worker, grants, ações, snapshots, console, rede e screenshots | Classificar efeitos irreversíveis e empacotar Chromium/Node |
| Plugins | Pendente | Inspeção de manifesto apenas | Host AppContainer, SDK, instalação, assinatura, SBOM e testes hostis |
| ComfyUI | Pendente | Contratos arquiteturais | Adaptador, fila, progresso, cancelamento e artefatos reais |
| Backup/restauração | Implementado para desenvolvimento | Manifesto SHA-256, CLI, cópia nova e testes de corrupção | Ativação guiada e teste no instalador final |
| Modo offline | Parcial | Bloqueio de inferência/terminal/MCP/LSP/browser e cancelamento | Instrumentação de rede independente e backends locais verificados |
| Distribuição Windows | Pendente | Build de desenvolvimento Tauri | NSIS, runtimes, assinatura, updater e máquina limpa |
| Runner remoto | Pendente | Contrato planejado | Serviço de referência TLS e autenticação |
| Acessibilidade/visual | Parcial | Resolucões principais inspecionadas, foco e redução de movimento em componentes | Auditoria completa, leitor de tela e escalas 125%/150% |

Para evidências detalhadas e contagens de testes, consulte [STATUS.md](STATUS.md). Para ordem recomendada de execução, consulte [ROADMAP.md](ROADMAP.md).
