use axum::{routing::{get, post}, Router};
use clap::Parser;
use rand_distr::{Distribution, Exp};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ExecutionMode {
    Threaded,
    FreshProcess,
    Bursty,
}

#[derive(Debug, Deserialize)]
struct StartCpuRequest {
    mode: ExecutionMode,
    /// Optional utilization percentage for bursty mode (0-100, default 50)
    utilization: Option<f64>,
}

#[derive(Parser, Debug)]
#[command(name = "distributed-cpu-stress-reporter")]
#[command(about = "CPU stress testing and performance reporting", long_about = None)]
struct Args {
    /// Internal: Run as worker process (do not use directly)
    #[arg(long, hide = true)]
    worker: bool,

    /// Internal: Number of operations for worker to perform
    #[arg(long, hide = true, default_value = "20000")]
    worker_ops: u64,
}

// Shared state for performance metrics
struct AppState {
    operations_per_second: AtomicU64,
    current_counter: Arc<AtomicU64>,
    is_running: AtomicBool,
    execution_mode: Mutex<ExecutionMode>,
    // Bursty mode state
    burst_phase: AtomicBool,
    burst_total_ops: AtomicU64,
    burst_total_time_ms: AtomicU64,
    burst_ops_per_second: AtomicU64,
    bursty_utilization: Mutex<f64>,
}

// Simple prime number check using trial division
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n.is_multiple_of(2) {
        return false;
    }
    let limit = (n as f64).sqrt() as u64;
    for i in (3..=limit).step_by(2) {
        if n.is_multiple_of(i) {
            return false;
        }
    }
    true
}

// CPU-bound worker that continuously calculates primes
fn cpu_worker(state: Arc<AppState>) {
    let mut n = 2u64;
    loop {
        // Check if we should be running AND in threaded mode
        let is_active = state.is_running.load(Ordering::Relaxed)
            && *state.execution_mode.lock().unwrap() == ExecutionMode::Threaded;

        if is_active {
            if is_prime(n) {
                state.current_counter.fetch_add(1, Ordering::Relaxed);
            }
            n = n.wrapping_add(1);
            if n < 2 {
                n = 2; // Reset on overflow
            }
        } else {
            // When not running or not in correct mode, sleep briefly to avoid busy-waiting
            thread::sleep(Duration::from_millis(100));
        }
    }
}

// Sampling thread that measures operations per second
fn sampler(state: Arc<AppState>) {
    loop {
        thread::sleep(Duration::from_secs(1));

        // Get current counter value and reset it
        let ops = state.current_counter.swap(0, Ordering::Relaxed);

        // Store as operations per second
        state.operations_per_second.store(ops, Ordering::Relaxed);
    }
}

// Worker mode: Run a fixed amount of work and exit
fn run_worker(num_ops: u64) {
    let mut count = 0u64;
    let mut n = 2u64;

    while count < num_ops {
        if is_prime(n) {
            count += 1;
        }
        n = n.wrapping_add(1);
        if n < 2 {
            n = 2; // Reset on overflow
        }
    }

    // Print the number of operations performed
    println!("{}", count);
}

// Fresh-process mode: Spawn child processes continuously
fn process_spawner(state: Arc<AppState>, core_id: usize, worker_ops: u64) {
    let exe_path = std::env::current_exe().expect("Failed to get current executable path");

    loop {
        // Check if we should be running AND in fresh-process mode
        let is_active = state.is_running.load(Ordering::Relaxed)
            && *state.execution_mode.lock().unwrap() == ExecutionMode::FreshProcess;

        if !is_active {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Spawn child process
        let output = Command::new(&exe_path)
            .arg("--worker")
            .arg("--worker-ops")
            .arg(worker_ops.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    // Parse the operation count from stdout
                    if let Ok(stdout) = String::from_utf8(output.stdout) {
                        if let Ok(ops) = stdout.trim().parse::<u64>() {
                            state.current_counter.fetch_add(ops, Ordering::Relaxed);
                        }
                    }
                } else {
                    eprintln!("Worker process {} failed with status: {}", core_id, output.status);
                }
            }
            Err(e) => {
                eprintln!("Failed to spawn worker process {}: {}", core_id, e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

// Burst metrics calculator: Calculates ops/sec based on accumulated burst time
fn burst_metrics_calculator(state: Arc<AppState>) {
    loop {
        thread::sleep(Duration::from_secs(1));

        // Only calculate when in bursty mode
        if *state.execution_mode.lock().unwrap() == ExecutionMode::Bursty {
            let total_ops = state.burst_total_ops.load(Ordering::Relaxed);
            let total_time_ms = state.burst_total_time_ms.load(Ordering::Relaxed);

            if total_time_ms > 0 {
                // Calculate ops per second: (total_ops * 1000) / total_time_ms
                let ops_per_sec = (total_ops * 1000) / total_time_ms;
                state.burst_ops_per_second.store(ops_per_sec, Ordering::Relaxed);
            }
        }
    }
}

// Burst coordinator: Controls burst/idle phases with exponential distribution
fn burst_coordinator(state: Arc<AppState>, num_cores: usize, worker_ops: u64) {
    let exe_path = std::env::current_exe().expect("Failed to get current executable path");
    let mut rng = rand::thread_rng();
    // Exponential distribution with lambda=0.67 gives mean ~1.5s
    let exp_dist = Exp::new(0.67).expect("Failed to create exponential distribution");

    loop {
        // Check if we should be running AND in bursty mode
        let is_active = state.is_running.load(Ordering::Relaxed)
            && *state.execution_mode.lock().unwrap() == ExecutionMode::Bursty;

        if !is_active {
            state.burst_phase.store(false, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Sample burst duration from exponential distribution, clamp to [500ms, 5s]
        let burst_duration_secs = exp_dist.sample(&mut rng);
        let burst_duration_secs = f64::clamp(burst_duration_secs, 0.5, 5.0);
        let burst_duration = Duration::from_secs_f64(burst_duration_secs);

        // Enter burst phase
        state.burst_phase.store(true, Ordering::Relaxed);
        let burst_start = Instant::now();

        // Spawn fresh worker processes (one per core)
        let mut children = Vec::new();
        for core_id in 0..num_cores {
            match Command::new(&exe_path)
                .arg("--worker")
                .arg("--worker-ops")
                .arg(worker_ops.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => children.push((core_id, child)),
                Err(e) => eprintln!("Failed to spawn burst worker {}: {}", core_id, e),
            }
        }

        // Let processes run for the burst duration
        thread::sleep(burst_duration);

        // Exit burst phase and measure actual elapsed time
        let burst_elapsed = burst_start.elapsed();
        state.burst_phase.store(false, Ordering::Relaxed);

        // Collect results from worker processes
        let mut total_ops_this_burst = 0u64;
        for (core_id, child) in children {
            match child.wait_with_output() {
                Ok(output) if output.status.success() => {
                    if let Ok(stdout) = String::from_utf8(output.stdout) {
                        if let Ok(ops) = stdout.trim().parse::<u64>() {
                            total_ops_this_burst += ops;
                        }
                    }
                }
                Ok(output) => {
                    eprintln!("Burst worker {} exited with status: {}", core_id, output.status);
                }
                Err(e) => {
                    eprintln!("Failed to wait for burst worker {}: {}", core_id, e);
                }
            }
        }

        // Accumulate totals for time-aware metrics
        state.burst_total_ops.fetch_add(total_ops_this_burst, Ordering::Relaxed);
        state.burst_total_time_ms.fetch_add(burst_elapsed.as_millis() as u64, Ordering::Relaxed);

        // Calculate idle duration to maintain target utilization
        let utilization = *state.bursty_utilization.lock().unwrap();
        let idle_duration_secs = burst_duration_secs * (1.0 - utilization) / utilization;
        let idle_duration = Duration::from_secs_f64(idle_duration_secs);

        // Idle period (no processes running)
        thread::sleep(idle_duration);
    }
}

// HTTP handler for /cpu-perf endpoint
async fn cpu_perf_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> String {
    let ops = state.operations_per_second.load(Ordering::Relaxed);
    format!("{}\n", ops)
}

// HTTP handler for /burst-perf endpoint - returns burst-only performance metrics
async fn burst_perf_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> String {
    let ops = state.burst_ops_per_second.load(Ordering::Relaxed);
    format!("{}\n", ops)
}

// HTTP handler for POST /start-cpu endpoint
async fn start_cpu_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::Json(request): axum::Json<StartCpuRequest>,
) -> String {
    let current_mode = *state.execution_mode.lock().unwrap();
    let requested_mode = request.mode;
    let is_running = state.is_running.load(Ordering::Relaxed);

    // Handle utilization for bursty mode
    if requested_mode == ExecutionMode::Bursty {
        let utilization_pct = request.utilization.unwrap_or(50.0);

        // Validate utilization is between 0 and 100
        if utilization_pct <= 0.0 || utilization_pct > 100.0 {
            return format!("Error: utilization must be between 0 and 100 (got {})\n", utilization_pct);
        }

        // Store as fraction (0.0-1.0)
        *state.bursty_utilization.lock().unwrap() = utilization_pct / 100.0;
    }

    // If already running with a different mode, we need to restart
    if is_running && current_mode != requested_mode {
        println!("Mode change requested while running. Stopping, changing mode, and restarting...");

        // Stop current workers
        state.is_running.store(false, Ordering::Relaxed);

        // Reset counters
        state.current_counter.store(0, Ordering::Relaxed);
        state.operations_per_second.store(0, Ordering::Relaxed);
        state.burst_total_ops.store(0, Ordering::Relaxed);
        state.burst_total_time_ms.store(0, Ordering::Relaxed);
        state.burst_ops_per_second.store(0, Ordering::Relaxed);

        // Wait a moment for workers to notice the stop
        thread::sleep(Duration::from_millis(200));

        // Change mode
        *state.execution_mode.lock().unwrap() = requested_mode;

        // Start with new mode
        state.is_running.store(true, Ordering::Relaxed);

        if requested_mode == ExecutionMode::Bursty {
            let util_pct = *state.bursty_utilization.lock().unwrap() * 100.0;
            println!("CPU stress test RESTARTED with mode: {:?}, utilization: {:.0}%", requested_mode, util_pct);
            format!("CPU stress test restarted with mode: {:?}, utilization: {:.0}%\n", requested_mode, util_pct)
        } else {
            println!("CPU stress test RESTARTED with mode: {:?}", requested_mode);
            format!("CPU stress test restarted with mode: {:?}\n", requested_mode)
        }
    } else if is_running && current_mode == requested_mode {
        // Already running with the requested mode
        if requested_mode == ExecutionMode::Bursty {
            let util_pct = *state.bursty_utilization.lock().unwrap() * 100.0;
            format!("CPU stress test already running with mode: {:?}, utilization: {:.0}%\n", current_mode, util_pct)
        } else {
            format!("CPU stress test already running with mode: {:?}\n", current_mode)
        }
    } else {
        // Not running, so set mode and start
        *state.execution_mode.lock().unwrap() = requested_mode;
        state.is_running.store(true, Ordering::Relaxed);

        if requested_mode == ExecutionMode::Bursty {
            let util_pct = *state.bursty_utilization.lock().unwrap() * 100.0;
            println!("CPU stress test STARTED with mode: {:?}, utilization: {:.0}%", requested_mode, util_pct);
            format!("CPU stress test started with mode: {:?}, utilization: {:.0}%\n", requested_mode, util_pct)
        } else {
            println!("CPU stress test STARTED with mode: {:?}", requested_mode);
            format!("CPU stress test started with mode: {:?}\n", requested_mode)
        }
    }
}

// HTTP handler for POST /end-cpu endpoint
async fn end_cpu_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> String {
    // Idempotent stop - always returns success
    state.is_running.store(false, Ordering::Relaxed);
    // Reset all counters when stopping
    state.current_counter.store(0, Ordering::Relaxed);
    state.operations_per_second.store(0, Ordering::Relaxed);
    state.burst_total_ops.store(0, Ordering::Relaxed);
    state.burst_total_time_ms.store(0, Ordering::Relaxed);
    state.burst_ops_per_second.store(0, Ordering::Relaxed);
    println!("CPU stress test STOPPED");
    "CPU stress test stopped\n".to_string()
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // If running in worker mode, do the work and exit
    if args.worker {
        run_worker(args.worker_ops);
        return;
    }

    let num_cores = num_cpus::get();

    println!("Distributed CPU Stress Reporter");
    println!("Worker threads/processes: {} (one per core)", num_cores);
    println!("HTTP server listening on [::]:8080 (IPv4 and IPv6)");
    println!();
    println!("Control endpoints:");
    println!("  POST http://localhost:8080/start-cpu - Start CPU stress test (requires JSON body with mode)");
    println!("       Examples:");
    println!("         curl -X POST http://localhost:8080/start-cpu -H 'Content-Type: application/json' -d '{{\"mode\":\"threaded\"}}'");
    println!("         curl -X POST http://localhost:8080/start-cpu -H 'Content-Type: application/json' -d '{{\"mode\":\"fresh-process\"}}'");
    println!("         curl -X POST http://localhost:8080/start-cpu -H 'Content-Type: application/json' -d '{{\"mode\":\"bursty\",\"utilization\":60}}'");
    println!("       Modes: \"threaded\", \"fresh-process\", or \"bursty\"");
    println!("       Bursty mode options: \"utilization\" (0-100, default 50) - target CPU utilization percentage");
    println!("  POST http://localhost:8080/end-cpu   - Stop CPU stress test");
    println!("Query endpoints:");
    println!("  GET  http://localhost:8080/cpu-perf   - Get operations per second (threaded/fresh-process modes)");
    println!("  GET  http://localhost:8080/burst-perf - Get burst-only operations per second (bursty mode)");
    println!();
    println!("CPU stress test is currently STOPPED. Send POST to /start-cpu with mode to begin.");
    println!();

    // Create shared state with CPU stress initially stopped, default to fresh-process mode
    let state = Arc::new(AppState {
        operations_per_second: AtomicU64::new(0),
        current_counter: Arc::new(AtomicU64::new(0)),
        is_running: AtomicBool::new(false),
        execution_mode: Mutex::new(ExecutionMode::FreshProcess),
        burst_phase: AtomicBool::new(false),
        burst_total_ops: AtomicU64::new(0),
        burst_total_time_ms: AtomicU64::new(0),
        burst_ops_per_second: AtomicU64::new(0),
        bursty_utilization: Mutex::new(0.5), // Default 50% utilization
    });

    // Spawn BOTH types of workers - they'll activate based on the execution_mode
    // Threaded workers
    for i in 0..num_cores {
        let state_clone = Arc::clone(&state);
        thread::spawn(move || {
            println!("Threaded worker {} ready (inactive until mode=threaded)", i);
            cpu_worker(state_clone);
        });
    }

    // Fresh-process spawners
    for i in 0..num_cores {
        let state_clone = Arc::clone(&state);
        let worker_ops = args.worker_ops;
        thread::spawn(move || {
            println!("Fresh-process spawner {} ready (inactive until mode=fresh-process)", i);
            process_spawner(state_clone, i, worker_ops);
        });
    }

    // Spawn sampling thread
    {
        let state_clone = Arc::clone(&state);
        thread::spawn(move || {
            sampler(state_clone);
        });
    }

    // Spawn burst coordinator thread
    {
        let state_clone = Arc::clone(&state);
        thread::spawn(move || {
            println!("Burst coordinator ready (inactive until mode=bursty)");
            burst_coordinator(state_clone, num_cores, args.worker_ops);
        });
    }

    // Spawn burst metrics calculator thread
    {
        let state_clone = Arc::clone(&state);
        thread::spawn(move || {
            burst_metrics_calculator(state_clone);
        });
    }

    // Wait a moment for threads to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Build HTTP router
    let app = Router::new()
        .route("/cpu-perf", get(cpu_perf_handler))
        .route("/burst-perf", get(burst_perf_handler))
        .route("/start-cpu", post(start_cpu_handler))
        .route("/end-cpu", post(end_cpu_handler))
        .with_state(state);

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind("[::]:8080")
        .await
        .expect("Failed to bind to port 8080");

    println!("Ready to serve requests");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
