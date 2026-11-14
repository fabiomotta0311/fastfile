//! Fonte sintética para geração de índices volumosos em testes e benchmarks em qualquer SO.

use crate::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
use crate::index::FileIndex;
use crate::source::IndexSource;

/// Gerador pseudo-aleatório leve (xorshift64) para velocidade máxima sem dependências externas.
struct FastRng {
    state: u64,
}

impl FastRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0xdead_beef_cafe_babe
            } else {
                seed
            },
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    #[inline]
    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        let span = (max - min) as u64;
        min + (self.next_u64() % span) as usize
    }
}

/// Fonte sintética de dados que gera uma árvore de pastas e arquivos para benchmark e testes.
pub struct SyntheticSource {
    total_entries: usize,
    seed: u64,
}

impl SyntheticSource {
    /// Cria uma nova fonte sintética para gerar `total_entries`.
    #[must_use]
    pub fn new(total_entries: usize) -> Self {
        Self {
            total_entries,
            seed: 42,
        }
    }

    /// Define a semente do gerador determinístico.
    #[must_use]
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
}

const EXTENSIONS: &[&str] = &[
    "rs", "toml", "json", "dll", "exe", "txt", "md", "log", "png", "jpg", "zip", "tar",
];

const DIR_PREFIXES: &[&str] = &[
    "src",
    "target",
    "debug",
    "release",
    "node_modules",
    "vendor",
    "lib",
    "bin",
    "assets",
    "docs",
];

impl IndexSource for SyntheticSource {
    type Error = std::convert::Infallible;

    fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error> {
        if self.total_entries == 0 {
            return Ok(());
        }

        // Pré-aloca estimando ~16 bytes de nome médio
        if index.is_empty() {
            *index = FileIndex::with_capacity(self.total_entries, self.total_entries * 16);
        }

        let mut rng = FastRng::new(self.seed);

        // Raiz "C:"
        let root_id = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 1_700_000_000)
            .expect("adicionar raiz sintética");

        let mut dir_ids = Vec::with_capacity(10_000);
        dir_ids.push(root_id);

        let mut name_buf = String::with_capacity(64);
        let mut created = 1;

        while created < self.total_entries {
            let parent_idx = rng.next_range(0, dir_ids.len());
            let parent_id = dir_ids[parent_idx];
            let frn = (created + 1) as u64;

            // ~10% de diretórios, 90% de arquivos
            let is_dir =
                dir_ids.len() < (self.total_entries / 10) && rng.next_u64().is_multiple_of(10);

            name_buf.clear();
            if is_dir {
                let prefix = DIR_PREFIXES[rng.next_range(0, DIR_PREFIXES.len())];
                use std::fmt::Write;
                let _ = write!(name_buf, "{prefix}_{created}");

                let dir_id = index
                    .add_entry(frn, parent_id, &name_buf, FLAG_DIRECTORY, 0, 1_700_000_000)
                    .expect("adicionar pasta sintética");

                dir_ids.push(dir_id);
            } else {
                let ext = EXTENSIONS[rng.next_range(0, EXTENSIONS.len())];
                use std::fmt::Write;
                let _ = write!(name_buf, "file_{created}.{ext}");

                let size = rng.next_u64() % (10 * 1024 * 1024);
                let mtime = 1_700_000_000 + (rng.next_u64() % 1_000_000) as i64;

                index
                    .add_entry(frn, parent_id, &name_buf, FLAG_FILE, size, mtime)
                    .expect("adicionar arquivo sintético");
            }

            created += 1;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_source_small() {
        let mut index = FileIndex::new();
        let mut source = SyntheticSource::new(100);
        source.populate(&mut index).unwrap();

        assert_eq!(index.len(), 100);
        // O primeiro é a raiz
        assert_eq!(index.name(0).unwrap(), "C:");

        // Caminho de uma folha
        let path = index.build_path(99).unwrap();
        assert!(path.starts_with("C:\\"));
    }

    #[test]
    fn test_synthetic_source_2m_benchmark() {
        let mut index = FileIndex::new();
        let start = std::time::Instant::now();
        let count = 2_000_000;
        let mut source = SyntheticSource::new(count);
        source.populate(&mut index).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(index.len(), count);
        let mem = index.memory_usage();
        let mem_mb = mem as f64 / (1024.0 * 1024.0);
        let bytes_per_entry = mem as f64 / count as f64;

        eprintln!(
            "Sintético: 2M entradas geradas em {:.2?} | Memória: {:.2} MB ({:.1} bytes/arquivo)",
            elapsed, mem_mb, bytes_per_entry
        );

        // Critério de aceite: memória < 300 MB para 2M entradas (§2.6)
        assert!(
            mem_mb < 300.0,
            "Uso de memória ({mem_mb:.2} MB) excedeu o limite de 300 MB"
        );
    }
}
