# ADR-0005: Native Surface Runtime como proprietário gráfico do Shell Slot

- Estado: Accepted
- Data: 2026-07-15
- Responsável: platform-runtime
- Relacionados: ADR-0002, ADR-0004,
  `docs/superpowers/specs/2026-07-15-native-surface-runtime-design.md`

## Contexto

`RuntimeSurfaces` possui renderer, cinco superfícies, controllers, timers,
preview e coordenação de eventos. A apresentação e recuperação gráfica estão
distribuídas entre vários arquivos que implementam métodos no mesmo tipo. Device
loss, ausência de recursos e HRESULT recuperável são tratados repetidamente, e
helpers de apresentação recebem o coordenador completo apenas para reconstruir
recursos.

Essa estrutura reduz Locality, amplia o impacto de mudanças e mantém sete
exceções temporárias de Clippy relacionadas à concentração de responsabilidade.

## Decisão

Extrair incrementalmente um Module interno `NativeSurfaceRuntime` que:

- possui CompositionRenderer e as superfícies topbar, dock, popover, preview e
  settings;
- oferece Interface por Surface Role para build, rebuild, present, resize,
  opacity e animation;
- normaliza apresentação em Presented, RebuildAllRequired ou erro definitivo;
- descarta superfícies antes do renderer;
- constrói um conjunto novo de forma transacional;
- tenta novamente uma única vez após falha recuperável de build;
- não possui controllers, timers, janelas ou cenas persistentes;
- usa DirectComposition Adapter em produção e recording Adapter nos testes.

Controllers produzem cenas frescas antes de chamar o Module. Recovery completo é
solicitado pelo resultado normalizado e executado depois que o empréstimo mutável
do Native Surface Runtime termina.

## Consequências

### Positivas

- Um proprietário para renderer e superfícies.
- Política de device loss e HRESULT concentrada.
- Testes determinísticos de ordem, retry e rollback parcial.
- Menor Interface do coordenador e maior liberdade para extrair Dock Runtime e
  Overlay Runtime posteriormente.

### Custos e riscos

- O primeiro incremento toca vários consumers de renderização.
- Surface Frames temporários precisam respeitar lifetimes das cenas.
- Recording Adapter adiciona uma Interface interna que deve permanecer menor que
  a Implementação nativa.
- O coordenador ainda produzirá cenas completas durante rebuild até os próximos
  runtimes serem extraídos.

## Alternativas consideradas

### Reescrever diretamente como Slot Runtime

Rejeitada por combinar muitos domínios num único diff e aumentar risco de
regressão e conflitos de empréstimo.

### Agrupar apenas referências de janelas

Rejeitada como solução arquitetural porque não transfere propriedade nem
conhecimento e falha no deletion test.

### Armazenar as últimas cenas dentro do runtime nativo

Rejeitada porque recovery poderia reapresentar estado obsoleto e criaria
dependência do runtime gráfico sobre controllers.

## Verificação

- Testes com recording Adapter.
- Busca estrutural prova ausência dos seis campos antigos em RuntimeSurfaces.
- Nenhum helper de apresentação recebe `&mut RuntimeSurfaces`.
- Gates determinísticos definidos em `docs/architecture/delivery.md`.
- Validação nativa permanece separada pelo feature `native-validation`.

## Migração e rollback

A migração ocorre por consumer: build/rebuild, topbar/dock, overlays e preview.
Cada etapa mantém o workspace compilável. Não há mudança de schema ou dados. O
rollback consiste em restaurar o consumer anterior e remover o método ainda não
adotado do Native Surface Runtime.
