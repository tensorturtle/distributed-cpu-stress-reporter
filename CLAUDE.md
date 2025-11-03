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

### Scheduler Catch-Up Bias

The application supports two execution modes to handle Linux scheduler behavior:

1. **Default (Threaded) Mode**: Spawns worker threads that run continuously in a single process
   - Maximum performance
   - Subject to scheduler catch-up bias when multiple instances compete for CPU
   - Newer instances may get more CPU allocation due to lower accumulated virtual runtime

2. **Fresh-Process Mode** (`--fresh-process-mode`): Spawns fresh child processes for each calculation cycle
   - Avoids catch-up bias by preventing virtual runtime accumulation
   - Equal scheduler treatment across all instances regardless of start time
   - Slightly lower absolute performance due to process creation overhead
   - Better for fair comparison when testing multiple instances

**Catch-up bias**: Linux CFS scheduler prioritizes processes with lower accumulated CPU time (virtual runtime), causing newly launched processes to receive more CPU allocation than older processes. This can skew performance measurements in multi-instance CPU contention tests.
