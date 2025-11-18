# Distributed CPU Stress Reporter

Lightweight HTTP server that stress-tests CPU cores and reports performance metrics. Built for measuring CPU performance in virtualized environments.

## Quick Start

```bash
git clone https://github.com/tensorturtle/distributed-cpu-stress-reporter.git
cd distributed-cpu-stress-reporter
cargo build --release
./target/release/distributed-cpu-stress-reporter
```

Start CPU stress test and query performance:
```bash
# Start the CPU stress test (requires mode specification)
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"fresh-process"}'

# Query performance
curl http://localhost:8080/cpu-perf
# Returns: 254060 (operations/second)

# Stop the CPU stress test
curl -X POST http://localhost:8080/end-cpu
```

## Use Case

Test CPU overprovisioning in VMs. Run this in multiple VMs on the same hypervisor to see how CPU contention affects actual performance.

**Example:** Proxmox host with 8 cores running 4 VMs with 4 vCPUs each (2x overprovisioned):

```bash
# Start CPU stress test on all VMs
for vm in vm1 vm2 vm3 vm4; do
  curl -X POST http://$vm:8080/start-cpu \
    -H 'Content-Type: application/json' \
    -d '{"mode":"fresh-process"}'
done

# Query each VM
curl http://vm1:8080/cpu-perf  # 240000 ops/sec
curl http://vm2:8080/cpu-perf  # 238000 ops/sec
curl http://vm3:8080/cpu-perf  # 120000 ops/sec  ← throttled!
curl http://vm4:8080/cpu-perf  # 241000 ops/sec
```

**Deploy to multiple VMs:**
```bash
# On each VM, run:
curl -L https://files.tensorturtle.com/yundera-cpu-stress/cpu-stress-linux-amd64 -o cpu-stress && chmod +x cpu-stress && ./cpu-stress
```

**Monitor multiple VMs:**
```bash
# Start CPU stress on all VMs
for vm in 192.168.1.{101..104}; do
  curl -s -X POST http://$vm:8080/start-cpu \
    -H 'Content-Type: application/json' \
    -d '{"mode":"fresh-process"}'
done

# Monitor performance
while true; do
  for vm in 192.168.1.{101..104}; do
    echo "$vm: $(curl -s http://$vm:8080/cpu-perf) ops/sec"
  done
  sleep 2
done
```

## How It Works

- Spawns threaded workers, fresh-process spawners, and burst coordinator (one per CPU core)
- CPU stress test starts in STOPPED state (use `/start-cpu` to begin)
- Mode selection determines which worker type is active:
  - **Threaded mode**: Long-running threads continuously calculate primes (max performance)
  - **Fresh-process mode**: Spawns short-lived child processes for each calculation cycle (avoids scheduler bias)
  - **Bursty mode**: Spawns processes during bursts with exponential distribution timing (realistic workload patterns)
- Atomic counters track operations per second with time-aware metrics for bursty mode
- HTTP server (Axum) provides control and query endpoints:
  - POST `/start-cpu` - Start CPU stress test (requires JSON body with mode and optional utilization)
  - POST `/end-cpu` - Stop CPU stress test
  - GET `/cpu-perf` - Get current operations per second (threaded/fresh-process modes)
  - GET `/burst-perf` - Get burst-only operations per second (bursty mode)

**Why prime numbers?** Pure CPU computation with no I/O - perfect for measuring CPU performance.

## Scheduler Catch-Up Bias

When running multiple instances of this program competing for limited CPU cores, you may observe that **newly launched instances receive more CPU allocation** than older running instances. This is a Linux scheduler behavior called "catch-up bias."

**What happens:**
- The Linux CFS (Completely Fair Scheduler) tracks CPU time consumed by each process (virtual runtime)
- Older processes have accumulated more virtual runtime
- Newly launched processes start with low virtual runtime
- The scheduler prioritizes processes that are "behind" to achieve fairness
- Result: New instances temporarily get more CPU to "catch up"

**Why this matters for testing:**
When measuring CPU overprovisioning effects, catch-up bias can skew results. If you launch instances at different times, newer instances will appear to perform better, making it difficult to measure true steady-state CPU contention.

**Solutions:**

1. **Launch all instances simultaneously** - Start all test instances at the same time to ensure fair comparison
2. **Wait for equilibrium** - Let instances run for several minutes until scheduler balancing stabilizes
3. **Use fresh-process mode** (recommended, see below)

### Execution Modes

The application supports three execution modes, controlled via the HTTP API:

#### Fresh Process Mode (Default & Recommended)

```bash
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"fresh-process"}'
```

In this mode:
- Main HTTP server runs continuously
- Each CPU calculation runs in a fresh child process that exits after completing work
- No long-running processes accumulate virtual runtime
- All instances get equal scheduler treatment regardless of start time

**When to use:**
- Testing multiple instances launched at different times
- Measuring steady-state CPU contention without scheduler bias (recommended)
- Comparing performance across instances that need equal scheduler treatment

#### Threaded Mode

```bash
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"threaded"}'
```

In this mode:
- Long-running worker threads continuously calculate primes
- Maximum CPU stress and absolute performance
- Subject to scheduler catch-up bias when multiple instances compete

**When to use:**
- Maximum CPU stress and performance
- Single instance testing
- All instances launched simultaneously

#### Bursty Mode

```bash
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"bursty","utilization":60}'
```

In this mode:
- Simulates consumer desktop CPU usage patterns with realistic bursty behavior
- Alternates between CPU bursts and idle periods using exponential distribution
- Burst durations: 500ms-5s (exponentially distributed, mean ~1.5s)
- Configurable utilization percentage (0-100, default 50)
- Uses fresh processes during bursts (avoids scheduler bias)
- Time-aware metrics track performance only during burst periods
- Independent random timing per VM instance (desynchronized across hosts)

**Query burst performance:**
```bash
curl http://localhost:8080/burst-perf
# Returns: 233672 (ops/sec during bursts only)
```

**When to use:**
- Testing CPU contention with realistic workload patterns
- Simulating consumer desktop or mixed workload scenarios
- Measuring "how much CPU do we get when we need it?"
- Testing multiple VMs with desynchronized load patterns

**Example: Different utilization levels**
```bash
# Light bursty load (25% utilization)
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"bursty","utilization":25}'

# Heavy bursty load (75% utilization)
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"bursty","utilization":75}'
```

#### Switching Modes

You can switch modes at any time via the API. If the CPU stress test is running, it will automatically restart with the new mode:

```bash
# Switch from fresh-process to threaded
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"threaded"}'

# Switch to bursty mode
curl -X POST http://localhost:8080/start-cpu \
  -H 'Content-Type: application/json' \
  -d '{"mode":"bursty","utilization":50}'
```

## Installation

**Download and run (Linux AMD64):**
```bash
curl -L https://files.tensorturtle.com/yundera-cpu-stress/cpu-stress-linux-amd64 -o cpu-stress && chmod +x cpu-stress && ./cpu-stress
```

**Install from crates.io:**
```bash
cargo install distributed-cpu-stress-reporter
```

**Build from source:**
```bash
git clone https://github.com/tensorturtle/distributed-cpu-stress-reporter.git
cd distributed-cpu-stress-reporter
cargo build --release
./target/release/distributed-cpu-stress-reporter
```

## FAQ

**Q: Will this harm my CPU?**
A: No. Standard CPU stress test like Prime95.

**Q: How do I stop it?**
A: `curl -X POST http://localhost:8080/end-cpu` or `Ctrl+C` to exit the application

**Q: Which mode should I use?**
A:
- **Fresh-process mode** (default): Most testing scenarios, especially when comparing multiple instances
- **Threaded mode**: Maximum performance or single instance testing
- **Bursty mode**: Realistic workload patterns, testing CPU responsiveness during bursts, simulating desktop/mixed workloads

**Q: Can I change the port?**
A: Edit `src/main.rs:110` and rebuild.

**Q: Works on Windows/macOS/Linux?**
A: Yes, all platforms Rust supports.

## Troubleshooting

**Port already in use:**
```bash
lsof -i :8080  # Find what's using the port
```

**Can't access from another machine:**
```bash
sudo ufw allow 8080/tcp  # Open firewall
```

**Low performance:**
```bash
# Check CPU governor (Linux)
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
# Set to performance mode if needed
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

## Performance Expectations

- Modern CPUs: ~200k-500k ops/sec per core
- 2x overprovisioning: ~50% performance drop
- Higher = better, consistency indicates fairness

**Tip:** Run on bare metal first to establish baseline.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
