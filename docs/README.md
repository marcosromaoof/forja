# Documentação do FORJA

O FORJA está em desenvolvimento. Comece pelo guia que corresponde à sua tarefa; [STATUS.md](STATUS.md) indica o que já foi validado e o que ainda é parcial ou pendente.

## Usar o aplicativo

- [Guia do usuário](USER_GUIDE.md) — projetos, conversas, planos, chat, modelos, contexto, agentes, busca web e recuperação.
- [Skills](SKILLS.md) — formato, seleção, carregamento e limites.
- [Hooks](HOOKS.md) — eventos, configuração, aprovações e falhas.
- [Dados e privacidade](DATA_AND_PRIVACY.md) — armazenamento local, cofre de credenciais, serviços remotos, backup e exclusão.

## Desenvolver e integrar

- [Desenvolvimento](DEVELOPMENT.md) — pré-requisitos, execução local, testes, estrutura do monorepo e fluxo de mudança.
- [Arquitetura](ARCHITECTURE.md) — processos, contratos, persistência, agente, política e recuperação.
- [API local](API.md) — autenticação, erros e rotas `/v1`.
- [Segurança](../SECURITY.md) — modelo de ameaça, reporte e limites conhecidos.
- [Contribuição](../CONTRIBUTING.md) — critérios para mudanças e revisões.

## Acompanhar a implementação

- [Estado atual](STATUS.md) — funcionalidades, evidências de teste e limites operacionais.
- [Matriz de requisitos](REQUIREMENTS_MATRIX.md) — capacidade solicitada, estado e trabalho para aceite.
- [Roadmap](ROADMAP.md) — pendências organizadas por prioridade e etapa.

## Onde o comportamento é definido

Os contratos serializados estão em [contracts.rs](../crates/core/src/contracts.rs), as rotas em [lib.rs](../crates/daemon/src/lib.rs) e a política de ferramentas e caminhos em [policy.rs](../crates/core/src/policy.rs). A documentação descreve o comportamento da revisão atual; se houver divergência, confirme no código e nos testes antes de alterar uma integração.
