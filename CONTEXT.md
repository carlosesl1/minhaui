# Contexto de domínio

Este documento nomeia conceitos do Windows Native Dock. Termos gerais de
arquitetura como Module, Interface, Adapter, Seam, Depth e Locality estão em
[`docs/architecture/glossary.md`](docs/architecture/glossary.md).

## Shell Slot

Conjunto do shell associado a um monitor. Possui as janelas nativas de topbar,
dock, popover, preview e settings, além do runtime que coordena esse conjunto.

## Surface Role

Papel visual de uma superfície nativa: topbar, dock, popover, preview ou
settings. O papel define qual cena pode ser apresentada; ele não transfere
propriedade da janela ou do controller.

## Native Surface Runtime

Module interno que possui o renderer de DirectComposition e todas as superfícies
de um Shell Slot. É responsável por create, resize, present, opacidade, animação,
device loss e rebuild. Não possui controllers nem armazena cenas antigas.

## Slot Runtime

Coordenador de um Shell Slot. Roteia eventos entre runtimes de domínio e o Native
Surface Runtime. Deve permanecer fino: coordena ordem, mas não reimplementa as
regras internas de dock, overlays ou renderer.

## Dock Runtime

Module planejado que possuirá DockController, preview, timers, motion de
visibilidade, edge reveal e regras de fullscreen relacionadas à experiência da
dock.

## Overlay Runtime

Module planejado que possuirá popovers, menu de contexto e settings. Esses
elementos compartilham exclusão mútua, placement e superfícies transitórias.

## Surface Frame

Cena atual e métricas necessárias para criar, redimensionar ou apresentar um
Surface Role. Um Surface Frame é produzido sob demanda pelo dono do estado de
domínio e nunca é mantido como fonte de verdade pelo Native Surface Runtime.

## Surface Build Plan

Conjunto temporário dos cinco Surface Frames e targets nativos necessários para
construir ou reconstruir um Shell Slot de forma consistente.

## Device Loss

Perda recuperável do device gráfico, incluindo device removed, device reset e
recreate target. Ela invalida renderer e superfícies do Shell Slot e solicita um
rebuild completo com cenas frescas.

## Recording Adapter

Adapter determinístico usado por testes do Native Surface Runtime. Registra a
ordem de create, resize, present, opacity, animation e drop sem depender de HWND,
DWM ou GPU reais.

## Diagnostics Policy

Module puro compartilhado que define redação, limites de campos e retenção dos
diagnósticos. Não conhece formato de arquivo, thread, fila ou filesystem. O app e
o watchdog mantêm writers próprios como Adapters dessa política.
