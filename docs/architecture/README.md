# Governança de arquitetura

Este diretório é a fonte oficial de verdade da arquitetura do Windows Native
Dock. Ele descreve o estado atual desejado; os ADRs registram por que decisões
duráveis foram tomadas.

## Ordem de autoridade

Quando houver divergência, vale a seguinte ordem:

1. código e manifests verificados pelos gates;
2. ADRs aceitos;
3. regras atuais deste diretório;
4. documentação geral do projeto;
5. planos e evidências históricas.

Uma divergência entre código e documentação é um defeito. A correção deve
atualizar ambos ou registrar uma exceção temporária explícita.

## Documentos

- [Contexto de domínio](../../CONTEXT.md): linguagem comum do produto e dos
  runtimes nativos.
- [Glossário](glossary.md): linguagem comum e significado dos termos.
- [Princípios](principles.md): restrições permanentes de crescimento.
- [Regras de dependência](dependency-rules.md): responsabilidades e direções
  permitidas entre crates.
- [Entrega](delivery.md): branches, classificação de risco, Definition of Done,
  exceções, releases e higiene.
- [ADRs](adr/README.md): decisões estruturais e seu histórico.
- [Auditorias](audits/2026-07-15-baseline.md): fotografias factuais do estado do
  repositório e o [checkpoint de consolidação](audits/2026-07-15-baseline-consolidation.md),
  sem substituir regras atuais.

## Regra central

`main` deve estar sempre verde, executável e potencialmente publicável. Toda
mudança entra por um incremento coerente e somente depois de satisfazer os gates
proporcionais ao seu risco.

## Manutenção

- Alterações locais dentro das regras existentes atualizam o documento afetado.
- Decisões estruturalmente relevantes exigem ADR.
- Regras objetivamente verificáveis devem migrar para automação no CI.
- Exceções nunca são silenciosas nem permanentes.
- Este diretório não armazena logs, binários, screenshots temporários ou saídas
  geradas de QA.
