# Distributed CPU Stress Reporter

**Measure real CPU performance in virtualized environments.**

A lightweight HTTP server that stress-tests all CPU cores and reports actual performance metrics. Perfect for testing CPU overprovisioning in Proxmox, VMware, or any virtualized infrastructure.

## Quick Start

```bash
# Clone and build
git clone https://github.com/tensorturtle/distributed-cpu-stress-reporter.git
cd distributed-cpu-stress-reporter
cargo build --release

# Run
./target/release/distributed-cpu-stress-reporter
```

The server will start on port 8080. Query it from anywhere:

```bash
curl http://localhost:8080/cpu-perf
```

**Response:**
```
254060
```

This number is the **operations per second** your CPU is actually achieving right now.

## Why Use This?

### Problem
You have a Proxmox host with 8 physical cores, but you've created 4 VMs with 4 vCPUs each (16 total). Are your VMs getting the CPU performance they expect? Is one VM stealing CPU from others?

### Solution
Run this app in each VM. It maxes out all cores and tells you the actual performance via a simple HTTP endpoint.

## Usage Examples

### Example 1: Test a Single VM

**Inside the VM:**
```bash
cargo run --release
```

**Output:**
```
Distributed CPU Stress Reporter
Starting CPU stress on 4 cores
HTTP server listening on 0.0.0.0:8080
Query endpoint: http://localhost:8080/cpu-perf

Worker thread 0 started
Worker thread 1 started
Worker thread 2 started
Worker thread 3 started
Ready to serve requests
```

**From your hypervisor or workstation:**
```bash
curl http://192.168.1.100:8080/cpu-perf
# Returns: 145823
```

### Example 2: Monitor Multiple VMs

Create a simple monitoring script:

```bash
#!/bin/bash
# monitor-vms.sh

echo "VM Performance Monitor"
echo "====================="
while true; do
  clear
  date
  echo ""
  echo "VM1 (192.168.1.101): $(curl -s http://192.168.1.101:8080/cpu-perf) ops/sec"
  echo "VM2 (192.168.1.102): $(curl -s http://192.168.1.102:8080/cpu-perf) ops/sec"
  echo "VM3 (192.168.1.103): $(curl -s http://192.168.1.103:8080/cpu-perf) ops/sec"
  echo "VM4 (192.168.1.104): $(curl -s http://192.168.1.104:8080/cpu-perf) ops/sec"
  sleep 2
done
```

**Sample output:**
```
VM Performance Monitor
=====================
Sun Nov  3 16:30:45 UTC 2025

VM1 (192.168.1.101): 245123 ops/sec
VM2 (192.168.1.102): 243891 ops/sec
VM3 (192.168.1.103): 122456 ops/sec  ← This VM is throttled!
VM4 (192.168.1.104): 244567 ops/sec
```

### Example 3: Test CPU Overprovisioning Impact

**Scenario:** Start with 2 VMs, then add 2 more to see performance degradation.

```bash
# Initially with 2 VMs running:
curl http://vm1:8080/cpu-perf  # 480000 ops/sec
curl http://vm2:8080/cpu-perf  # 478000 ops/sec

# After starting 2 more VMs with the stress app:
curl http://vm1:8080/cpu-perf  # 240000 ops/sec ← Performance halved!
curl http://vm2:8080/cpu-perf  # 238000 ops/sec
curl http://vm3:8080/cpu-perf  # 239000 ops/sec
curl http://vm4:8080/cpu-perf  # 241000 ops/sec
```

### Example 4: Integration with Prometheus

The endpoint returns plain numbers, making it easy to scrape:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'vm-cpu-performance'
    scrape_interval: 5s
    metrics_path: '/cpu-perf'
    static_configs:
      - targets:
        - '192.168.1.101:8080'
        - '192.168.1.102:8080'
        - '192.168.1.103:8080'
        - '192.168.1.104:8080'
    metric_relabel_configs:
      - source_labels: [__address__]
        target_label: vm_instance
```

### Example 5: One-Liner Performance Check

```bash
# Check all VMs at once
for vm in 192.168.1.{101..104}; do echo -n "$vm: "; curl -s http://$vm:8080/cpu-perf; done
```

**Output:**
```
192.168.1.101: 245123
192.168.1.102: 243891
192.168.1.103: 244567
192.168.1.104: 246012
```

## What The Numbers Mean

- **Higher = Better**: More operations per second means better CPU performance
- **Consistency**: Similar VMs should show similar numbers
- **Degradation**: A sudden drop indicates CPU contention or throttling
- **Baseline**: Run on bare metal first to establish expected performance

## Installation Options

### Option 1: Build from Source (Recommended)
```bash
git clone https://github.com/tensorturtle/distributed-cpu-stress-reporter.git
cd distributed-cpu-stress-reporter
cargo build --release
./target/release/distributed-cpu-stress-reporter
```

### Option 2: Direct Run (Development)
```bash
git clone https://github.com/tensorturtle/distributed-cpu-stress-reporter.git
cd distributed-cpu-stress-reporter
cargo run --release
```

### Option 3: Deploy Binary
After building, copy the binary to your VMs:
```bash
# On build machine
scp target/release/distributed-cpu-stress-reporter user@vm-ip:/usr/local/bin/

# On each VM
chmod +x /usr/local/bin/distributed-cpu-stress-reporter
distributed-cpu-stress-reporter
```

## Common Use Cases

### ✓ Test Proxmox CPU Overprovisioning
See real impact when allocating more vCPUs than physical cores

### ✓ Identify CPU-Throttled VMs
Find which VMs are getting less CPU than expected

### ✓ Benchmark VM Performance
Compare CPU performance across different hosts or configurations

### ✓ Monitor CPU Contention Over Time
Track performance degradation as workloads change

### ✓ Validate Resource Allocation
Verify VMs are getting their fair share of CPU

## Performance Expectations

Results will vary based on:
- **CPU Architecture**: Modern CPUs (Ryzen 5000+, Intel 12th gen+) = 200k-500k ops/sec per core
- **Overprovisioning**: 2x overprovisioned = ~50% performance
- **VM Configuration**: CPU pinning vs floating scheduling
- **Host Load**: Other VMs and processes competing for CPU

**Tip**: Run on bare metal first to establish a baseline, then compare VM performance against it.

## FAQ

**Q: Will this harm my CPU?**
A: No. This is a standard CPU stress test, similar to Prime95 or stress-ng. Modern CPUs are designed to run at 100% indefinitely.

**Q: How do I stop it?**
A: Press `Ctrl+C` in the terminal where it's running.

**Q: Can I change the port?**
A: Currently, port 8080 is hardcoded. Fork the repo and modify `src/main.rs:110` if needed.

**Q: Does it work on Windows/macOS/Linux?**
A: Yes! It works on any platform that Rust supports.

**Q: Why are my numbers lower than expected?**
A: Check: CPU throttling (thermal), power-saving mode, background processes, or CPU overprovisioning.

**Q: Can I run multiple instances?**
A: Yes, but they'll compete for CPU and each will show ~50% performance. Change the port for the second instance.

**Q: How accurate is this?**
A: Very accurate for comparing relative performance. Absolute numbers vary by CPU architecture but are consistent for the same CPU.

## Troubleshooting

**Port 8080 already in use:**
```bash
# Find what's using port 8080
lsof -i :8080
# Or on Linux
ss -tulpn | grep 8080
```

**Can't access from another machine:**
```bash
# Check firewall (example for Ubuntu)
sudo ufw allow 8080/tcp

# Verify server is listening on 0.0.0.0
netstat -tulpn | grep 8080
```

**Low performance numbers:**
```bash
# Check CPU governor (Linux)
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
# Should be "performance", not "powersave"

# Set to performance mode
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

## How It Works

The application uses a simple but effective architecture:

1. **Worker threads** (one per CPU core) continuously calculate prime numbers
2. **Atomic counter** tracks total operations across all threads (lock-free)
3. **Sampler thread** reads and resets the counter every second
4. **HTTP server** (Axum + Tokio) serves the latest metric at `/cpu-perf`

**Why prime numbers?** Pure CPU computation with no I/O bottlenecks, predictable workload that scales with CPU speed.

**Technical details:** See [CLAUDE.md](CLAUDE.md) for architecture documentation.

## License

This project is open source and available for use in testing and monitoring scenarios.
