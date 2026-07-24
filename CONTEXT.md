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

## System Action Adapter

Adapter interno da plataforma Windows que traduz intents tipados de topbar e
popover para pesquisa, rotas de Settings, volume, mídia e ações de sessão. Não
possui estado de produto, não apresenta cenas e não executa espera, descoberta
de pacotes ou I/O em disco na thread da UI.

## Shell Observation

Snapshot imutável e process-wide das janelas observadas e do status da topbar.
Todos os Shell Slots recebem a mesma observação para um ciclo de sincronização,
evitando descoberta e leitura de status duplicadas por monitor.

## Shell Observation Runtime

Module interno que possui os Adapters de descoberta de janelas, identidade e
status da topbar. Aplica um único Poll Budget por processo e distribui a Shell
Observation atual aos Shell Slots. A thread da UI apenas agenda capturas e
aplica resultados imutáveis; um worker process-wide, com uma única captura
ativa e somente a solicitação pendente mais recente, possui o trabalho
potencialmente lento. Cada solicitação recebe uma geração, resultados anteriores
à geração mais recente são descartados e o wake usa a fila da thread da UI, sem
depender do lifetime de um HWND. Não possui controllers, janelas nativas ou
estado visual.

## Dock Edge Probe

Timer adaptativo do Dock Runtime usado para revelar ou ocultar a dock. Sua
cadência depende do estado: desligado quando desnecessário, lento quando o
cursor está distante, rápido perto da borda física e alinhado ao frame durante
animação.

## Lightweight Liquid Glass

Material experimental e opt-in da dock que adiciona profundidade óptica por
bitmaps Direct2D cacheados. Reutiliza a posição e a força do hover existentes,
sem captura do desktop, novo timer, blur adicional ou alteração de geometria.
O renderer possui os recursos e seleciona os modos Dynamic, Static ou Disabled
conforme hardware, movimento reduzido e fallback sólido.
