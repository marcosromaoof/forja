# Trabalho restante

Este backlog descreve o que falta para cumprir o plano integral. Itens marcados como pendentes não devem ser apresentados como recursos prontos para produção.

## Prioridade imediata: segurança e consistência

1. Aplicar ACL explícita ao diretório de dados e a `daemon.json`, com testes em contas Windows distintas.
2. Introduzir classificação e aprovação específica para cliques de navegador que possam publicar, pagar, enviar arquivos ou credenciais.
3. Gerar tipos TypeScript a partir dos contratos Rust e eliminar duplicação manual.
4. Ampliar migrações versionadas, backup/rollback e testes de interrupção.
5. Testar junctions, reparse points, nomes alternativos e corridas de caminho em Windows.
6. Ampliar reconciliação de comandos, MCP e ações do navegador com resultado incerto.
7. Fazer auditoria independente antes de qualquer release ou publicação do repositório.

## Etapa 0 — Fundação

- Geração de tipos TS e schemas a partir de Rust.
- Configurações humanas em TOML com precedência e proveniência completas.
- Matriz de migrações para todas as versões de armazenamento.
- Testes adicionais de Job Objects, encerramento abrupto e symlinks/junctions.
- Métricas e logs estruturados com política de redaction.

## Etapa 1 — Agente funcional

- Progresso e cancelamento de clone com limpeza/reconciliação do destino parcial.
- Cobertura de efeitos incertos em todas as ferramentas mutáveis.
- Fluxos E2E de bug real em repositórios adicionais.
- Refinar títulos automáticos de conversa e arquivamento.

## Etapa 2 — IDE e contexto

- Empacotar Node, Chromium e servidores LSP suportados.
- Memória revisável com origem explícita e UI de edição.
- Persistir e restaurar todas as divisões, abas e layouts Foco/IDE.
- Rename, code actions e mais linguagens LSP.
- Reduzir/carregar sob demanda o chunk Monaco, hoje com aviso de aproximadamente 3,3 MB.
- Aceite completo por teclado, leitor de tela, 125%/150% e comparação visual final.

## Etapa 3 — Skills, hooks e MCP

- Instalador/editor de bibliotecas de skills e anexação individual de recursos.
- Eventos adicionais de hooks para sessão, plano, agentes, arquivo e checkpoint.
- Webhooks e destinos declarativos com política própria.
- OAuth MCP e negociação das extensões Tasks, Skills e Apps.
- Matriz completa das versões MCP alvo e renovação segura de sessão.

## Etapa 4 — Plugins

- Host por processo com AppContainer e ambiente mínimo.
- SDK, UI declarativa e broker de filesystem/rede.
- Instalação por arquivo, atualização, rollback e revogação.
- Assinatura, hashes, SBOM e política de permissões por versão.
- Teste hostil que prove bloqueio de segredo, rede e arquivos fora do grant.

## Etapa 5 — Multiagentes

- Integração transacional de branches/worktrees.
- Revisão e resolução assistida de conflitos antes do merge.
- Steering, pausa e recuperação de tarefas em segundo plano.
- Filas e budgets globais, especialmente para inferência local.
- Visão consolidada do DAG e comunicação entre agentes na UI.

## Etapa 6 — Provedores e mídia

- Validação autenticada real de OpenAI, Anthropic, Gemini, Ollama, LM Studio e compatíveis disponíveis.
- Fixtures versionadas de metadados e mudanças de protocolo.
- Adaptador REST declarativo completo e SDK público de provedores.
- ComfyUI: validação de workflow, fila, progresso, cancelamento, artefatos e erros de recursos.
- Telemetria local de GPU/VRAM quando disponível, sem pressupor hardware.

## Etapa 7 — distribuição

- Empacotar daemon e runtimes internos no Tauri.
- Instalador NSIS x64 com WebView2 e desinstalação segura.
- Assinatura de executáveis e atualização assinada configurável.
- Runner remoto de referência com protocolo versionado, TLS e autenticação.
- Ativação guiada de backup/restauração pela interface.
- Testes de instalar, atualizar e recuperar em máquina Windows 10/11 limpa.
- Documentação de release, diagnóstico e suporte.

## Gates para considerar o produto completo

- Todos os cenários obrigatórios da especificação passam em ambiente limpo.
- Nenhuma integração é aprovada apenas por mock quando há endpoint real disponível.
- Plugins hostis, prompt injection e exfiltração são testados de forma independente.
- Instalação, atualização, backup e retomada funcionam sem toolchain de desenvolvimento.
- As duas experiências visuais atingem aceite nas três resoluções e escalas definidas.
- Capacidades não testadas aparecem como desconhecidas, nunca como suportadas.
