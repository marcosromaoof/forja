# API local `/v1`

## Convenções

A API é interna ao aplicativo. Ela escuta somente em loopback, usa token Bearer descoberto em `daemon.json` e valida a origem. Clientes não devem registrar esse token. O protocolo atual é versão 1.

Erros seguem:

```json
{
  "code": "provider_unavailable",
  "message": "Serviço local indisponível.",
  "retryable": true,
  "details": {},
  "correlation_id": "..."
}
```

Segredos, headers e credenciais nunca devem aparecer em `details`. Operações concorrentes usam `revision`; chamadas de efeito usam aprovação, hash-base ou idempotência conforme o recurso.

## Saúde e configurações

- `GET /v1/health`
- `GET|PUT /v1/settings`
- `POST /v1/backup`
- `POST /v1/backup/verify`
- `POST /v1/backup/restore`

## Workspaces, arquivos e Git

- `GET|POST /v1/workspaces`
- `POST /v1/workspaces/clone`
- `GET /v1/workspaces/:id/files`
- `GET|PUT /v1/workspaces/:id/file`
- `GET /v1/workspaces/:id/search`
- `GET /v1/workspaces/:id/git`
- `POST /v1/workspaces/:id/git/init`
- `POST /v1/workspaces/:id/index`
- `GET /v1/workspaces/:id/map`
- `GET /v1/workspaces/:id/symbols`
- `GET /v1/workspaces/:id/checkpoints`
- `POST /v1/checkpoints/:id/restore`

Caminhos de arquivo são relativos ao workspace; abrir um workspace exige caminho absoluto existente. Clone aceita HTTPS sem credenciais embutidas e destino absoluto novo.

## Conversas, execuções e eventos

- `GET|POST /v1/workspaces/:id/sessions`
- `GET|POST /v1/sessions`
- `PATCH /v1/sessions/:id`
- `GET /v1/sessions/:id/events?after_session_sequence=&limit=`
- `POST /v1/sessions/:id/runs`
- `GET /v1/runs`
- `POST /v1/runs/:id/{cancel|pause|resume}`
- `GET /v1/approvals`
- `POST /v1/approvals/:id/decision`
- `GET /v1/interactions`
- `POST /v1/interactions/:id/answer`
- `POST /v1/interactions/:id/cancel`
- `GET /v1/sessions/:id/context`
- `POST /v1/sessions/:id/compact`
- `GET /v1/sessions/:id/export`

A listagem por workspace é a fonte da barra lateral da IDE. `session_sequence` é o cursor estável do replay entre várias execuções.

## Planos e código proposto

- `GET /v1/sessions/:id/plans`
- `GET /v1/plans/:id`
- `POST /v1/plans/:id/{implement|retry}`
- `GET|POST /v1/plans/:id/checkpoints`
- `GET /v1/plans/:id/checkpoints/latest`
- `GET /v1/plans/:id/revisions`
- `POST /v1/plans/:id/revisions/:revision_id/{approve|reject}`
- `GET /v1/sessions/:id/code-proposals`
- `POST /v1/code-proposals/:id/{apply|dismiss}`

Aplicar código exige hash-base válido e cria checkpoint. Implementar um plano captura baseline antes de qualquer mutação e é idempotente para a revisão esperada.

## Provedores e modelos

- `GET|POST /v1/providers`
- `DELETE /v1/providers/:id`
- `GET /v1/providers/:id/models`
- `POST /v1/providers/:id/probe`
- `GET|POST /v1/model-profiles`
- `GET|PUT|DELETE /v1/model-profiles/:id`

O corpo pode conter `api_key` apenas ao criar/atualizar uma credencial. A resposta contém no máximo uma referência opaca. Essa referência é definida pelo daemon e não é aceita como autoridade do cliente.

Falhas de upstream usam códigos como `provider_auth_failed`, `provider_unavailable`, `provider_rate_limited`, `provider_invalid_response`, `provider_misconfigured` e `provider_upstream_error`.

## Busca, agentes e artefatos

- `GET|POST /v1/search-providers`
- `DELETE /v1/search-providers/:id`
- `GET|POST /v1/workspaces/:id/agents`
- `PUT|DELETE /v1/agents/:id`
- `GET /v1/sessions/:id/agent-tasks`
- `GET /v1/artifacts/:id`
- `GET /v1/artifacts/:id/content`

Perfis e tarefas são validados contra workspace/sessão. Credenciais de busca ficam no cofre do sistema; `secret_ref` é controlada pelo daemon.

## Skills, hooks, LSP e MCP

- `POST /v1/skills/validate`
- `GET|POST /v1/workspaces/:id/skills`
- `POST /v1/workspaces/:id/skills/{read|resources}`
- `GET /v1/hooks/events`
- `GET|POST /v1/workspaces/:id/hooks`
- `PUT|DELETE /v1/workspaces/:id/hooks/:hook_id`
- `POST /v1/workspaces/:id/hooks/order`
- `GET /v1/lsp/presets`
- `GET /v1/workspaces/:id/lsp`
- `POST /v1/workspaces/:id/lsp/{start|sync}`
- `POST /v1/workspaces/:id/lsp/:language/{stop|query}`
- `GET /v1/workspaces/:id/lsp/:language/diagnostics`
- `GET /v1/mcp/catalogs`
- `GET|POST /v1/mcp/servers`
- `DELETE /v1/mcp/servers/:id`
- `POST /v1/mcp/servers/:id/{connect|disconnect|call}`
- `GET /v1/mcp/servers/:id/{catalog|tools|resources|prompts}`

Salvar hook ou MCP não executa processo. Conexão e chamada têm avaliações separadas. Modo offline encerra serviços incompatíveis.

## Terminal

- `POST /v1/terminals`
- `GET /v1/terminals/:id`
- `POST /v1/terminals/:id/input`
- `POST /v1/terminals/:id/resize`
- `DELETE /v1/terminals/:id`

O terminal manual usa PTY e autoridade do usuário local. Ele é separado do executor de ferramentas e fica indisponível no modo offline estrito.
