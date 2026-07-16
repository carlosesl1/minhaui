# ADR-0002: Direção das dependências do workspace

- Estado: Accepted
- Data: 2026-07-15
- Responsável: mantenedor do projeto
- Relacionados: `dependency-rules.md`

## Contexto

A divisão atual em crates possui direção acíclica saudável, mas essa intenção
existe principalmente no README e nos manifests atuais. Sem regra executável,
novas funcionalidades podem introduzir dependências reversas ou transformar a
plataforma Windows em destino de regras não nativas.

## Decisão

- `shell-core` não depende de outra crate local.
- `shell-config` depende apenas de `shell-core`.
- `shell-renderer` depende apenas de `shell-core` entre crates locais.
- `shell-platform-windows` pode depender de `shell-core`, `shell-config` e
  `shell-renderer`.
- `shell-app` depende de `shell-platform-windows` e permanece composição.
- `shell-watchdog` depende apenas de `shell-core` entre crates locais.
- Nova aresta, nova crate ou mudança de responsabilidade exige ADR.
- A regra será validada automaticamente a partir de `cargo metadata`.
- A fachada padrão de `shell-platform-windows` expõe somente o ponto de entrada
  executável (`run_showcase`), sua configuração (`ShowcaseRunConfig`) e a
  identidade da crate.
- Controllers, ações, roteamento e `PlatformEvent` são detalhes de
  Implementação da plataforma. Seus testes são compilados como Modules internos
  sob `cfg(test)` e não justificam ampliar a Interface pública.
- `shell-renderer` publica somente o contrato usado pela plataforma e os tipos
  que fazem parte das assinaturas desse contrato. Tipos de inspeção usados
  apenas por testes ficam privados ou condicionados a `cfg(test)`.
- Geometria continua pertencendo ao renderer. A plataforma pode consumi-la
  internamente, mas não a reexporta em sua fachada.
- Um gate baseado nas declarações de `lib.rs` impede novos itens públicos fora
  das listas aprovadas para plataforma e renderer.

## Consequências

### Positivas

- Domínio puro e testável sem Windows.
- Fluxo de dependência previsível.
- Menor superfície de impacto para mudanças nativas.
- Refatorações internas deixam de quebrar consumidores inexistentes.
- A fronteira entre plataforma e renderer fica explícita e executável.

### Custos e riscos

- Código colocado na crate errada precisará ser movido antes da integração.
- Casos legítimos novos exigirão ADR em vez de uma dependência conveniente.
- Um novo item público exige consumidor demonstrado e atualização deliberada do
  gate de fachada.

## Alternativas consideradas

### Permitir dependências conforme a necessidade de cada feature

Rejeitada porque conveniência local cria ciclos conceituais e acoplamento
crescente.

## Verificação

- Gate de arquitetura sobre `cargo metadata`.
- `Test-PublicFacadePolicy` valida as fachadas de
  `shell-platform-windows/src/lib.rs` e `shell-renderer/src/lib.rs`.
- Os testes da política cobrem fachadas válidas e exposições indevidas.

## Migração e rollback

A direção e as fachadas atuais satisfazem as regras automatizadas. Em rollback,
os testes podem voltar a targets externos sem tornar controllers públicos: deve
ser criado um suporte de teste explícito e condicionado, nunca uma reexportação
geral da Implementação.
