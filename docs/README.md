# Documentação do FORJA

Esta pasta descreve o estado real do FORJA 0.1.0. O produto está em desenvolvimento e ainda não é uma distribuição pronta para usuários finais. A especificação funcional continua sendo o alvo; [STATUS.md](STATUS.md) separa o que já funciona do que ainda precisa ser entregue.

## Guias

- [Guia do usuário](USER_GUIDE.md): primeiro uso, projetos, conversas, planos, modelos, contexto, agentes e recuperação.
- [Desenvolvimento](DEVELOPMENT.md): pré-requisitos, execução local, testes, estrutura e fluxo de contribuição.
- [Arquitetura](ARCHITECTURE.md): componentes, persistência, execução, limites de confiança e recuperação.
- [API local](API.md): autenticação, erros, paginação e grupos de rotas `/v1`.
- [Dados e privacidade](DATA_AND_PRIVACY.md): dados persistidos, credenciais, serviços remotos, exportação e limpeza.
- [Matriz de requisitos](REQUIREMENTS_MATRIX.md): rastreabilidade entre capacidades pedidas, implementação e evidência.
- [Trabalho restante](ROADMAP.md): backlog organizado por risco e por etapa do produto.
- [Estado e evidências](STATUS.md): inventário detalhado da implementação e validações executadas.
- [Skills](SKILLS.md) e [Hooks](HOOKS.md): contratos, limites e comportamento específico.
- [Política de segurança](../SECURITY.md): reporte, controles e limitações conhecidas.

## Fonte de verdade

Contratos serializados vivem em `crates/core/src/contracts.rs`; as rotas ficam em `crates/daemon/src/lib.rs`; a política de ferramentas e caminhos fica em `crates/core/src/policy.rs`. Quando a documentação divergir do código, trate o código e os testes da revisão atual como comportamento observado e abra uma correção documental.

## Estado desta revisão

Esta revisão foi preparada como baseline local. Nenhum dado de execução, banco SQLite, chave de API, token do daemon, arquivo `.env`, captura de tela ou diretório de dependências faz parte do conteúdo versionado. Consulte [Dados e privacidade](DATA_AND_PRIVACY.md) e [SECURITY.md](../SECURITY.md) para os detalhes da verificação.
