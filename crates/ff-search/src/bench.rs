//! Benchmark reproduzível de latência de busca (Checkpoint B, §2.6).

use std::time::Duration;

use ff_index::FileIndex;

use crate::engine::SearchEngine;
use crate::query::Query;

/// Estatísticas de latência de uma bateria de consultas.
#[derive(Debug, Clone)]
pub struct BenchmarkStats {
    pub pattern: String,
    pub iterations: usize,
    pub total_scanned: usize,
    pub matches_count: usize,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub min: Duration,
    pub max: Duration,
}

/// Executa múltiplas iterações de uma consulta e calcula os percentis p50, p95 e p99.
#[must_use]
pub fn benchmark_search(
    index: &FileIndex,
    pattern: &str,
    is_regex: bool,
    iterations: usize,
) -> BenchmarkStats {
    let query = Query::parse(pattern, is_regex).expect("padrão de benchmark válido");
    let iters = iterations.max(1);
    let mut latencies = Vec::with_capacity(iters);
    let mut matches_count = 0;

    // Warm-up
    let _ = SearchEngine::search(index, &query);

    for _ in 0..iters {
        let result = SearchEngine::search(index, &query);
        matches_count = result.matches.len();
        latencies.push(result.elapsed);
    }

    latencies.sort();

    let p50_idx = (iters as f64 * 0.50).floor() as usize;
    let p95_idx = (iters as f64 * 0.95).floor().min((iters - 1) as f64) as usize;
    let p99_idx = (iters as f64 * 0.99).floor().min((iters - 1) as f64) as usize;

    BenchmarkStats {
        pattern: pattern.to_string(),
        iterations: iters,
        total_scanned: index.len(),
        matches_count,
        p50: latencies[p50_idx],
        p95: latencies[p95_idx],
        p99: latencies[p99_idx],
        min: latencies[0],
        max: latencies[iters - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff_index::{IndexSource, SyntheticSource};

    #[test]
    fn test_checkpoint_b_2m_latency_p95() {
        let mut index = FileIndex::new();
        let total = 2_000_000;
        let mut source = SyntheticSource::new(total);
        source.populate(&mut index).unwrap();

        // Testa substring sobre os 2M de itens com 10 iterações
        let stats = benchmark_search(&index, "file_123", false, 10);

        eprintln!(
            "Checkpoint B Benchmark (2M entradas): pattern='{}' | matches={} | p50={:.2?} | p95={:.2?} | min={:.2?} | max={:.2?}",
            stats.pattern, stats.matches_count, stats.p50, stats.p95, stats.min, stats.max
        );

        // Critério de aceite Checkpoint B: p95 < 50 ms em release (§2.6).
        // Em modo debug (sem otimizações do compilador) em runners de CI com 2 vCPUs compartilhadas,
        // permite margem para ausência de inlining e bounds-checking ativo.
        let limit = if cfg!(debug_assertions) {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(50)
        };
        assert!(
            stats.p95 < limit,
            "Latência p95 ({:?}) excedeu a meta de {:?} do Checkpoint B",
            stats.p95,
            limit
        );
    }
}
