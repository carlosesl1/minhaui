# Governança de entrega

## Modelo de branch

- `main` permanece verde e potencialmente publicável.
- Cada branch possui um único objetivo e vida curta.
- Uma branch que ultrapasse aproximadamente três dias ou misture objetivos deve
  ser dividida em incrementos integráveis.
- Refatorações não relacionadas não acompanham uma funcionalidade.
- Integração só ocorre a partir de um commit limpo verificado.

## Classificação de risco

### S — local

Um Module, sem mudança estrutural de Interface ou persistência. Exige teste
focado e gate rápido.

### M — integrada

Vários Modules, persistência ou comportamento integrado dentro da direção atual.
Exige plano curto, seção de Impacto Arquitetural e testes de integração.

### L — estrutural

Várias crates, nova Interface pública relevante, FFI, concorrência, processo,
recuperação ou mudança difícil de reverter. Exige ADR, plano por etapas, Impacto
Arquitetural e revisão arquitetural final separada.

## Impacto Arquitetural para M/L

O plano registra:

- Modules e crates afetados;
- Interfaces alteradas;
- proprietário de novo estado;
- dependências adicionadas;
- impacto na thread da UI;
- diagnóstico necessário;
- estratégia de testes;
- migração e rollback;
- ADR relacionado, quando aplicável.

## Definition of Done

Uma mudança está concluída somente quando:

- objetivo e critérios de aceitação foram atendidos;
- o diff está restrito ao objetivo da branch;
- testes são proporcionais ao risco S/M/L;
- caminhos de erro e rollback foram considerados;
- observabilidade foi adicionada apenas quando necessária e não bloqueia a UI;
- documentação e ADR foram atualizados quando aplicável;
- o commit não contém artefatos gerados acidentais;
- todos os gates obrigatórios estão verdes;
- o build foi produzido a partir do commit candidato à integração.

## Gates

### Gate rápido

- `cargo fmt --all --check`;
- Clippy e testes focados no escopo alterado.

### Gate determinístico de integração

- `powershell -NoProfile -File scripts/Check-Architecture.ps1`;
- Clippy estrito do workspace;
- `cargo test --workspace` para os testes herméticos;
- build release;
- validação da direção arquitetural;
- auditoria de vulnerabilidades, licenças e fontes.

### Gate nativo Windows

Valida AppBar, DWM, pacotes instalados, ícones e comportamento visual em ambiente
controlado com pré-requisitos explícitos. Ele não é misturado à suíte hermética
e bloqueia releases afetadas por integrações nativas.

Comandos atuais:

```powershell
cargo test -p shell-platform-windows --features native-validation --lib
cargo test -p shell-app --features native-validation --test showcase_startup
```

O runner controlado usa os labels `self-hosted`, `windows`, `x64` e
`obsidian-native-validation`. Ele deve possuir Calculator e Windows Terminal,
DWM ativo, desktop interativo e nenhuma instância do produto ou AppBar
conflitante. O job é solicitado explicitamente por `workflow_dispatch` e não
deve ser substituído por skips condicionais dentro dos testes.

## Exceções

Nenhum gate é desativado silenciosamente. Uma exceção declara:

- regra e escopo exatos;
- motivo e risco aceito;
- responsável;
- compensação temporária;
- data de expiração, limitada por padrão a 14 dias;
- plano de remoção.

Ao expirar, o gate volta a bloquear. Testes ignorados seguem a mesma política.

## Releases, migrações e rollback

- O artefato indica versão e commit de origem.
- Releases são produzidos apenas de commits limpos e reproduzíveis.
- Configurações persistidas permanecem versionadas.
- Migrações preservam backup do último estado válido.
- Mudança incompatível de schema exige ADR e teste de migração.
- Cada release possui instrução objetiva de rollback.

## Higiene

- Não versionar logs, binários, dumps, caches ou capturas temporárias de QA.
- Fixtures pequenas e estáveis podem ser versionadas quando participam de teste.
- Evidência permanente deve explicar finalidade e reprodução.
- Nenhuma limpeza, movimentação, stage ou commit de arquivos preexistentes ocorre
  sem revisão explícita do inventário.
