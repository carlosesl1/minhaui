# ADR-0003: Gates determinísticos e validação nativa Windows

- Estado: Accepted
- Data: 2026-07-15
- Responsável: mantenedor do projeto
- Relacionados: `delivery.md`, auditoria de baseline de 2026-07-15

## Contexto

A suíte atual mistura regras determinísticas com verificações que dependem de
Calculator, Windows Terminal e registro real de AppBar. O mesmo comando pode
falhar conforme a máquina, reduzindo a confiança do CI e ocultando regressões
reais entre falhas ambientais.

## Decisão

- Gates de integração usam somente testes herméticos.
- Verificações de DWM, AppBar, pacotes instalados, ícones e comportamento visual
  formam um gate nativo separado.
- O ambiente do gate nativo declara imagem, aplicativos e estado necessários.
- Gate rápido atende feedback local; gate determinístico bloqueia integração;
  gate nativo bloqueia releases afetadas por integração Windows.
- Testes dependentes da máquina não podem permanecer disfarçados como testes
  unitários herméticos.

## Consequências

### Positivas

- Falha determinística representa regressão reproduzível.
- Integração Windows continua coberta sem contaminar testes puros.
- Diagnóstico de CI fica mais rápido e confiável.

### Custos e riscos

- As Seams de descoberta de aplicativos, catálogo de pacotes e AppBar precisam
  manter Adapters Windows e fakes determinísticos alinhados.
- O gate nativo exige ambiente controlado adicional.

## Alternativas consideradas

### Ignorar testes conforme a máquina

Rejeitada porque skips silenciosos reduzem cobertura e não comprovam o contrato
nativo.

### Executar tudo no mesmo job

Rejeitada porque mistura falha de produto com variação ambiental.

## Verificação

- A suíte hermética passa sem pressupor aplicativos instalados.
- `AppDiscoveryAdapter`, `PackageCatalog` e `AppBarAdapter` exercitam fallback,
  resolução de ícone e ciclo de reserva sem consultar o estado real da máquina.
- O smoke de bootstrap executa por padrão; o showcase com AppBar real permanece
  restrito ao feature `native-validation`.
- O gate nativo lista pré-requisitos e produz resultado separado.
- Nenhuma exceção ou teste ignorado existe sem expiração registrada.

## Migração e rollback

Os testes ambientais estão rotulados pelo feature `native-validation` e as
Seams determinísticas possuem Adapters fake. O rollback restaura as chamadas
diretas aos Adapters Windows e remove apenas os testes herméticos correspondentes;
o gate nativo não é desativado nem convertido em skip condicional.
