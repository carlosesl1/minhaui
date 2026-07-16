# Consolidação do baseline — 2026-07-15

## Resultado

O checkout do Windows Native Dock foi convertido de um snapshot local amplo em
um baseline Git reproduzível na branch `feature/windows-native-dock-v1`.

- `a8cef44` — remove metadados e evidências `.omo/**` somente do índice Git e
  adiciona a regra de ignore; os arquivos permanecem disponíveis no disco local.
- `1c12487` — consolida fontes, testes, documentação, ADRs, governança, CI,
  scripts e packaging do produto.

## Gates do commit candidato

| Gate | Resultado |
| --- | --- |
| `cargo fmt --all --check` | Verde |
| Testes da política PowerShell | Verde |
| `scripts/Check-Architecture.ps1` | Verde: 7 crates e 7 exceções registradas |
| `git diff --cached --check` | Verde |
| Clippy estrito do workspace, todos os targets e features | Verde |
| `cargo test --workspace` | Verde |
| Build release do workspace em target isolado | Verde |
| `cargo deny check advisories bans licenses sources` | Verde, com allowances de licença não encontradas como avisos |

`cargo-audit` não está instalado no ambiente local. O workflow instala e executa
a ferramenta no job obrigatório do GitHub Actions. A validação nativa Windows
permanece separada e opt-in, conforme ADR-0003; ela não foi reexecutada neste
checkpoint determinístico.

## Estado de higiene

- fontes duráveis estão versionadas;
- `.omo/**`, `target/**`, worktrees e metadados locais permanecem fora do Git;
- nenhum log, screenshot, binário ou segredo foi incluído no baseline;
- quatro scripts históricos de QA continuam preservados sob `.omo/**` e somente
  deverão migrar para `scripts/qa/**` após validação própria.

## Regra para continuidade

Novas mudanças partem deste baseline em branches curtas, com um objetivo por
incremento, classificação S/M/L e gates proporcionais. O snapshot amplo não deve
ser repetido: decisões estruturais usam ADR e cada incremento deve permanecer
revisável e reversível.
