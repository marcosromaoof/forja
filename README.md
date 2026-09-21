# FORJA

Ambiente de programação desktop com agentes de IA, editor de código e execução assistida. O FORJA combina conversa, planejamento, alterações em arquivos e verificação do resultado em um projeto local. A interface é em português e o desenvolvimento prioriza Windows.

> **Estado do projeto:** versão de desenvolvimento. O fluxo principal pode ser executado localmente, mas ainda não há instalador validado para uso geral. Consulte o [estado da implementação](docs/STATUS.md) antes de depender de uma integração específica.

## O que já é possível fazer

| Área | Funcionalidade disponível |
| --- | --- |
| Projetos e conversas | Criar ou abrir um workspace, manter conversas por projeto e recuperar o histórico após reiniciar |
| Planejamento | Criar um plano estruturado, responder perguntas interativas e iniciar **Implementar Plano** na mesma conversa |
| Implementação | Revisar aprovações, aplicar alterações com hash-base, acompanhar checkpoints, diff e resultados de testes |
| IDE | Editar com Monaco, usar terminal, buscar arquivos e símbolos e consultar diagnósticos LSP de TypeScript/JavaScript e Python |
| Modelos | Configurar provedores e perfis, selecionar executor e revisores, acompanhar o uso de contexto e compactar o histórico |
| Extensões | Usar skills, hooks, MCP, agentes por projeto, busca web e navegação automatizada conforme suas permissões |

O fluxo de planejamento mantém um **documento de plano**, um **estado inicial da implementação** e **checkpoints de progresso**. Assim, o agente consegue consultar o que foi aprovado, o que já mudou e o que ainda falta mesmo após compactação de contexto ou reinício.

## Começar no Windows

Você precisa de Rust com toolchain MSVC, ferramentas C++ do Visual Studio, Node.js 22.22.2 ou posterior, pnpm, Git e WebView2. Docker e WSL não são necessários.

Na raiz do repositório:

```powershell
pnpm install
cargo build -p forja-daemon
pnpm desktop
```

No primeiro uso, abra uma pasta de projeto, configure um provedor e escolha explicitamente um modelo. O FORJA não seleciona automaticamente um serviço de nuvem. Ollama e LM Studio exigem que seus servidores estejam em execução; provedores remotos exigem credenciais próprias.

Para abrir apenas a prévia web de desenvolvimento, execute em terminais separados:

```powershell
cargo run -p forja-daemon
```

```powershell
pnpm dev
```

A prévia estará em `http://127.0.0.1:1420/`. Ela usa uma ponte local de desenvolvimento; não substitui o aplicativo desktop. Consulte o [guia de desenvolvimento](docs/DEVELOPMENT.md) para configuração, testes e solução de problemas.

## Um fluxo típico

1. Abra um projeto e crie uma conversa.
2. Selecione **Planejamento** e descreva a tarefa. Responda às escolhas que o agente apresentar.
3. Revise o plano persistido e clique em **Implementar Plano**.
4. Autorize cada ação necessária, acompanhe as etapas e confira arquivos alterados, diffs e testes.
5. Se a execução for interrompida, reabra a conversa e consulte o último checkpoint antes de continuar.

O terminal e outros processos nativos autorizados executam com a autoridade da sua conta Windows e são identificados no aplicativo como **isolamento reduzido**.

## Documentação

O [índice da documentação](docs/README.md) reúne todos os guias. Para ir direto ao assunto:

| Quero... | Leia |
| --- | --- |
| Aprender a usar projetos, chat, planos, modelos e agentes | [Guia do usuário](docs/USER_GUIDE.md) |
| Instalar dependências, executar e testar o código | [Desenvolvimento](docs/DEVELOPMENT.md) |
| Entender processos, persistência e limites de confiança | [Arquitetura](docs/ARCHITECTURE.md) |
| Consultar rotas e contratos locais | [API](docs/API.md) |
| Saber onde ficam dados e credenciais | [Dados e privacidade](docs/DATA_AND_PRIVACY.md) e [Segurança](SECURITY.md) |
| Ver o que funciona e o que falta | [Estado atual](docs/STATUS.md), [matriz de requisitos](docs/REQUIREMENTS_MATRIX.md) e [roadmap](docs/ROADMAP.md) |

## Verificação

```powershell
cargo fmt --all -- --check
cargo test --workspace --exclude forja-desktop
pnpm typecheck
pnpm test
pnpm build
```

Os testes incluem fluxos determinísticos de agente e contratos de integração. Eles não substituem a validação com credenciais, serviços e hardware reais. O resultado mais recente e as limitações conhecidas estão em [docs/STATUS.md](docs/STATUS.md).

## Escopo e licença

O FORJA é um produto local em desenvolvimento. Marketplace público, contas FORJA e infraestrutura hospedada não fazem parte desta entrega. Integrações como ComfyUI, plugins isolados, runner remoto e instalador final ainda estão no [roadmap](docs/ROADMAP.md).

O código está sob a [licença MIT](LICENSE). Para contribuir, leia [CONTRIBUTING.md](CONTRIBUTING.md).
