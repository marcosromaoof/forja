# Contribuindo

O FORJA está em desenvolvimento ativo. Antes de alterar comportamento, leia [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) e [SECURITY.md](SECURITY.md).

## Fluxo local

1. Crie uma branch com escopo claro.
2. Mantenha mudanças pequenas e revise o impacto em persistência, permissões e recuperação.
3. Adicione testes somente quando comprovarem comportamento ou regressão relevante.
4. Execute `cargo fmt --all -- --check`, `cargo test --workspace --exclude forja-desktop`, `pnpm typecheck`, `pnpm test` e `pnpm build`.
5. Atualize a documentação e a matriz de requisitos.
6. Verifique o conteúdo preparado para commit e procure segredos.

Commits devem explicar o resultado, com mensagens no formato `tipo: descrição`, por exemplo `feat: persistir planos e checkpoints`. Não inclua dados locais nem credenciais em fixtures.

## Pull requests futuros

Descreva o problema e o comportamento resultante, inclua validação executada e registre riscos/limitações materiais. Mudanças de banco precisam explicar migração e rollback. Mudanças de política, rede, execução de processos, credenciais, navegador ou plugins precisam incluir análise de abuso e teste negativo.

O repositório usa a licença MIT; consulte [LICENSE](LICENSE). Contribuições devem preservar os avisos de copyright e licença aplicáveis.
