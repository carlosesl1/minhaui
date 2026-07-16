# ADR-0006: Política compartilhada de diagnóstico

- Estado: Accepted
- Data: 2026-07-15
- Responsável: mantenedor do projeto
- Relacionados: ADR-0002, ADR-0004, `dependency-rules.md`

## Contexto

O app e o watchdog possuíam Implementações próprias de redação e retenção. As
duas reconheciam chaves sensíveis e caminhos de perfil, mas divergiam em caixa,
limites de campos, tamanho de valores e estratégia de retenção. Essa duplicação
permitia que uma correção de privacidade alcançasse apenas um processo.

Os writers têm necessidades diferentes: a plataforma usa fila limitada e um
worker assíncrono para proteger a thread da UI; o watchdog exporta JSON fora do
caminho de frame. Compartilhar I/O reduziria a Locality desses Adapters.

## Decisão

- A crate `shell-diagnostics`, sem dependências locais, possui o Diagnostics
  Policy puro.
- `DiagnosticPolicy` define redação case-insensitive, máximo de oito campos e
  máximo de 1.024 caracteres por valor.
- `RetentionPolicy` define aceitação de registros, rotação por bytes e seleção
  dos segmentos válidos mais recentes.
- A política padrão limita cada arquivo a 4 MiB e mantém um segmento, preservando
  o comportamento atual do writer da plataforma.
- `shell-platform-windows` continua responsável por fila, descarte, worker,
  formato textual, flush e caminho do arquivo.
- `shell-watchdog` continua responsável pelo Adapter JSON e pelo destino de
  exportação.
- Um gate impede que redação ou tipos de política de retenção voltem a ser
  implementados nos dois Adapters.

## Consequências

### Positivas

- Uma correção de privacidade passa a proteger app e watchdog simultaneamente.
- A Interface compartilhada é pequena e oferece Leverage para dois Adapters.
- I/O e concorrência mantêm Locality nas crates que realmente os possuem.
- Testes puros cobrem a política sem filesystem, thread ou sessão Windows.

### Custos e riscos

- O workspace ganha uma crate e duas novas arestas de dependência.
- Alterações na política padrão afetam os dois processos e exigem testes de
  compatibilidade nos Adapters.

## Alternativas consideradas

### Colocar a política em `shell-core`

Rejeitada porque redação e retenção são preocupações operacionais, não estado ou
regra de domínio do shell.

### Compartilhar o writer completo

Rejeitada porque fila assíncrona e exportação JSON possuem ciclos de vida e
formatos diferentes. Isso criaria uma Interface mais larga sem aumentar Depth.

### Manter duas Implementações sincronizadas por revisão

Rejeitada porque revisão manual não impede divergência futura de privacidade.

## Verificação

- Testes de `shell-diagnostics` cobrem redação, limites e retenção.
- Testes do watchdog cobrem caixa mista e limites compartilhados.
- Testes da plataforma cobrem fila limitada, writer e rotação usando a política.
- `Test-DiagnosticsPolicyOwnership` valida o proprietário único.

## Migração e rollback

Os Adapters preservam seus formatos e fluxos de I/O. Um rollback pode restaurar
o último commit verde da política sem mover ownership dos writers. Não é aceito
copiar novamente a Implementação para as crates consumidoras.
