# Fuzz

Alvos de [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) para os parsers de entrada
não confiável. Exige Rust **nightly** (em Linux/macOS).

| Alvo | O que exercita |
| --- | --- |
| `snapshot_load` | `FileIndex::load_snapshot` + leitura de todas as entradas |
| `usn_buffer` / `mft_record` / `mft_layout` | parsers NTFS puros (`ff_index::ntfs`): buffers de USN e registros da MFT |
| `docx_xml` / `docx_zip` | extração de texto de DOCX (XML e pacote zip) |
| `pdf` | extração de texto de PDF (`pdf-extract`) |
| `query_parse` | `Pattern::parse_with_options` (regex, curingas, fuzzy) |
| `ipc_request` | desserialização + execução de `IpcRequest` no `RequestHandler` |

```sh
cargo install cargo-fuzz --locked
cargo +nightly fuzz run snapshot_load -- -max_total_time=60
```

Um crash gera um arquivo em `fuzz/artifacts/<alvo>/`. Transforme-o em teste de regressão em
`crates/*/tests/*_robustness.rs` antes de corrigir. O job semanal está em `.github/workflows/fuzz.yml`.

Os testes determinísticos (`*_robustness.rs`) rodam em todo `cargo test --workspace`; o fuzz é a
exploração longa que eles não substituem.
