# Princípios arquiteturais

## P-001 — `main` sempre verde

`main` deve estar executável, verificável e potencialmente publicável. Gates
vermelhos impedem integração; não são normalizados como estado permanente.

## P-002 — Arquitetura verificável é arquitetura executável

Toda regra objetiva deve possuir verificação automática. Documentos registram
intenção e contexto; automação impede regressões conhecidas.

## P-003 — Privado por padrão

Interfaces públicas precisam de consumidor e finalidade claros. Estado mutável
tem um proprietário. Modules irmãos não devem compartilhar mutação irrestrita.

## P-004 — Depth antes de fragmentação

Modules devem esconder conhecimento e oferecer Interfaces pequenas. Tamanho de
arquivo, quantidade de campos e número de argumentos são sinais para revisão,
não ordens automáticas para criar arquivos, traits ou crates.

## P-005 — Abstrações precisam de variação real

Traits e Adapters existem para duas Implementações reais, para isolar tecnologia
externa ou para criar uma Seam determinística necessária. Não são criados apenas
para antecipar um futuro hipotético.

## P-006 — Responsividade é uma restrição arquitetural

A thread da UI não executa escrita em disco, descoberta de pacotes, espera de
processo ou trabalho potencialmente lento. Filas são limitadas e caminhos
quentes são medidos.

## P-007 — Observabilidade não pode degradar o produto

Logs são assíncronos, limitados, redigidos, modulares e desativados por padrão.
Sob pressão, diagnóstico pode ser descartado; a UI não pode aguardar o logger.

## P-008 — Testes na camada determinística mais baixa

Todo bug corrigido recebe regressão reproduzível. Regras puras não dependem de
Windows. Testes nativos ficam separados e declaram seus pré-requisitos.

## P-009 — Mudanças devem ser reversíveis

Persistência é versionada, migrações preservam o último estado válido e releases
indicam o commit de origem. Alterações difíceis de reverter exigem ADR.

## P-010 — O repositório contém fontes duráveis

Código, testes, fixtures e documentação durável pertencem ao Git. Logs,
binários, caches, screenshots e relatórios temporários ficam fora. Um artefato
gerado versionado deve ter finalidade e comando de reprodução documentados.
