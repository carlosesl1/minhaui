# Regras de dependência

## Direção permitida

```text
shell-core
├── shell-config
├── shell-renderer
├── shell-platform-windows
│   └── shell-app
└── shell-watchdog

shell-diagnostics
├── shell-platform-windows
└── shell-watchdog
```

A ilustração mostra direção conceitual, não todas as arestas atuais. As arestas
permitidas do workspace são:

| Crate | Pode depender de crates locais |
| --- | --- |
| `shell-core` | nenhuma |
| `shell-config` | `shell-core` |
| `shell-diagnostics` | nenhuma |
| `shell-renderer` | `shell-core` |
| `shell-platform-windows` | `shell-core`, `shell-config`, `shell-diagnostics`, `shell-renderer` |
| `shell-app` | `shell-platform-windows` |
| `shell-watchdog` | `shell-core`, `shell-diagnostics` |

Qualquer nova aresta, nova crate ou inversão exige ADR antes da implementação.

## Responsabilidades

### `shell-core`

- Tipos de domínio, estado, eventos e reducers puros.
- Sem Win32, renderer, filesystem, processo, relógio ou rede.
- Sem `unsafe`.

### `shell-config`

- Schema versionado, validação, migração e persistência atômica.
- Não contém interação de UI nem chama Win32.
- Sem `unsafe`.

### `shell-renderer`

- Geometria, cenas e Implementação de D3D11/DirectComposition.
- Não controla o ciclo de vida da aplicação ou políticas da dock.
- Uso de `unsafe` fica isolado por recurso nativo e expõe Interface segura.

### `shell-diagnostics`

- Política pura de privacidade, limites e retenção de diagnósticos.
- Não executa I/O, não cria threads e não conhece formato de serialização.
- Writers do app e watchdog permanecem Adapters nas crates proprietárias.
- Sem dependências locais e sem `unsafe`.

### `shell-platform-windows`

- Adapters Win32, entrada nativa e coordenação das superfícies Windows.
- Regras puras reutilizáveis não devem nascer aqui por conveniência.
- Cada operação `unsafe` documenta invariantes e propriedade do recurso.

### `shell-app`

- Composição, argumentos de inicialização e início do processo principal.
- Não acumula regra de domínio nem detalhes de renderização.
- Sem `unsafe`.

### `shell-watchdog`

- Supervisão, recuperação e restauração independente do processo principal.
- Não depende da plataforma ou do renderer.
- Regras, journal e supervisão permanecem sem `unsafe`.
- O Adapter privado de restauração Win32 pode usar `unsafe` somente no módulo
  isolado aprovado pelo ADR-0011, com invariantes documentadas e testes nativos.

## Interface pública

- Tudo é privado por padrão.
- Reexports no `lib.rs` fazem parte da Interface da crate e exigem consumidor.
- Tipos Win32 concretos não atravessam crates sem necessidade demonstrada.
- Campos mutáveis compartilhados entre Modules indicam responsabilidade sem
  proprietário e exigem revisão.

## Automação planejada

Um gate baseado em `cargo metadata` deve validar as arestas permitidas. Gates
adicionais devem verificar crates autorizadas a usar `unsafe` e detectar novas
exposições nativas públicas. Até essa automação existir, a revisão M/L valida
estas regras manualmente.
