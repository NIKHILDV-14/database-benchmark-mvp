# Database Benchmarking and Recommendation Tool MVP

A fair and reproducible benchmarking system for comparing PostgreSQL, MongoDB, and Qdrant.

## Setup

1. Copy the environment variables template:
   ```bash
   cp .env.example .env
   ```

2. Start the database infrastructure:
   ```bash
   docker compose up -d
   ```

3. Wait for the containers to report as healthy:
   ```bash
   docker compose ps
   ```

4. Build the project:
   ```bash
   cargo build
   ```
