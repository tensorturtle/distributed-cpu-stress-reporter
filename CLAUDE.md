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
