# ADR-0008: Catálogo de aplicativos da área de notificação sob demanda

- Estado: Accepted
- Data: 2026-07-16
- Responsável: platform-overlays
- Relacionados: ADR-0002, ADR-0004, ADR-0005, ADR-0007,
  `docs/superpowers/specs/2026-07-16-background-apps-topbar-design.md`

## Contexto

A barra superior precisa listar os aplicativos que o Explorer conhece na área
de notificação. `Shell_NotifyIcon` permite que cada processo gerencie o próprio
ícone, mas não oferece enumeração pública global. Automatizar a janela interna
da tray não encontra de forma confiável o overflow fechado e reproduzir
callbacks privados do Explorer seria frágil e inseguro.

A solução deve preservar a responsividade da UI, não criar polling adicional,
não registrar dados privados dos aplicativos e falhar sem encerrar o shell caso
uma versão futura do Windows remova o contrato interno utilizado.

## Decisão

- `BackgroundApps` é um módulo runtime-only de `shell-core`, fixo antes do
  relógio e ausente do schema V1 persistido.
- O catálogo puro cruza no máximo 256 registros com processos ativos da sessão,
  remove duplicatas, limita a saída a 32 itens e expõe somente identidade,
  rótulo, fonte de ícone e dados necessários à ativação.
- Um Adapter privado de `shell-platform-windows` lê somente
  `HKCU\Control Panel\NotifyIconSettings` e enumera processos com ToolHelp. O
  registro é um contrato interno do Explorer e não é tratado como API pública.
- A captura ocorre somente ao abrir o popover, em um worker pertencente ao
  `Shell Slot`. Cada worker executa no máximo uma captura por vez e conserva
  somente a solicitação pendente mais recente; reaberturas intermediárias são
  coalescidas sem criar novas threads.
- O worker é encerrado e aguardado antes dos HWNDs do `Shell Slot` serem
  destruídos. O resultado volta pela fila nativa existente e um `PostMessageW`
  sem payload apenas acorda o loop da UI; se o wake falhar, o resultado é
  removido da fila em vez de permanecer órfão.
- A geração do carregamento invalida resultados atrasados após reabertura ou
  fechamento. Não existe timer nem segunda descoberta de janelas.
- A abertura de uma linha primeiro reutiliza a última descoberta centralizada
  de janelas. Sem janela elegível, o Adapter revalida o caminho do mesmo
  processo ativo antes de executar o arquivo.
- Não são reproduzidos callbacks privados de ícones da tray. O clique
  abre/focaliza o aplicativo, que é o comportamento estável oferecido pela UI.
- Caminhos, tooltips e identificadores de processo não são registrados. Falhas
  expõem apenas códigos fechados e mantêm o shell em execução.
- Ícones são carregados exclusivamente pelo cache já pertencente ao renderer;
  controller, layout e worker não criam recursos gráficos.

## Consequências

### Positivas

- Custo zero de polling quando o menu está fechado.
- Trabalho de registro, processos e ícones não bloqueia a thread da UI.
- O Adapter, a concorrência e os recursos nativos permanecem privados e locais.
- A concorrência fica limitada pelo número de `Shell Slots`, com uma captura
  ativa e uma solicitação coalescida por slot.
- A remoção futura do contrato interno degrada para estado indisponível.

### Custos e riscos

- `NotifyIconSettings` pode mudar entre versões do Windows.
- Cada `Shell Slot` mantém uma thread ociosa enquanto existe, em troca de
  propriedade explícita, encerramento determinístico e ausência de tempestade
  de threads durante reaberturas.
- O catálogo representa registros ativos por executável, não callbacks ou menus
  privados de cada ícone.
- Aplicativos sem registro reconhecível ou sem processo ativo não aparecem.

## Alternativas consideradas

### Automatizar a UI interna da área de notificação

Rejeitada porque o overflow fechado não é enumerado de forma confiável e a
estrutura visual do Explorer não é um contrato estável.

### Ler barras de ferramentas e enviar callbacks privados

Rejeitada por depender de estruturas internas, memória de outro processo e
mensagens sem contrato público.

### Enumerar todos os processos continuamente

Rejeitada porque inclui itens que não pertencem à área de notificação e adiciona
custo ocioso permanente.

## Verificação

- testes puros de filtro, deduplicação, rótulo, ordem e limites;
- testes de geração, descarte de resultado atrasado, ação e scroll;
- testes de viewport e fonte opcional de ícone no renderer;
- testes do módulo da topbar e da descoberta centralizada de janelas;
- `cargo check`, Clippy, testes do workspace e gate de arquitetura;
- smoke nativo separado para presença, limites, rolagem e custo sob demanda.

## Migração e rollback

Não há migração persistida. O rollback remove o módulo runtime-only, worker,
Adapter e variante de popover. Arquivos V1 continuam idênticos. Se o registro
deixar de funcionar, o estado indisponível é aceitável até a remoção ou
substituição do Adapter por uma API pública equivalente.
