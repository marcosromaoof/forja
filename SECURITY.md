# Política de segurança

O FORJA 0.1.0 está em desenvolvimento e ainda não possui uma versão suportada para produção. Não use esta revisão como barreira de segurança para código ou credenciais de alto valor.

## Como reportar

Quando o repositório estiver publicado no GitHub, prefira um **Security Advisory privado** do repositório. Não publique chave, token, banco, exploit funcional ou conteúdo de usuário em uma issue. Inclua versão/commit, ambiente, impacto, passos mínimos e uma forma sanitizada de reproduzir.

## Modelo de ameaça

O FORJA processa conteúdo não confiável vindo de modelos, workspaces, páginas, skills, hooks, MCP e plugins. Esses dados não podem ampliar permissões. A fronteira privilegiada é o daemon local, que valida schema, workspace, caminho, modo, grant e aprovação antes do efeito.

O aplicativo não tenta se defender de malware já executado com a mesma conta do Windows. Terminal, hooks, MCP stdio, LSP e outros processos autorizados usam a autoridade dessa conta e são apresentados como isolamento reduzido.

## Controles atuais

- API em loopback com token aleatório, origem validada e limite de corpo.
- Ponte Tauri restrita a `/v1`, URL loopback e métodos permitidos.
- Chaves no Credential Manager; banco armazena referência controlada pelo daemon.
- URLs de provedores sem credenciais/query/fragmento e HTTPS, salvo loopback explícito.
- Busca/fetch com mitigação de SSRF, resolução e redirecionamentos revalidados.
- Paths relativos ao workspace, proteção contra escape, arquivos reservados e concorrência por hash-base.
- Escrita atômica e checkpoints anteriores ao patch.
- Política por modo, aprovação por escopo, modo offline e schemas de ferramentas.
- Git clone HTTPS sem credential helper, hooks ou protocolo `file`.
- Browser worker separado, contexto efêmero, origem autorizada e downloads bloqueados.
- MCP e hooks exigem avaliações explícitas; catálogo e revisão são fixados.
- Backups com manifesto SHA-256 e restauração em destino novo.
- Markdown sem HTML bruto e payloads desconhecidos exibidos como dados.

## Credenciais e repositório

Não versionar `.env`, `daemon.json`, bancos, backups, chaves, certificados, logs de sessão, screenshots ou diretórios `.forja`. O `.gitignore` contém essas categorias. Antes de cada publicação, revise o índice preparado e execute uma varredura de segredos no conteúdo exato do commit.

A revisão baseline foi verificada em 20/09/2026: nenhum padrão de chave/token, banco de dados ou arquivo de credencial foi encontrado entre os arquivos candidatos. Durante a revisão, a API de provedores de busca foi endurecida para ignorar referências de credencial fornecidas pelo cliente e exigir revisão esperada em atualizações.

## Limitações conhecidas

- A ACL explícita do diretório de dados e de `daemon.json` ainda precisa de implementação e teste dedicado.
- Job Objects encerram descendentes, mas não são sandbox de segurança.
- Computer Use ainda não reconhece semanticamente pagamento, publicação, upload ou envio de credencial; esses efeitos não devem ser automatizados.
- Plugins com AppContainer ainda não foram implementados.
- OAuth MCP, atualização assinada e runner remoto TLS permanecem pendentes.
- O modo offline bloqueia executores incompatíveis, mas sua garantia final ainda precisa de instrumentação independente de rede.

Veja [docs/ROADMAP.md](docs/ROADMAP.md) para o plano de endurecimento e [docs/DATA_AND_PRIVACY.md](docs/DATA_AND_PRIVACY.md) para retenção e fluxo de dados.
