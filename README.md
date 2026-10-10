# Database Benchmarking and Recommendation Tool MVP

A fair and reproducible benchmarking and recommendation system for comparing **PostgreSQL**, **MongoDB**, **Qdrant**, and **LanceDB** on text and vector workloads.

---

## Quickstart

### 1. Zero-Dependency Mode (LanceDB Embedded)

To run the full end-to-end pipeline immediately without installing Docker or external database servers:

```bash
# Run the pipeline with the embedded LanceDB configuration
cargo run -- configs/lancedb_only.toml

# Or output the full results as structured JSON
cargo run -- configs/lancedb_only.toml --json
```

### 2. Multi-Database Mode (PostgreSQL, MongoDB, Qdrant, LanceDB)

To benchmark external databases alongside LanceDB:

1. Copy the environment variables template:
   ```bash
   cp .env.example .env
   ```

2. Start the database infrastructure:
   ```bash
   docker compose up -d
   ```

3. Verify all containers are healthy:
   ```bash
   docker compose ps
   ```

4. Run the benchmark across all enabled databases:
   ```bash
   cargo run -- configs/sample.toml
   ```

> **Note on Service Availability:** If an external database container is not running, the tool will gracefully log the connection failure, mark that evaluation as invalid, and exclude it from the final recommendation with an explanatory diagnostic message, allowing healthy databases to still be benchmarked and ranked.

---

## End-to-End Pipeline (Phase 8 Architecture)

The system connects all components in a unified, automated lifecycle:

```text
┌─────────────────┐
│ Dataset Loading │  (JSONL / CSV loading, schema validation, size limits, duplicate check)
└────────┬────────┘
         │
┌────────▼────────┐
│ Preprocessing   │  (Line-ending normalization, whitespace trimming, deterministic SHA-256 hash)
└────────┬────────┘
         │
┌────────▼────────┐
│ Workload Setup  │  (Canonical documents, query specs, environment hardware recording)
└────────┬────────┘
         │
┌────────▼────────┐
│ Embedding       │  (Deterministic chunking, embedding orchestrator, vector record creation)
└────────┬────────┘
         │
┌────────▼────────┐
│ Benchmark Exec  │  (Adapters: connect → initialize → load/insert → execute queries → cleanup)
└────────┬────────┘
         │
┌────────▼────────┐
│ Evaluation      │  (Latency, throughput, success rate, validation rules, cross-db stats)
└────────┬────────┘
         │
┌────────▼────────┐
│ Recommendation  │  (Min-max normalization, configurable multi-criteria weights, rank & winner)
└─────────────────┘
```

---

## Workspace Crates

The project is structured as a modular Rust workspace:

| Crate | Purpose |
|-------|---------|
| `common` | Core types (`DatabaseType`, `VectorRecord`, `Capabilities`), shared errors, and utilities. |
| `config` | Experiment, dataset, database, and embedding configuration models and validation. |
| `dataset` | Streaming dataset loader (JSONL, CSV), schema validation, preprocessing, and SHA-256 hashing. |
| `embedding` | Vector embedding pipeline: chunking, cache management, retry policy, and provider abstraction. |
| `adapters` | Database adapters implementing `DatabaseAdapter` and `VectorAdapter` (PostgreSQL, MongoDB, Qdrant, LanceDB). |
| `environment` | Hardware and OS environment recorder (CPU cores, memory, OS version, experiment UUID). |
| `benchmark` | Workload definitions, adapter lifecycle execution runner, and raw metric collection. |
| `evaluation` | Metric calculation, benchmark validation, cross-database aggregation, and evaluation reports. |
| `recommendation` | Multi-criteria scoring, normalization, configurable weights, and database recommendation ranking. |
| `runner` | End-to-end integration pipeline orchestrator (`BenchmarkPipeline`) and CLI executable (`runner`). |

---

## Execution & CLI Usage

The `runner` crate serves as the primary practical entry point:

```bash
# Default run (uses configs/sample.toml)
cargo run

# Custom configuration file
cargo run -- path/to/config.toml

# Machine-readable JSON output
cargo run -- path/to/config.toml --json

# Help menu
cargo run -- --help
```

### Programmatic API

The pipeline can also be called directly from Rust code:

```rust
use runner::{BenchmarkPipeline, PipelineOptions};
use benchmark_config::BenchmarkConfig;

// Run from a file
let result = BenchmarkPipeline::run_from_file("configs/lancedb_only.toml")?;

// Or run with custom evaluation and recommendation weights
let options = PipelineOptions::default();
let result = BenchmarkPipeline::run_with_options(&config, &options)?;

println!("Recommended database: {:?}", result.best_database());
println!("{}", result.format_summary());
```

---

## Configuration Reference

Example experiment configuration (`configs/sample.toml`):

```toml
[experiment]
name = "text-mvp-test"
seed = 42

[dataset]
path              = "./datasets/sample.jsonl"
max_size_gb       = 2.0
empty_text_policy = "reject"   # "reject" | "allow"
trim_whitespace   = true

[databases.postgres]
enabled  = true
host     = "localhost"
port     = 5432
username = "postgres"
password = "postgres"
database = "benchmark_db"

[databases.mongodb]
enabled  = true
host     = "localhost"
port     = 27017
username = "mongo"
password = "mongo"
database = "benchmark_db"

[databases.qdrant]
enabled  = true
host     = "localhost"
port     = 6334
database = "benchmark_vectors"

[databases.lancedb]
enabled  = true
host     = "/tmp/lancedb_benchmark"
database = "benchmark_vectors"

[embedding]
provider      = "dummy"        # "dummy" provides deterministic local vectors
batch_size    = 32
chunk_size    = 500
chunk_overlap = 50
cache_enabled = false
dimension     = 128
```

---

## Recommendation Engine (Phase 7)

The recommendation engine scores and ranks eligible database candidates based on multi-criteria utility:

- **Metrics Evaluated**:
  - `latency`: insert and query latencies (lower is better: normalized as $(v_{\max} - v) / (v_{\max} - v_{\min})$).
  - `throughput`: operations per second (higher is better: normalized as $(v - v_{\min}) / (v_{\max} - v_{\min})$).
  - `accuracy`: precision, recall, and query F1 when ground truth is supplied (higher is better).
  - `resource_usage`: memory and CPU usage when recorded (lower is better).
- **Missing Metrics**: Handled dynamically without inventing arbitrary values; scores renormalize over observed dimensions.
- **Exclusion**: Failed or invalid evaluations (e.g. database connection refused) are excluded with explicit diagnostic explanations.
- **Tie-Breaking**: Ties in composite score are broken deterministically using alphabetical ordering of database identifiers.

---

## Running Tests

All 185 workspace tests run in-memory and do **not** require external Docker containers:

```bash
# Run all workspace tests (unit + integration + doc-tests)
cargo test --workspace

# Run integration tests only
cargo test -p runner --test integration_test

# Run recommendation tests only
cargo test -p recommendation

# Run evaluation tests only
cargo test -p evaluation
```

---

## Current Limitations

1. **Local LanceDB Backend**: The default LanceDB adapter runs in an in-memory vector mode. Enabling native LanceDB disk storage requires the Protocol Buffer compiler (`protoc`) installed on the host system.
2. **Synthetic Embedding Provider**: The default embedding provider is `dummy`, which deterministically hashes text into dense float vectors for offline reproducible benchmarking. Production models (e.g. ONNX BGE/E5 or remote APIs) require external provider configuration.
3. **External Database Dependencies**: Benchmarking PostgreSQL, MongoDB, or Qdrant requires their Docker containers (`docker compose up -d`) to be running. If not running, they will be reported as connection failures.
4. **Single-Node Benchmarking**: The benchmarking harness currently runs on a single node; distributed cluster testing and network jitter simulation are out of scope for the MVP.
5. **In-Memory ID Deduplication**: Dataset validation stores document IDs in an in-memory HashSet; datasets with hundreds of millions of documents require disk-backed deduplication.
