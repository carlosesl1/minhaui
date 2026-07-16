# Classificação de mudanças — 2026-07-15

## Método

Inventário produzido com `git status --porcelain=v1 -uall` depois da criação da
governança e durante a estabilização dos gates, imediatamente antes da criação
deste próprio arquivo. Cada caminho foi agrupado pela primeira pasta de
propriedade. Esta classificação não autoriza stage, commit, movimentação ou
exclusão.

## Visão geral

| Classe | Modificados | Não rastreados | Total | Disposição proposta |
| --- | ---: | ---: | ---: | --- |
| `.omo/evidence/**` | 76 | 592 | 668 | Preservar; não integrar em massa; selecionar evidências duráveis ou arquivar externamente após aprovação |
| Outros `.omo/**` | 5 | 0 | 5 | Revisar individualmente como metadados de processo |
| `crates/**` | 77 | 51 | 128 | Fonte/testes candidatos a Git; dividir por incremento funcional |
| `docs/**` | 1 | 22 | 23 | Documentação durável; separar governança de planos históricos |
| `scripts/**` | 0 | 6 | 6 | Automação de governança candidata a Git |
| `packaging/**` | 7 | 0 | 7 | Fonte de distribuição; validar em incremento próprio |
| Raiz, CI e configuração | 15 | 0 | 15 | Revisar por responsabilidade e integrar com o incremento correspondente |
| **Total** | **181** | **671** | **852** | — |

## Detalhamento por propriedade

### Evidências `.omo`

- 668 caminhos estão em `.omo/evidence/**`.
- 592 deles não são rastreados e 76 já rastreados foram modificados.
- Maiores subárvores atuais incluem `f30-final` (56),
  `f25-figma-liquid-glass-swapchain-fixed` (40),
  `f26-figma-liquid-glass-final` (40),
  `f27-figma-liquid-glass-final` (40) e `f33-dock-menu-flicker` (37).
- Capturas, logs e relatórios repetidos são evidência de execução, não fonte do
  produto. Nenhum item será removido antes de uma lista explícita de retenção.

### Código e testes

| Crate | Caminhos alterados |
| --- | ---: |
| `shell-platform-windows` | 73 |
| `shell-renderer` | 37 |
| `shell-core` | 10 |
| `shell-config` | 5 |
| `shell-app` | 2 |
| `shell-watchdog` | 1 |

A concentração em plataforma e renderer confirma que o baseline não deve ser
integrado como um único incremento. A separação proposta é:

1. governança, gates e classificação de testes;
2. domínio/configuração;
3. dock, contexto e Gerenciador de Tarefas;
4. preview e identidade de janelas;
5. renderer e refinamento visual;
6. AppBar, multimonitor e ciclo de vida nativo;
7. packaging e documentação de distribuição.

Essa lista é uma estratégia de revisão, não uma ordem de stage ou commit.

### Documentação e automação

- `docs/architecture/**`: 13 caminhos de governança e auditoria.
- `docs/superpowers/**`: 9 planos históricos ou executáveis.
- `scripts/**`: 6 caminhos do gate arquitetural e suas fixtures.
- `.github/workflows/ci.yml`: separação entre gate determinístico e validação
  nativa opt-in.

## Critérios de retenção

Um caminho é candidato a fonte durável quando satisfaz pelo menos um destes
critérios:

- necessário para compilar, testar, empacotar ou operar o produto;
- fixture pequena e determinística usada por teste;
- decisão, contrato ou instrução de manutenção vigente;
- licença ou obrigação de distribuição;
- script reproduzível usado por gate.

Um caminho é candidato a evidência externa ou descarte posterior quando for log,
binário, dump, screenshot repetida, captura de uma execução ou relatório que
pode ser regenerado. A decisão final exige aprovação por lista, nunca por
exclusão recursiva genérica.

## Próximo checkpoint de higiene

Antes de qualquer limpeza:

1. gerar uma lista exata dos 668 caminhos de evidência;
2. marcar `reter`, `arquivar` ou `descartar` para cada subárvore;
3. confirmar que fixtures e evidências exigidas por auditoria não estão na lista;
4. obter aprovação explícita;
5. alterar `.gitignore` somente depois de validar que nenhum source path será
   ocultado.

## Resultado do checkpoint de estabilização

Verificação fresca executada após a implementação:

| Gate | Resultado |
| --- | --- |
| Testes da política PowerShell | Verde |
| Direção de dependências e exceções | Verde: 6 crates, 7 exceções registradas |
| `cargo fmt --all --check` | Verde |
| Clippy estrito com todos os targets/features | Verde |
| `cargo test --workspace` | Verde; suíte hermética completa |
| `cargo build --workspace --release` | Verde |
| `cargo deny check advisories bans licenses sources` | Verde, com avisos não bloqueantes de allowances não encontradas |
| Links e whitespace de `docs/architecture/**` | Verde: 13 documentos |

Validação nativa executada separadamente:

- plataforma: 49 testes aprovados e 3 falhas por Calculator/Windows Terminal
  ausentes ou não resolvidos;
- startup AppBar: 1 falha porque o Windows rejeitou o registro real da AppBar no
  ambiente atual.

As falhas nativas continuam visíveis no feature gate `native-validation` e não
foram convertidas em skips. O CI normal agora executa apenas a suíte hermética; o
job nativo opt-in exige o runner controlado `obsidian-native-validation`.

Limitações locais registradas:

- `cargo-audit` não está instalado; o workflow continua instalando e executando
  a ferramenta no CI;
- `actionlint` e parser YAML local não estão disponíveis; o workflow recebeu
  inspeção estrutural e seus comandos locais foram executados, mas a sintaxe
  ainda deve ser confirmada pela primeira execução do GitHub Actions.

Estado de preservação ao final do checkpoint:

- nenhum arquivo staged;
- nenhum caminho marcado como excluído;
- 181 entradas modificadas e 672 não rastreadas no checkout completo;
- nenhuma limpeza ou movimentação de evidências `.omo`.

## Checkpoint de redução da Interface pública

O incremento posterior de fachada pública preservou o grafo de dependências e
reduziu o acoplamento entre crates:

- `shell-platform-windows` passou a expor somente `run_showcase`,
  `ShowcaseRunConfig` e `crate_identity`;
- `PlatformEvent`, controllers, ações, roteamento, animações e helpers de teste
  passaram a ser internos;
- os testes da plataforma e do renderer são incluídos sob `cfg(test)`, portanto
  não forçam reexports de Implementação;
- `shell-renderer` preserva os tipos de cena, geometria e layout necessários à
  plataforma, inclusive os tipos que aparecem em retornos públicos;
- inspeções exclusivas dos testes do renderer passaram a `cfg(test)`;
- a plataforma consome geometria do renderer sem reexportá-la.

`Test-PublicFacadePolicy` adicionou uma allowlist exata por `lib.rs`. Qualquer
novo `pub use`, item público raiz ou Module público não aprovado torna o gate
vermelho e exige uma decisão deliberada.

Verificação focada do incremento:

- testes da política PowerShell: verdes;
- política real: verde para 6 crates e 7 exceções registradas;
- `shell-renderer`: 57 testes verdes;
- `shell-platform-windows`: 186 testes verdes;
- `cargo fmt --all -- --check`: verde;
- Clippy estrito com todos os targets e features: verde;
- `cargo test --workspace`: verde;
- `cargo build --workspace --release`: verde;
- `git diff --check`: verde, com apenas avisos informativos de LF/CRLF no
  checkout preexistente.

Nenhum arquivo foi staged, commitado ou enviado ao remoto neste checkpoint.
