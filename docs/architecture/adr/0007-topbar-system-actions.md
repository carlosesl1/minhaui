# ADR-0007: Ações de sistema da topbar por intents puros e Adapter Windows

- Estado: Accepted
- Data: 2026-07-16
- Responsável: platform-overlays
- Relacionados: ADR-0002, ADR-0003, ADR-0004, ADR-0005,
  `docs/superpowers/specs/2026-07-16-priority-one-topbar-design.md`

## Contexto

A topbar já apresenta relógio, rede, áudio, energia e notificações, mas somente
abrir Settings possui efeito nativo. Os demais itens são fixtures ou intents
descartados. A primeira prioridade exige pesquisa, identidade do aplicativo
ativo, comandos de mídia, rotas de configuração e ações de sessão.

Essa entrega altera persistência, o contrato puro entre controller e renderer e
adiciona novo uso de FFI/integração Windows. Ela não pode mover regras para o
renderer, acoplar controllers ao `NativeSurfaceRuntime`, ampliar a fachada da
plataforma nem executar trabalho potencialmente lento na thread da UI.

## Decisão

- `shell-core` possui os tipos puros de módulo, ordem, visibilidade e
  `TopbarIntent`.
- `shell-config` mantém o contrato V1 existente e persiste somente os módulos
  compatíveis com esse schema. `AppIdentity` e `Search` são inseridos em runtime
  como módulos fixos, sem alterar o formato já persistido.
- `shell-renderer` continua proprietário de cenas, layout, bounds e hit testing.
  O contrato público existente pode carregar `TopbarIntent`, mas novos tipos de
  conveniência não são reexportados apenas para a plataforma.
- `TopbarController` e `PopoverController` transformam entrada em intents
  tipados. Eles não chamam Win32.
- Um Adapter interno de ações de sistema em `shell-platform-windows` traduz
  intents para rotas `ms-settings:`, pesquisa do Windows, volume, teclas globais
  de mídia, topologias documentadas de projeção e ações de sessão.
- Operações `unsafe` permanecem locais ao recurso nativo, documentam invariantes
  e expõem uma Interface segura e privada à crate.
- O Slot Runtime coordena ordem, exclusão mútua e redraw. Ele não reimplementa
  políticas internas dos controllers.
- Mudanças de topologia criam ou reutilizam os novos Shell Slots antes de
  remover slots obsoletos. Assim, a contagem de HWNDs nunca chega a zero durante
  uma projeção e o loop de mensagens não interpreta a reconciliação como saída.
- O `NativeSurfaceRuntime` permanece exclusivamente gráfico: não recebe intents,
  não executa comandos do Windows e não armazena cenas ou estado de produto.
- O Adapter não espera processos, não faz I/O em disco, não executa descoberta
  de pacotes e não bloqueia em operações WinRT assíncronas na thread da UI.
- GSMTC, mudança direta de rádio Bluetooth/Wi-Fi e brilho DDC/CI ficam fora deste
  incremento. Mídia usa comandos globais; Bluetooth, foco, brilho e dispositivos
  de saída usam rotas documentadas do Windows.
- Ações destrutivas só atravessam o Adapter após a confirmação tipada já
  existente. Testes herméticos nunca executam lock, sleep, sign out, restart ou
  shutdown.

## Consequências

### Positivas

- Regras e composição do estado permanecem puras e testáveis sem Windows.
- Integração nativa ganha Locality em um Adapter seguro.
- O renderer mantém Depth sem acumular política de sistema.
- A propriedade gráfica definida pelo ADR-0005 não muda.
- O incremento pode ser revertido sem alterar a versão do schema.

### Custos e riscos

- `TopbarIntent` passa a fazer parte do contrato entre `shell-core`,
  `shell-renderer` e a plataforma.
- Novas chamadas nativas exigem validação Windows separada.
- A UI de listas é funcional, mas não oferece ainda slider, tiles, artwork ou
  grade visual de calendário.
- Rotas do Windows podem estar indisponíveis em edições antigas; falhas precisam
  ser tratadas sem encerrar o shell.

## Alternativas consideradas

### Executar comandos diretamente no renderer

Rejeitada porque mistura geometria/apresentação com política e tecnologia
Windows, viola ADR-0002 e reduz Locality.

### Colocar comandos no Native Surface Runtime

Rejeitada porque contradiz ADR-0005: o Module gráfico não possui controllers nem
estado ou efeitos de produto.

### Criar traits públicos para cada recurso

Rejeitada por ampliar a Interface sem duas Implementações reais. Seams privadas
e fakes determinísticos são usados somente quando necessários aos testes.

### Implementar GSMTC, rádios e brilho já neste incremento

Rejeitada porque adicionaria capabilities de pacote, consentimento e trabalho
assíncrono/process-global, ampliando o risco e o escopo além da prioridade
funcional.

## Verificação

- `powershell -NoProfile -File scripts/Check-Architecture.ps1`;
- testes puros de reducer, schema/migração, layout, controller e calendário;
- testes de classificação/mapeamento do Adapter sem efeitos destrutivos;
- `cargo check` do pacote Windows e testes focados nas unidades alteradas;
- gate `native-validation` separado para pesquisa, rotas, leitura de status e
  input de mídia em ambiente Windows controlado;
- busca estrutural confirma que `NativeSurfaceRuntime` não conhece intents ou
  Adapters de ações do sistema.

## Migração e rollback

A adoção ocorre em etapas: modelo/configuração, intents/layout, dados dinâmicos,
Adapter nativo e integração. Cada etapa mantém gates focados verdes. O rollback
remove os módulos runtime-only e seus handlers; documentos V1 anteriores
continuam legíveis sem migração e o último arquivo válido permanece protegido
pelo `ConfigStore`.
