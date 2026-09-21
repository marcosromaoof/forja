# Dados e privacidade

## Onde os dados ficam

No Windows, o diretório padrão é `%LOCALAPPDATA%\Forja`. `FORJA_DATA_DIR` substitui esse local em desenvolvimento. Os principais dados são:

| Dado | Local/armazenamento | Observação |
| --- | --- | --- |
| Conversas, eventos, planos e configurações | `forja.sqlite` | SQLite em WAL |
| Artefatos, screenshots e conteúdos preservados | `blobs/<sha256>` | Conteúdo endereçado por hash |
| Descoberta do daemon | `daemon.json` | URL, PID, protocolo e token local; não compartilhar |
| Chaves de modelos | Credential Manager, serviço `app.forja.provider` | Banco guarda referência opaca |
| Tokens MCP | Credential Manager, serviço `app.forja.mcp` | Banco guarda referência opaca |
| Chaves de busca | Credential Manager, serviço `app.forja.search` | Banco guarda referência opaca |
| Planos legíveis | `<workspace>/docs/forja-plans/` | Criados somente após plano estruturado |

Telemetria permanece desligada. Não existe conta FORJA nem serviço hospedado do produto nesta revisão.

## O que pode sair do computador

Ao usar um modelo remoto, o FORJA envia ao provedor selecionado o prompt-base, definições de ferramentas, histórico efetivo, plano/checkpoint fixado, anexos escolhidos e a mensagem atual. Imagens são enviadas apenas quando o perfil declara visão. Revisores recebem uma versão do mesmo histórico, sem ferramentas.

Busca web, `web.fetch`, MCP HTTP e Computer Use acessam os destinos autorizados. URLs visitadas, console, rede e screenshots podem conter informações sensíveis da página e ficam no histórico/artefatos locais. Revise origens e conteúdo antes de exportar ou compartilhar uma conversa.

O FORJA não alterna de backend local para nuvem silenciosamente. Um modelo `cloud` servido por uma API local ainda deve ser classificado corretamente pelo usuário/provedor.

## Segredos

Chaves não são gravadas no repositório, banco, eventos ou documentos de plano. O daemon aceita o valor na mutação de configuração, grava no cofre e persiste somente seu próprio identificador interno. Atualizar endpoint autenticado exige informar a chave novamente. Erros devem omitir request headers e corpos sensíveis.

Não use credencial em:

- URL de provedor, Git ou MCP;
- arquivo `.env` versionado;
- prompt, plano, skill, hook ou argumento de terminal;
- screenshot ou exportação compartilhada;
- issue pública.

## Backup e exportação

Backup inclui o snapshot do SQLite, blobs e manifesto SHA-256. Ele exclui Credential Manager, `daemon.json` e a árvore completa dos workspaces. Credenciais precisam ser configuradas novamente em outra máquina.

A exportação de conversa pode conter código, comandos, resultados, planos, caminhos, URLs e conteúdo recuperado da web. Ela é uma ação local explícita e deve ser revisada antes de publicação.

## Proteção do repositório

O `.gitignore` exclui bancos (`*.db`, `*.sqlite*`), backups, `.forja/`, `.env*`, chaves/certificados, credenciais JSON, tokens, logs, screenshots de validação, dependências e builds. Antes do baseline local desta revisão, o conjunto candidato ao commit foi verificado por nome, tamanho e padrões de segredo. Nenhuma chave, token, banco ou credencial real foi encontrada.

Essa verificação não prova que textos de projeto futuros não terão informação confidencial. Todo commit deve revisar exatamente o índice preparado, inclusive documentação, fixtures e lockfiles.

## Exclusão e retenção

Remover um perfil de modelo apaga apenas a configuração local; não remove um modelo do serviço externo e não apaga snapshots históricos. Remover um provedor também remove a credencial correspondente do cofre quando permitido pelo estado das execuções.

Para apagar todos os dados locais, encerre interface e daemon, remova a pasta de dados escolhida e exclua as credenciais `app.forja.provider`, `app.forja.mcp` e `app.forja.search` no Credential Manager. Faça isso somente depois de guardar os backups necessários.

## Limitações atuais

- A proteção do token local depende também das ACLs do perfil do usuário do Windows; endurecimento e teste explícito de ACL do `daemon.json` continuam no roadmap.
- Processos nativos autorizados podem ler dados acessíveis ao mesmo usuário e usar rede. O aplicativo informa isolamento reduzido.
- O grant de navegador é por origem e capacidade; classificação de pagamentos, publicação, uploads e outros efeitos irreversíveis ainda não está implementada.
- Plugins em AppContainer ainda não estão disponíveis.
