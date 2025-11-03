# Distributed CPU Stress Reporter

Lightweight HTTP server that stress-tests CPU cores and reports performance metrics. Built for measuring CPU performance in virtualized environments.

## Quick Start

```bash
git clone https://github.com/tensorturtle/distributed-cpu-stress-reporter.git
cd distributed-cpu-stress-reporter
cargo build --release
./target/release/distributed-cpu-stress-reporter
```

Query performance:
```bash
curl http://localhost:8080/cpu-perf
# Returns: 254060 (operations/second)
```

## Use Case

Test CPU overprovisioning in VMs. Run this in multiple VMs on the same hypervisor to see how CPU contention affects actual performance.

**Example:** Proxmox host with 8 cores running 4 VMs with 4 vCPUs each (2x overprovisioned):

```bash
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
while true; do
  for vm in 192.168.1.{101..104}; do
    echo "$vm: $(curl -s http://$vm:8080/cpu-perf) ops/sec"
  done
  sleep 2
done
```

## How It Works

- Spawns one worker thread per CPU core
- Each thread continuously calculates prime numbers
- Atomic counter tracks operations per second (1-second intervals)
- HTTP server (Axum) serves latest metric at `/cpu-perf`

**Why prime numbers?** Pure CPU computation with no I/O - perfect for measuring CPU performance.

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
A: `Ctrl+C`

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
