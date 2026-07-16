# Architecture Decision Records

## Quando criar

Um ADR é obrigatório para:

- nova crate ou mudança de responsabilidade entre crates;
- nova direção de dependência;
- Interface pública estruturalmente relevante;
- mudança de concorrência, processo, persistência ou recuperação;
- novo escopo de `unsafe` ou propriedade FFI;
- exceção arquitetural de alto impacto;
- decisão difícil ou cara de reverter.

Correções e funcionalidades que permanecem dentro da arquitetura vigente não
precisam de ADR.

## Estados

- `Proposed`: em discussão; não autoriza implementação estrutural.
- `Accepted`: decisão vigente.
- `Deprecated`: ainda existente, mas não deve receber expansão.
- `Superseded`: substituída por outro ADR.
- `Rejected`: considerada e não adotada.

## Fluxo

1. Copiar `0000-template.md` para o próximo número sequencial.
2. Preencher contexto, decisão, consequências e alternativas.
3. Referenciar o ADR no plano da mudança L.
4. Mudar para `Accepted` antes da integração estrutural.
5. Nunca reescrever a decisão histórica; criar sucessor quando ela mudar.

## Índice

| ADR | Estado | Decisão |
| --- | --- | --- |
| [0001](0001-governance-and-delivery.md) | Accepted | Governança e entrega incremental |
| [0002](0002-workspace-dependency-direction.md) | Accepted | Direção das dependências do workspace |
| [0003](0003-quality-gates.md) | Accepted | Gates determinísticos e validação nativa |
| [0004](0004-ui-responsiveness-and-diagnostics.md) | Accepted | Responsividade da UI e diagnóstico modular |
| [0005](0005-native-surface-runtime.md) | Accepted | Propriedade gráfica do Shell Slot |
| [0006](0006-shared-diagnostics-policy.md) | Accepted | Política compartilhada de diagnóstico |
