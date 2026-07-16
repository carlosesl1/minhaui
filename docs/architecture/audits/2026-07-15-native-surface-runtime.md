# Auditoria arquitetural — Native Surface Runtime

- Data: 2026-07-15
- Escopo: `shell-platform-windows` e a fronteira nativa de `shell-renderer`
- Decisão relacionada: ADR-0005
- Resultado: fronteira implementada; gates da etapa verdes

## Fronteira de ownership alcançada

`RuntimeSurfaces` possui um único campo gráfico, `surface_runtime:
Win32NativeSurfaceRuntime`. O tipo concreto `NativeSurfaceRuntime` possui o
`CompositionRenderer` e o conjunto não opcional das cinco `WindowSurface`:
topbar, dock, popover, preview e settings.

O runtime nativo:

- constrói e instala recursos de forma transacional;
- descarta superfícies antes do renderer;
- limita falhas recuperáveis de build a duas tentativas totais;
- normaliza redraw, resize, opacity e animações em `SurfaceUpdate` ou erro final;
- aplica estado transitório de dock e preview antes de instalar uma transação;
- não armazena controllers, timers, janelas Win32 ou cenas persistentes.

O `DirectCompositionAdapter` é a implementação de produção. O
`RecordingAdapter` mantém os testes independentes de uma sessão gráfica real.

## Evidência estrutural antes/depois

Antes, conforme o baseline registrado na especificação e no ADR-0005,
`RuntimeSurfaces` possuía diretamente um renderer e cinco superfícies nativas.
Além disso, `win32_surface_present.rs` recebia `&mut RuntimeSurfaces` apenas para
acionar rebuild completo.

Depois:

- `rg -n "CompositionRenderer|WindowSurface" crates/shell-platform-windows/src`
  encontra esses tipos somente em `win32_surface_runtime.rs`;
- `RuntimeSurfaces` contém `surface_runtime`, sem campos de renderer ou de
  superfície nativa;
- existe uma única definição de `rebuild_native_surfaces`, no coordenador
  `win32_owner.rs`;
- `rg -n "win32_surface_present|handle_present|physical_window_size"
  crates/shell-platform-windows/src` não encontra resultados;
- `win32_surface_present.rs` e sua declaração de módulo foram removidos.

`Test-NativeSurfaceOwnershipPolicy` transforma essa fronteira em gate. As
fixtures cobrem renderer ou surface reintroduzidos em `RuntimeSurfaces`,
ownership concreto fora do módulo autorizado, helper de rebuild acoplado ao
coordenador e uso válido da interface por role.

## Coordenação que permanece intencionalmente em RuntimeSurfaces

`RuntimeSurfaces` continua sendo o coordenador Win32. Ele recebe eventos, mantém
controllers e timers, produz cenas frescas, reúne as cinco referências de
janelas em `SurfaceWindows` e decide quando um `RebuildAllRequired` deve chamar
o rebuild completo. Essa responsabilidade não foi movida para o runtime gráfico
para evitar que recursos DirectComposition passem a possuir estado de produto
ou cenas obsoletas.

## Gates executados nesta etapa

- `cargo fmt --all -- --check`: passou;
- testes da política arquitetural: passaram;
- checkout real da política: passou para 6 crates e 7 exceções registradas;
- `cargo test -p shell-platform-windows`: passou, incluindo 85 testes unitários
  e todas as suítes de integração do pacote;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  passou.
- `cargo test --workspace --all-targets`: passou para todos os crates e targets
  herméticos;
- `cargo build --workspace --release`: passou;
- `cargo deny check`: passou em advisories, bans, licenses e sources, com cinco
  avisos não bloqueantes de licenças permitidas não encontradas no grafo atual;
- `git diff --check`: passou; os avisos emitidos tratam apenas da conversão
  futura de LF para CRLF pelo Git.

As sete exceções arquiteturais existentes continuam correspondendo a sete
`allow` reais. Nenhuma foi removida e nenhuma data de expiração foi estendida.

## Validação nativa separada

O script citado no plano, `scripts/Validate-NativeRuntime.ps1`, não existe neste
checkout. Foram executados diretamente os dois comandos equivalentes registrados
no README e em `docs/architecture/delivery.md`.

- `cargo test -p shell-platform-windows --features native-validation --lib`:
  88 passaram e 3 falharam porque Calculator/Windows Terminal e seus ícones de
  pacote não foram resolvidos neste perfil do Windows;
- `cargo test -p shell-app --features native-validation --test
  showcase_startup`: 1 falha porque o Windows recusou o registro da AppBar.

Há uma instância do MyDockFinder ativa nesta máquina. Nenhum processo do usuário
foi encerrado para tentar tornar esse gate verde. O teste combinado com
`--all-features` reproduziu apenas a mesma falha de AppBar antes de interromper a
suíte. Portanto, a validação nativa permanece vermelha por restrições ambientais
já documentadas e não é declarada como gate verde.

## Desvios e decisões preservadas

Não houve mudança durável em relação ao ADR-0005, portanto ele não foi
reescrito. A implementação evitou uma operação de replace em recurso ativo:
mudanças de métricas usam resize, pois criar um segundo target
DirectComposition para o mesmo HWND e layer antes de liberar o anterior não é
uma transação válida. Essa escolha preserva a decisão do ADR de concentrar
ownership e recuperação no módulo.

## Próximos candidatos, não implementados

- `DockRuntime`: encapsular política de visibilidade, timers, animação e
  coordenação específica da dock.
- `OverlayRuntime`: encapsular popover, menu de contexto, settings e preview com
  um contexto de janelas estreito.
- `RuntimeWindowContext`: reduzir assinaturas largas que ainda recebem as cinco
  janelas e permitir retirar as exceções temporárias correspondentes.

Esses itens são candidatos futuros; não fazem parte da implementação auditada.
