# ADR-0004: Responsividade da UI e diagnóstico modular

- Estado: Accepted
- Data: 2026-07-15
- Responsável: mantenedor do projeto
- Relacionados: `principles.md`, ADR-0006

## Contexto

A dock executa pointer, animação e apresentação em caminhos sensíveis a frame.
Logs são necessários para diagnosticar travamentos, mas I/O ou filas sem limite
podem introduzir o mesmo problema que deveriam observar.

## Decisão

- A thread da UI não executa I/O em disco, descoberta de pacotes, espera de
  processo ou trabalho potencialmente lento.
- Filas de diagnóstico são limitadas e nunca bloqueiam o produtor da UI.
- Sob pressão, eventos de diagnóstico podem ser descartados e o descarte é
  contabilizado.
- Diagnóstico é modular, assíncrono, redigido e desativado por padrão.
- Campos e arquivos têm limites; caminhos do usuário e dados sensíveis são
  redigidos.
- Caminhos quentes registram somente operações acima do orçamento definido, não
  cada frame normal.

## Consequências

### Positivas

- Observabilidade sem acoplamento ao tempo de resposta da dock.
- Logs focados no Module investigado.
- Custos e falhas do writer ficam fora da thread da UI.

### Custos e riscos

- Parte dos eventos pode ser perdida sob pressão por decisão explícita.
- Novos trabalhos assíncronos exigem propriedade e encerramento bem definidos.

## Alternativas consideradas

### Escrita síncrona para garantir cada evento

Rejeitada porque confiabilidade do log não pode ter prioridade sobre a
responsividade do produto.

### Logging global sempre ativo

Rejeitada pelo custo, ruído e risco de registrar dados desnecessários.

## Verificação

- Testes provam fila limitada, descarte sem espera, redação e rotação.
- Mudanças em pointer, animação e renderização apresentam medição proporcional
  ao risco.
- Revisão impede I/O síncrono em callbacks da UI.

## Migração e rollback

A política duplicada foi consolidada por ADR-0006. Os writers continuam
separados e podem ser revertidos independentemente, mas redação, limites e
retenção retornam sempre ao último estado verde de `shell-diagnostics`.
