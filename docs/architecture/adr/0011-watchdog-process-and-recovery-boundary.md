# ADR-0011: Watchdog como launcher, supervisor e limite de recuperação

- Estado: Accepted
- Data: 2026-08-12
- Responsável: windows-recovery
- Relacionados: ADR-0002, ADR-0003, ADR-0004

## Contexto

O binário `shell-watchdog` existia como simulador, enquanto o pacote iniciava
`shell-app` diretamente. Com isso, uma falha abrupta não tinha processo externo
para detectar travamento, limitar crash loops ou preservar informação de
recuperação. Essa lacuna é crítica porque o shell possui uma experiência
experimental que pode alterar a taskbar do Explorer e o perfil de release usa
`panic = "abort"`.

Supervisão de processo e journal crash-safe são responsabilidades estruturais,
não apenas uma correção local. A restauração Win32 também exige um escopo FFI
independente do processo que pode falhar.

## Decisão

- `shell-watchdog` é o entrypoint empacotado e inicia o `shell-app` irmão.
- O filho recebe a flag interna `--watchdog-child` e publica heartbeats
  versionados por stdout. Escrita ocorre numa thread dedicada; a UI apenas usa
  `try_send` e nunca bloqueia em I/O.
- O watchdog monitora startup e heartbeat com deadlines limitados, tolera um
  salto de agendamento compatível com suspensão, termina filhos sem resposta,
  aplica backoff e tenta modo seguro uma vez após um crash loop.
- Um único heartbeat não zera o orçamento de falhas; somente uma janela saudável
  sustentada o faz.
- O journal de recuperação é versionado e limitado a 256 KiB. A criação
  `Prepared` grava e sincroniza staging no mesmo diretório e publica com
  create-no-overwrite; journal pendente, futuro ou corrompido nunca é
  substituído por uma nova transação. A transição `Prepared` -> `Applied` ocorre
  sob lock de arquivo e exige o `RecoveryTransactionId` esperado antes da
  substituição atômica. No Windows, a publicação usa `MoveFileExW` com
  `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`, porque `std::fs::rename`
  não substitui o `Prepared` existente nesse alvo. Dados futuros, corruptos,
  grandes demais ou com `txid`
  divergente falham fechados e são preservados.
- A conclusão da restauração faz compare-and-delete sob o mesmo lock: ausência
  é sucesso idempotente, mas um journal com outro `RecoveryTransactionId` é
  preservado. Assim, um restaurador atrasado não apaga uma transação mais nova.
- A restauração nativa é um Adapter privado do watchdog. Todo `unsafe` fica em um
  único módulo `cfg(windows)`, com invariantes por chamada. Regras, journal,
  supervisão e planejamento continuam seguros e testáveis fora do Windows.
- O Adapter V1 redescobre taskbars apenas por classe, imagem confiável do
  `explorer.exe`, device do monitor e bounds; HWND nunca é persistido. Contagem
  divergente, ambiguidade, topologia alterada ou processos owner distintos
  interrompem a recuperação e preservam o journal.
- O preflight V1 só remove a região vazia que a mutação experimental conhecida
  cria. Região customizada e o retorno ambíguo `ERROR` de `GetWindowRgn` não são
  interpretados como estado restaurável. Owner de autohide diferente do Explorer
  também interrompe a operação. Esses estados exigem snapshot/preflight V2.
- Após `SetWindowRgn(NULL)`, a restauração usa `ABM_SETSTATE`, owner autohide por
  monitor e `ShowWindowAsync(SW_SHOWNOACTIVATE)`. O journal só é removido depois
  de polling limitado comprovar topologia, visibilidade, AppBar state, owner e
  work area de todos os monitores; qualquer dúvida mantém o arquivo.
- Como `ABM_GETSTATE` deixou de reportar `ABS_ALWAYSONTOP` desde o Windows 7,
  V1 aceita apenas o bit verificável `ABS_AUTOHIDE`; estados com outros bits
  falham no planner em vez de declarar uma verificação que a API não oferece.
- A experiência que substitui a taskbar continua desabilitada em builds estáveis.
  Compilar o feature experimental não basta: uma segunda confirmação explícita
  é necessária até journal, mutação e restauração formarem uma transação nativa
  integralmente validada.

## Consequências

### Positivas

- O processo que monitora e restaura não compartilha o destino de falha da UI.
- Travamento, crash loop e recuperação passam a ter políticas limitadas e
  observáveis.
- Os scripts de update/uninstall expõem um contrato real `--restore-only`, sem
  simulação que pudesse reportar sucesso falso. O wiring automático em cada
  instalador/pipeline ainda é um gate de empacotamento.
- O journal preserva evidência quando a restauração não pode ser comprovada.

### Custos e riscos

- O pacote contém dois executáveis e precisa validar a presença de ambos.
- Stdout do filho vira um protocolo interno; frames são limitados e versionados
  para não permitir crescimento de memória.
- FFI no watchdog é uma exceção intencional à regra anterior de crate totalmente
  segura e exige validação em Windows real.
- A primeira versão não deve habilitar mutação da taskbar apenas porque a leitura
  do journal existe; arming, aplicação, verificação e restore precisam ser uma
  transação completa.
- Journal V1 não registra região anterior, owner anterior, edge claim ou work
  area anterior. Recuperação V1 é deliberadamente estreita; journal V2 e gates
  nativos são obrigatórios antes de habilitar replacement fora de imagem de teste.

## Alternativas consideradas

### Manter `shell-app` como entrypoint e iniciar watchdog depois

Rejeitada porque existe uma janela sem supervisor e porque o watchdog iniciado
pelo filho não é um pai confiável para possuir o lifecycle.

### Restaurar taskbar dentro de `shell-platform-windows`

Rejeitada porque uma falha do processo principal elimina junto o mecanismo de
recuperação.

### Considerar o arquivo do journal suficiente para habilitar replacement

Rejeitada porque persistência sem Adapter Win32 verificado apenas registra o
problema; não restaura o estado do Explorer.

## Verificação

- Testes de parser/framing, oversize, timeout, crash loop e janela saudável.
- Testes do journal para round-trip, criação concorrente sem overwrite, staging
  abandonado por crash, transição autenticada por `txid`, compare-and-delete
  resistente a ABA, corrupção, schema futuro, oversize e remoção idempotente.
- Testes puros do planner para vazio, duplicidade, mismatch e preservação do
  journal, além da política que permite `unsafe` somente no Adapter Win32.
- `cargo check` e Clippy com todos os targets/features para Windows.
- CI Windows com build release MSVC/C++.
- Gate nativo separado deve matar/travar o filho em cada fase da transação e
  verificar região, visibilidade, AppBar state e work area antes de liberar o
  feature experimental.

## Migração e rollback

O manifest passa a iniciar `shell-watchdog.exe`; executar `shell-app.exe`
diretamente continua disponível como diagnóstico. Um rollback de supervisão
restaura temporariamente o entrypoint anterior, mas não autoriza a substituição
experimental da taskbar. Journal desconhecido nunca é apagado por rollback.
