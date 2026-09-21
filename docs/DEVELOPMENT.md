# Desenvolvimento

## Requisitos

- Windows 10/11 x64 para o alvo atual;
- Rust stable com toolchain MSVC;
- Visual Studio Build Tools com ferramentas C++;
- Node.js 22.22.2 ou posterior;
- pnpm 11.3.0;
- Git;
- WebView2 para o desktop Tauri.

Docker e WSL não são necessários. Ollama, LM Studio, provedores pagos e ComfyUI são integrações opcionais e não são instalados pelo projeto.

## Instalação

```powershell
pnpm install
cargo build -p forja-daemon
```

Desktop Tauri:

```powershell
pnpm desktop
```

Prévia web, em terminais separados:

```powershell
cargo run -p forja-daemon
pnpm dev
```

Acesse `http://127.0.0.1:1420/`. Para isolar dados de desenvolvimento:

```powershell
$env:FORJA_DATA_DIR = 'D:\forja-dev-data'
cargo run -p forja-daemon
```

Nunca aponte `FORJA_DATA_DIR` para uma pasta que será versionada.

## Comandos de qualidade

```powershell
cargo fmt --all -- --check
cargo test --workspace --exclude forja-desktop
pnpm typecheck
pnpm test
pnpm build
cargo build -p forja-desktop
```

`pnpm check` executa testes Rust, typecheck, Vitest e build web. No Windows, encerre um daemon ativo antes de recompilar `forja-daemon.exe`; o sistema bloqueia a substituição de um executável em uso.

Browser worker:

```powershell
node apps/browser-worker/index.mjs
```

Ele fala JSON por linha em stdio quando iniciado pelo daemon. Testes reais dependem do Chromium instalado pelo Playwright.

## Estrutura

```text
apps/
  browser-worker/     worker Playwright
  cli/                CLI Rust
  desktop/            React/Vite e Tauri
crates/
  core/               contratos, política e serviços
  daemon/             API Axum e testes de integração
docs/                  documentação de produto e engenharia
scripts/               utilitários de desenvolvimento
skills/                skills de exemplo
```

## Fluxo para mudanças

1. Confirme o comportamento em [STATUS.md](STATUS.md) e [ROADMAP.md](ROADMAP.md).
2. Altere o contrato Rust antes de depender de um novo campo no front-end.
3. Valide entradas no limite do daemon e novamente no subsistema privilegiado.
4. Não aceite `secret_ref`, IDs de aprovação ou escopos de permissão como autoridade fornecida pelo cliente.
5. Persista o evento antes de transmiti-lo.
6. Para uma mutação de arquivo, exija workspace, caminho validado, hash-base e checkpoint.
7. Para rede, valide URL, resolução, redirecionamentos, tamanho e modo offline.
8. Adicione teste de regressão para regras de segurança, recuperação e concorrência.
9. Atualize documentação e matriz de requisitos.

## API e erros

Rotas novas ficam sob `/v1`. Use o envelope `ApiError` com código estável, mensagem em PT-BR, `retryable`, detalhes sanitizados e `correlation_id`. Não inclua headers, chaves, corpo de credencial nem comando expandido com segredo em logs.

As mutações devem usar revisão esperada ou chave idempotente quando a repetição puder causar efeito. Um cancelamento não equivale a provar que uma operação externa não aconteceu.

## Persistência e migração

O armazenamento usa documentos JSON no SQLite e blobs por hash. Migrações precisam:

- criar backup antes da primeira alteração;
- recusar schema futuro;
- executar em transação;
- preservar eventos e blobs;
- restaurar o backup ao falhar;
- ter teste com uma cópia antiga e uma interrupção simulada.

## Segurança do repositório

O `.gitignore` exclui `.forja/`, bancos SQLite, backups, `.env*`, chaves/certificados, logs, resultados de teste, screenshots locais, dependências e builds. Isso reduz risco, mas não substitui a revisão do índice antes de cada publicação:

```powershell
git status --short --ignored
git diff --cached --name-only
git diff --cached --check
```

Use somente placeholders em fixtures e documentação. Credenciais reais devem ser cadastradas pela interface e guardadas no Credential Manager.

## Estado de distribuição

`pnpm bundle` ainda não produz a entrega final prevista. Permanecem pendentes o empacotamento do daemon, Node/Chromium/LSP, NSIS, atualização assinada, runner remoto e validação em máquina Windows limpa. Veja [ROADMAP.md](ROADMAP.md).
