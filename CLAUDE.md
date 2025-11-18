# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**distributed-cpu-stress-reporter** is a Rust project for distributed CPU stress testing and reporting across multiple nodes.

## Build and Development Commands

```bash
# Build the project
cargo build

# Build with optimizations
cargo build --release

# Run the application
cargo run

# Run with release optimizations
cargo run --release

# Check code without building
cargo check

# Run tests
cargo test

# Run a specific test
cargo test <test_name>

# Format code
cargo fmt

# Lint with Clippy
cargo clippy

# Clean build artifacts
cargo clean
```

## Project Structure

This is an early-stage project with a minimal structure:
- `src/main.rs` - Entry point of the application
- `Cargo.toml` - Project configuration and dependencies

## Architecture Notes

This application is designed for testing CPU performance in overprovisioned VM environments:

- **CPU Stress Engine**: Multi-threaded prime number calculation running on all available cores
- **Performance Sampling**: Tracks operations per second using 1-second sampling intervals
- **HTTP Reporter**: Axum-based server on port 8080 serving metrics at `/cpu-perf` endpoint
- **Use Case**: Runs inside VMs to report actual CPU performance to external monitoring systems, helping measure the impact of CPU overprovisioning in Proxmox hosts

The application combines continuous CPU-bound workload with an HTTP server to enable remote performance queries.

### Execution Modes

The application supports three execution modes controlled via HTTP API:

1. **Threaded Mode**: Spawns worker threads that run continuously in a single process
   - Maximum performance
   - Subject to scheduler catch-up bias when multiple instances compete for CPU
   - Newer instances may get more CPU allocation due to lower accumulated virtual runtime

2. **Fresh-Process Mode**: Spawns fresh child processes for each calculation cycle
   - Avoids catch-up bias by preventing virtual runtime accumulation
   - Equal scheduler treatment across all instances regardless of start time
   - Slightly lower absolute performance due to process creation overhead
   - Better for fair comparison when testing multiple instances

3. **Bursty Mode**: Simulates consumer desktop CPU usage patterns
   - Uses fresh processes during burst periods (avoids scheduler bias)
   - Alternates between high CPU load (bursts) and idle periods
   - Burst/idle timing follows exponential distribution for realistic bursty behavior
   - Configurable utilization percentage (e.g., 50% = half burst, half idle)
   - Time-aware metrics track performance only during burst periods
   - Reports "how much CPU do we get when we need it?"
   - Each VM instance uses independent random timing (desynchronized bursts across hosts)
   - Useful for testing CPU contention with realistic workload patterns

**Catch-up bias**: Linux CFS scheduler prioritizes processes with lower accumulated CPU time (virtual runtime), causing newly launched processes to receive more CPU allocation than older processes. This can skew performance measurements in multi-instance CPU contention tests.

### HTTP Endpoints

- `POST /start-cpu` - Start CPU stress test with mode specification
  - `{"mode":"threaded"}` - Maximum CPU stress
  - `{"mode":"fresh-process"}` - Avoid scheduler bias
  - `{"mode":"bursty","utilization":60}` - Simulate bursty workload at 60% utilization (default: 50%)
- `POST /end-cpu` - Stop CPU stress test
- `GET /cpu-perf` - Get operations per second (threaded/fresh-process modes)
- `GET /burst-perf` - Get burst-only operations per second (bursty mode)
