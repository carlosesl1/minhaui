# Native Surface Runtime Design

- Estado: Approved
- Data: 2026-07-15
- Classificação: L — estrutural
- ADR: `docs/architecture/adr/0005-native-surface-runtime.md`

## Objetivo

Extrair a propriedade e o ciclo de vida do renderer e das cinco superfícies
nativas de `RuntimeSurfaces` para um Module interno profundo chamado
`NativeSurfaceRuntime`, preservando integralmente o comportamento visível.

## Contexto

`RuntimeSurfaces` atualmente possui renderer, superfícies de topbar, dock,
popover, preview e settings, controllers de domínio, timers, motion, estado de
preview e coordenação de eventos. Oito arquivos adicionam métodos ao mesmo tipo e
acessam campos `pub(super)`. A separação física diminui arquivos, mas não cria
propriedade nem Locality.

O ciclo gráfico também está distribuído. `win32_owner.rs` constrói e reconstrói
recursos; `win32_dock_render.rs`, `win32_topbar_render.rs`,
`win32_popover_render.rs` e arquivos de preview apresentam ou redimensionam; e
`win32_surface_present.rs` recebe o runtime completo apenas para solicitar
rebuild. Device loss e HRESULT recuperável são classificados em vários lugares.

## Escopo

### Incluído

- propriedade de `CompositionRenderer`;
- propriedade das cinco `WindowSurface`;
- create, resize, present, opacity e animação por Surface Role;
- classificação uniforme de `PresentOutcome` e HRESULT;
- descarte ordenado e rebuild completo;
- uma nova tentativa após falha recuperável durante build;
- recording Adapter determinístico;
- migração incremental dos consumidores atuais;
- atualização dos testes, ADR e exceções arquiteturais.

### Não incluído

- mudanças visuais ou de interação;
- alteração de timings, curvas ou política de autohide;
- extração de Dock Runtime ou Overlay Runtime;
- mudança da direção entre crates;
- nova crate ou Interface pública;
- correção de pré-requisitos do gate nativo Windows;
- otimização de performance sem medição posterior.

## Alternativas consideradas

### Extração incremental por propriedade

Escolhida. Native Surface Runtime é extraído primeiro, seguido futuramente por
Dock Runtime e Overlay Runtime. Cada incremento permanece compilável, testável e
reversível.

### Reescrita direta como Slot Runtime

Rejeitada. Misturaria renderer, dock e overlays num único diff, elevaria o risco
de empréstimos mutáveis sobrepostos e dificultaria localizar regressões.

### Apenas agrupar parâmetros de janela

Rejeitada como objetivo. `SurfaceWindows` reduz assinaturas, mas não transfere
propriedade nem conhecimento. Isoladamente, falha no deletion test.

## Propriedade

`NativeSurfaceRuntime` possui exclusivamente:

- configuração `force_warp` e `solid_material`;
- renderer de produção;
- superfícies topbar, dock, popover, preview e settings;
- estado nativo necessário para reaplicar opacidade e animação;
- Adapter usado para operações gráficas;
- transição de recursos ausentes, válidos ou inválidos por device loss.

Ele não possui:

- DockController, TopbarController, PopoverController ou SettingsController;
- PreviewController ou thumbnails DWM;
- OwnedWindow ou referências persistentes a HWND;
- cenas persistidas;
- timers, fullscreen ou políticas de interação.

`RuntimeSurfaces` permanece temporariamente com controllers e políticas de
domínio, mas substitui renderer e cinco superfícies por um único campo
`surface_runtime`.

## Interface interna

A Interface é privada à crate e orientada por Surface Role:

```text
new(options, adapter)
device_kind()
build(plan)
rebuild(plan)
present(role, frame)
resize(role, frame)
set_opacity(role, value)
start_animation(role, animation)
```

`Surface Frame` contém cena atual e métricas. `Surface Build Plan` contém targets
e frames dos cinco papéis apenas durante build/rebuild. Nenhum método aceita
`&mut RuntimeSurfaces`.

O Adapter de produção usa DirectComposition. O recording Adapter usado em testes
é a segunda Implementação real da Seam e registra operações e falhas injetadas.
A Interface não é pública e não expõe Option de renderer ou WindowSurface.

## Fluxo de apresentação

1. O controller dono do domínio produz uma cena como valor local.
2. O coordenador forma o Surface Frame e chama `present` ou `resize`.
3. Native Surface Runtime usa renderer e superfície correspondentes.
4. O resultado é normalizado para `Presented`, `RebuildAllRequired` ou erro não
   recuperável.
5. O empréstimo do Native Surface Runtime termina.
6. Para `RebuildAllRequired`, o coordenador produz cenas frescas dos cinco
   controllers e chama `rebuild` com um Surface Build Plan.

O Module não mantém frames para recovery. Isso evita apresentar cenas antigas e
evita callbacks que capturam `&mut self`, uma fonte provável de conflitos no
borrow checker.

## Build e recovery

- Recursos antigos são descartados na ordem superfícies e depois renderer.
- Renderer e cinco superfícies novos são construídos em variáveis locais.
- O conjunto só substitui o anterior depois que todas as criações terminarem.
- Opacidade atual da dock é aplicada antes da instalação do conjunto.
- Falha recuperável no primeiro build descarta os recursos parciais e permite uma
  segunda tentativa completa.
- A segunda falha, ou qualquer falha definitiva, é propagada.
- Device loss em present/resize invalida o conjunto e retorna
  `RebuildAllRequired`.
- Ausência inesperada de renderer ou superfície também retorna
  `RebuildAllRequired`.
- Resize válido de um único papel não reconstrói os demais.

## Estratégia para o borrow checker

- Cenas são valores locais produzidos antes do empréstimo mutável de
  `surface_runtime`.
- O resultado de present/resize é copiado e o empréstimo termina antes de gerar o
  Surface Build Plan.
- Não haverá closure de recovery capturando `&mut RuntimeSurfaces`.
- Controllers não entram no Native Surface Runtime.
- Targets nativos são snapshots temporários; o Module não armazena referências a
  OwnedWindow.
- O conjunto de superfícies tem um único proprietário e não é dividido em
  referências mutáveis mantidas por outros Modules.

## Testes

### TDD da Interface

O primeiro teste deve falhar porque Native Surface Runtime ainda não existe. O
recording Adapter então comprova:

- build cria cinco papéis em ordem determinística;
- `Presented` não reconstrói recursos;
- device loss solicita exatamente um rebuild;
- HRESULT recuperável segue a mesma política;
- erro definitivo é preservado;
- build recuperável tenta no máximo duas vezes;
- resize de uma superfície não recria as demais;
- opacidade da dock é reaplicada depois do rebuild;
- recursos parciais são descartados antes do retry.

### Caracterização de integração

Os testes existentes de dock, topbar, preview, context menu e native slice
continuam verdes. Novos testes verificam que nenhum consumer recebe
`&mut RuntimeSurfaces` para tratar `PresentOutcome`.

### Gates

- política PowerShell de arquitetura;
- `cargo fmt --all --check`;
- Clippy estrito com todos os targets e features;
- `cargo test --workspace`;
- `cargo build --workspace --release`;
- validação nativa separada, sem transformar falhas ambientais em skips.

## Migração incremental

1. Criar teste RED da Interface e recording Adapter.
2. Implementar estado, Surface Role e normalização de resultados.
3. Mover build/rebuild e `device_kind`.
4. Migrar topbar e dock.
5. Migrar popover, context menu e settings.
6. Migrar preview e apresentação auxiliar.
7. Remover acesso direto aos seis campos antigos.
8. Absorver a política rasa de `win32_surface_present.rs`, preservando seus
   testes ou movendo-os para o novo Module.
9. Executar gates e atualizar o registro de exceções.

Cada passo deve compilar e manter a suíte hermética verde antes do próximo.

## Impacto Arquitetural

- **Modules:** novo Native Surface Runtime; RuntimeSurfaces reduzido; consumers
  de renderização adaptados.
- **Interfaces:** somente Interface interna nova; nenhuma Interface pública.
- **Estado:** renderer e superfícies passam a ter um proprietário único.
- **Dependências:** nenhuma aresta nova entre crates.
- **Thread da UI:** nenhuma operação adicional, espera ou I/O; a sequência atual
  permanece síncrona na thread proprietária dos HWNDs.
- **Diagnóstico:** eventos existentes permanecem; medições novas só serão
  adicionadas quando demonstrarem operações acima do orçamento de frame.
- **Testes:** recording Adapter e caracterização de integração.
- **Migração:** incremental dentro da crate.
- **Rollback:** cada passo pode retornar ao acesso direto anterior sem alterar
  schema, configuração ou dados do usuário.

## Critérios de sucesso

- RuntimeSurfaces não possui CompositionRenderer nem cinco WindowSurface.
- Nenhum helper de apresentação recebe `&mut RuntimeSurfaces`.
- Classificação de PresentOutcome e HRESULT vive no novo Module.
- Recovery tenta no máximo duas construções completas.
- Opacidade e animação da dock são preservadas.
- Regras de dock, preview e overlays não mudam.
- Recording Adapter cobre ordem, retry, resize e erro definitivo.
- Gates determinísticos permanecem verdes.
- Gate nativo permanece separado e visível.

## Próximos incrementos

Depois da aceitação deste incremento:

1. caracterizar e extrair Dock Runtime com preview;
2. caracterizar e extrair Overlay Runtime;
3. reduzir o coordenador a Slot Runtime;
4. aplicar o deletion test ao RuntimeOrchestrator atual.
