# FORJA

Aplicativo desktop de programação com agentes de IA. Windows primeiro, interface em português e armazenamento local. Este repositório contém uma implementação em desenvolvimento do plano aprovado; o escopo completo ainda não está concluído.

Consulte o [índice de documentação](docs/README.md), o [estado atual](docs/STATUS.md) e o [trabalho restante](docs/ROADMAP.md). Segurança e tratamento de dados estão descritos em [SECURITY.md](SECURITY.md) e [docs/DATA_AND_PRIVACY.md](docs/DATA_AND_PRIVACY.md).

## Desenvolvimento no Windows

Pré-requisitos: Rust com toolchain MSVC, ferramentas C++ do Visual Studio, Node.js >=22.22.2, pnpm e WebView2. Git é necessário para operações de repositório. Docker e WSL não são exigidos.

Na raiz do repositório:

```powershell
pnpm install
cargo build -p forja-daemon
```

Para executar a interface desktop em desenvolvimento:

```powershell
pnpm desktop
```

Para testar somente a interface no navegador, execute em dois terminais:

```powershell
cargo run -p forja-daemon
```

```powershell
pnpm dev
```

Abra http://127.0.0.1:1420. A ponte do servidor Vite é exclusiva de desenvolvimento: o token do daemon fica no processo do servidor, fora do JavaScript da página.

O desktop procura forja-daemon.exe ao lado do executável e o inicia quando necessário. O empacotamento final do daemon no instalador ainda está pendente.

## Primeiro uso

1. Abra ou crie uma pasta de projeto.
2. Em Modelos, configure o endpoint e escolha explicitamente o modelo.
3. Envie um objetivo. Use @caminho/arquivo para incluir contexto do projeto.
4. Revise as aprovações de edição e comando. O terminal nativo tem isolamento reduzido.
5. Consulte histórico, saída das ferramentas e checkpoints.

Ollama e LM Studio dependem de servidores configurados e iniciados pelo usuário. Provedores de nuvem exigem suas próprias credenciais. A aplicação não escolhe um serviço de nuvem nem troca para ele silenciosamente.

Em Contexto, atualize o índice para navegar até a linha dos arquivos e símbolos. Em Inteligência de código, revise e autorize o servidor TypeScript/JavaScript ou Python. O editor oferece hover, sugestões, F12 e diagnósticos no painel Problemas. Os servidores analisam também o texto não salvo. Em Skills, crie e valide arquivos SKILL.md no projeto. O agente recebe os metadados e pode carregar o corpo e referências sob demanda com skills.read, verificando o hash e registrando a origem. Declarações allowed-tools não ampliam permissões e nenhum script é executado durante a leitura. Use Selecionar skills no compositor para incluir explicitamente até oito skills neste envio. A seleção é validada por hash e preservada em caso de erro. Explore recursos de texto no seletor ou no painel, sem executar scripts. Consulte [docs/SKILLS.md](docs/SKILLS.md) para API, limites e comportamento do histórico. O formato segue [Agent Skills](https://agentskills.io/specification), com limites locais documentados em STATUS.

Em Hooks, configure comandos antes/depois das consultas ao modelo, antes da conclusão ou antes/depois de ferramentas específicas e em suas falhas. Um exemplo é executar testes após um patch. Cada acionamento exige aprovação individual. Falha, timeout ou negação interrompem a execução e preservam alterações anteriores. Edite, ative/desative ou reordene os hooks pelo painel. Edições concorrentes são detectadas; alterações de conteúdo revogam aprovações antigas. Novas configurações e mudanças de ordem valem para a próxima execução; Consulta e Planejamento não executam hooks nativos. Consulte [docs/HOOKS.md](docs/HOOKS.md) para limites, API e eventos.

Em Servidores MCP, configure transporte e versão. Salvar não executa o programa. A conexão exige revisão explícita; cada chamada de ferramenta exige aprovação própria. Tokens HTTP ficam no cofre do sistema. OAuth e extensões MCP ainda não estão disponíveis.

## Dados

O diretório padrão no Windows é %LOCALAPPDATA%/Forja. FORJA_DATA_DIR permite apontar um diretório de desenvolvimento separado. Esse diretório contém SQLite, blobs, configurações persistidas e o arquivo privado de descoberta do daemon.

Não compartilhe daemon.json: ele contém o token local de autenticação. Não coloque chaves de API em URLs ou manifests. As chaves configuradas pela interface são armazenadas no cofre do sistema.

Em Configurações, Backup e recuperação cria uma pasta com o banco SQLite, blobs e manifesto SHA-256. A cópia não contém as credenciais do cofre, o token do daemon nem todos os arquivos das pastas de projeto. Verificar integridade valida os hashes e o banco; Restaurar cria uma nova pasta de dados, preservando a pasta atual.

A CLI também verifica e restaura sem um daemon ativo:

```powershell
.\target\debug\forja.exe backup-verify C:\Backups\backup-ID
.\target\debug\forja.exe restore C:\Backups\backup-ID --destination D:\Forja-recuperado
```

Para usar uma restauração, encerre o FORJA e o daemon. Defina FORJA_DATA_DIR para o caminho restaurado no ambiente que inicia os executáveis e abra o aplicativo. Credenciais de outra máquina precisam ser configuradas novamente. Uma cópia incompleta é recusada ao abrir; a recuperação nunca repete ferramentas automaticamente.

## Verificação

```powershell
cargo test --workspace --exclude forja-desktop
pnpm test
pnpm build
cargo build -p forja-desktop
```

Antes de preparar um commit, revise também `git status --short --ignored`, o conteúdo do índice e os padrões de segredo. Bancos, `.env`, credenciais, logs, screenshots e dados `.forja/` estão excluídos do versionamento.

Os testes de integração usam modelos e servidores determinísticos apenas para testes. Incluem alteração real de arquivo, aprovação, execução de teste Node.js, streaming interrompido, recuperação sem repetir ferramentas e backup/restauração pela CLI. Os testes LSP usam TypeScript Language Server e Pyright reais instalados pelo pnpm.

## Organização

- apps/desktop — React, Monaco, xterm e ponte Tauri.
- apps/cli — cliente de linha de comando.
- crates/core — política, armazenamento, agente, ferramentas, contexto e provedores.
- crates/daemon — API autenticada em loopback.
- crates/daemon/tests — fluxos e contratos de integração.
- skills/revisar-forja — skill de exemplo criada pela interface.
- docs — estado e critérios de entrega.

Nenhum marketplace público, conta FORJA ou infraestrutura hospedada faz parte desta entrega.

## Licença

O código está disponível sob a [licença MIT](LICENSE).
