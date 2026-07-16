# Glossário

## Adapter

Implementação que traduz uma Interface do projeto para uma tecnologia externa,
como Win32, DirectComposition, filesystem ou processo do Windows.

## ADR

Architecture Decision Record. Registro imutável de contexto, decisão,
consequências e alternativas de uma escolha estrutural. Um ADR substituído não
é apagado; recebe o estado `Superseded` e aponta para o sucessor.

## Baseline

Commit limpo, reproduzível e verificado que serve como ponto confiável de
comparação, integração e rollback.

## Depth

Quanto conhecimento ou complexidade uma Interface simples esconde. Um Module
profundo oferece grande Leverage sem exigir que seus consumidores conheçam a
Implementação interna.

## Gate

Verificação objetiva que precisa passar antes de uma integração ou release.

## Implementação

Detalhes privados que realizam o comportamento prometido por uma Interface.

## Interface

Superfície pela qual um Module pode ser usado. Deve ser menor e mais estável que
sua Implementação.

## Leverage

Quantidade de comportamento confiável obtida por uma pequena Interface ou
mudança localizada.

## Locality

Proximidade entre uma regra, seu estado, sua Implementação e seus testes.
Mudanças com boa Locality exigem navegar e alterar poucas regiões coerentes.

## Module

Unidade com responsabilidade, estado e invariantes próprios. Um arquivo só é um
Module quando oculta conhecimento; dividir arquivos sem dividir responsabilidade
não cria novos Modules.

## Seam

Ponto controlado onde uma Implementação pode ser substituída ou observada, como
um Adapter real e um fake determinístico para testes.

## Teste hermético

Teste cujo resultado não depende de aplicativos instalados, janelas abertas,
estado atual da AppBar, rede, horário ou configuração específica da máquina.

## Validação nativa

Verificação executada em ambiente Windows controlado para comportamentos que não
podem ser provados por testes herméticos, como DWM, AppBar e pacotes instalados.
