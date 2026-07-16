# ADR-0001: Governança e entrega incremental

- Estado: Accepted
- Data: 2026-07-15
- Responsável: mantenedor do projeto
- Relacionados: `delivery.md`, auditoria de baseline de 2026-07-15

## Contexto

O produto cresceu numa branch longa enquanto `main` permaneceu próximo da
inicialização do repositório. O snapshot atual reúne muitas alterações e
evidências sem um baseline recente que permita revisão e rollback confiáveis.

## Decisão

- `main` deve permanecer verde, executável e potencialmente publicável.
- Mudanças usam branches curtas com um único objetivo.
- O risco é classificado como S, M ou L, com exigências progressivas.
- Mudanças M/L registram Impacto Arquitetural; mudanças L exigem ADR e revisão
  arquitetural final.
- A Definition of Done e os gates são obrigatórios para integração.
- Exceções são explícitas, possuem responsável e expiram em até 14 dias por
  padrão.
- Antes de novas funcionalidades, o snapshot atual passa por estabilização de
  baseline sem perda do trabalho existente.

## Consequências

### Positivas

- Rollback por incremento e histórico revisável.
- Gates vermelhos deixam de ser dívida normalizada.
- Decisões estruturais permanecem rastreáveis.

### Custos e riscos

- A estabilização inicial adia novas funcionalidades.
- Mudanças L exigem preparação e revisão adicionais.

## Alternativas consideradas

### Manter uma branch de produto de longa duração

Rejeitada porque amplia conflitos, dificulta isolamento de regressões e torna o
rollback dependente de investigação manual.

### Exigir ADR para toda mudança

Rejeitada por criar burocracia sem ganho estrutural em alterações locais.

## Verificação

- Histórico mostra incrementos coerentes e integrados com gates verdes.
- `main` nunca permanece com gate obrigatório vermelho.
- Mudanças L referenciam ADR aceito.

## Migração e rollback

O estado atual será inventariado, separado em incrementos e estabilizado antes
de ser promovido a baseline. Nenhum arquivo preexistente será apagado ou movido
sem aprovação explícita.
