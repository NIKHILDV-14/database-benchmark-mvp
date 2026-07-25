# Database Benchmarking and Recommendation Tool MVP

A fair and reproducible benchmarking system for comparing PostgreSQL, MongoDB, and Qdrant on real text workloads.

---

## Setup

1. Copy the environment variables template:
   ```bash
   cp .env.example .env
   ```

2. Start the database infrastructure:
   ```bash
   docker compose up -d
   ```

3. Wait for all containers to report as healthy:
   ```bash
   docker compose ps
   ```

4. Build the project:
   ```bash
   cargo build
   ```

---

## Dataset Pipeline (Phase 2)

### Supported Formats

| Format | Extension | Notes |
|--------|-----------|-------|
| JSON Lines | `.jsonl` | One JSON object per line |
| CSV | `.csv` | Requires `id` and `text` columns |

### Canonical Document Schema

All input datasets are normalised into a single internal representation before loading into any database:

```json
{
  "id":       "unique-document-id",
  "title":    "Optional title string",
  "text":     "The main text content",
  "metadata": { "any": "key-value pairs" }
}
```

- `id` — **required**, non-empty string, must be unique within the dataset
- `title` — optional string
- `text` — **required** string (empty-text behaviour is configurable)
- `metadata` — arbitrary JSON object; preserved verbatim and serialised with sorted keys for deterministic hashing

### Sample JSONL Format

```jsonl
{"id": "doc_1", "title": "Introduction to Databases", "text": "A database is an organised collection of data.", "metadata": {"category": "databases"}}
{"id": "doc_2", "title": "Machine Learning Basics", "text": "Machine learning uses data and algorithms to imitate how humans learn.", "metadata": {"category": "ml"}}
```

### Sample CSV Format

```csv
id,title,text,category
doc_1,Introduction to Databases,A database is an organised collection of data.,databases
doc_2,Machine Learning Basics,Machine learning uses algorithms to imitate learning.,ml
```

Any column that is not `id`, `title`, or `text` is stored as a string value in the document's `metadata` map.

---

## Validation Rules

The pipeline enforces the following checks before accepting a dataset:

1. **File existence** — the path must resolve to an existing file
2. **Format** — extension must be `.jsonl` or `.csv`
3. **Size limit** — file must not exceed `max_size_gb` (default 2 GB) in the config
4. **Readable input** — any I/O error during streaming produces a structured error
5. **Required `id`** — each record must have a non-empty `id` field after whitespace trimming
6. **Unique IDs** — duplicate IDs produce an error identifying the offending ID
7. **Required `text`** — the `text` field must be present and parse correctly
8. **Empty-text policy** — controlled by `empty_text_policy`:
   - `"reject"` (default) — empty text after preprocessing causes an error
   - `"allow"` — empty text is accepted (useful for metadata-only workloads)
9. **Line context** — errors include line or record numbers where practical

Malformed records are **never silently skipped**.

---

## Preprocessing

Preprocessing is deterministic and minimal. The same rules are applied:
- Before dataset validation and hashing
- Before loading into any database (Phase 3+)
- Before embedding generation (Phase 3+)

| Setting | Config key | Default | Effect |
|---------|-----------|---------|--------|
| Normalise line endings | built-in | always | `\r\n` → `\n` |
| Trim surrounding whitespace | `trim_whitespace` | `true` | Strips leading/trailing whitespace from `text` |

Aggressive transformations (lower-casing, stemming, punctuation removal) are intentionally excluded to preserve semantic content and avoid biasing database-specific retrieval.

---

## Deterministic Dataset Hashing

Each processed dataset is assigned a SHA-256 fingerprint that identifies the **logical canonical content**, not filesystem properties such as path or modification time.

**What is hashed (in order):**
1. Preprocessing configuration (`empty_text_policy`, `trim_whitespace`)
2. For each document (insertion order): `id`, `title`, `text`, `metadata`

**Metadata key ordering:** metadata keys are sorted lexicographically before hashing so that `{"a":1,"b":2}` and `{"b":2,"a":1}` produce the same hash.

**Document ordering:** hash is **order-sensitive** — two datasets with the same documents but different file ordering produce different hashes. This is intentional and reflects that document order may matter for retrieval evaluation.

---

## Dataset Metadata

`DatasetMetadata` is returned after successful validation and contains:

| Field | Type | Description |
|-------|------|-------------|
| `path` | String | Dataset file path |
| `format` | String | `"jsonl"` or `"csv"` |
| `document_count` | usize | Number of accepted documents |
| `original_size_bytes` | u64 | Raw file size in bytes |
| `dataset_hash` | String | Hex SHA-256 fingerprint |
| `empty_text_policy` | String | Policy used during validation |
| `trim_whitespace` | bool | Trim setting used during validation |

---

## Configuration

Add dataset settings to your experiment TOML (e.g. `configs/sample.toml`):

```toml
[dataset]
path             = "./datasets/sample.jsonl"
max_size_gb      = 2.0
empty_text_policy = "reject"   # "reject" | "allow"
trim_whitespace  = true
```

The fields `empty_text_policy` and `trim_whitespace` have serde defaults (`"reject"` and `true` respectively), so existing configs without these keys remain valid.

---

## Running Tests

```bash
# All tests (unit + doc-tests)
cargo test --workspace

# Dataset crate only
cargo test -p dataset

# With output visible
cargo test -p dataset -- --nocapture
```

Tests do **not** require any running database. All test files are written to the OS temporary directory and cleaned up after each test.

---

## Sample Datasets

| File | Format | Documents | Purpose |
|------|--------|-----------|---------|
| `datasets/sample.jsonl` | JSONL | 12 | Development testing (databases, ML, finance, networking, OS) |
| `datasets/sample.csv` | CSV | 5 | CSV loader testing |

> **⚠ Not for benchmarking:** These samples are too small for meaningful performance conclusions.
> Use a realistic dataset (≥ tens of thousands of documents) for actual benchmarking.

---

## Phase 2 Limitations

- **In-memory duplicate detection:** The HashSet used for ID deduplication is held in RAM. For extremely large datasets (hundreds of millions of documents), a disk-backed approach would be needed. This is documented and acceptable for the MVP.
- **CSV metadata type preservation:** CSV fields not mapped to `id`/`title`/`text` are stored as JSON strings — numeric and boolean types are not inferred.
- **Document ordering is file ordering:** The dataset hash commits to the order documents appear in the file. Clients must ensure stable ordering when reproducibility across file re-generations is required.
