use axum::{routing::{get, post}, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ExecutionMode {
    Threaded,
    FreshProcess,
}

#[derive(Debug, Deserialize)]
struct StartCpuRequest {
    mode: ExecutionMode,
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

// HTTP handler for /cpu-perf endpoint
async fn cpu_perf_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> String {
    let ops = state.operations_per_second.load(Ordering::Relaxed);
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

    // If already running with a different mode, we need to restart
    if is_running && current_mode != requested_mode {
        println!("Mode change requested while running. Stopping, changing mode, and restarting...");

        // Stop current workers
        state.is_running.store(false, Ordering::Relaxed);

        // Reset counters
        state.current_counter.store(0, Ordering::Relaxed);
        state.operations_per_second.store(0, Ordering::Relaxed);

        // Wait a moment for workers to notice the stop
        thread::sleep(Duration::from_millis(200));

        // Change mode
        *state.execution_mode.lock().unwrap() = requested_mode;

        // Start with new mode
        state.is_running.store(true, Ordering::Relaxed);

        println!("CPU stress test RESTARTED with mode: {:?}", requested_mode);
        format!("CPU stress test restarted with mode: {:?}\n", requested_mode)
    } else if is_running && current_mode == requested_mode {
        // Already running with the requested mode
        format!("CPU stress test already running with mode: {:?}\n", current_mode)
    } else {
        // Not running, so set mode and start
        *state.execution_mode.lock().unwrap() = requested_mode;
        state.is_running.store(true, Ordering::Relaxed);

        println!("CPU stress test STARTED with mode: {:?}", requested_mode);
        format!("CPU stress test started with mode: {:?}\n", requested_mode)
    }
}

// HTTP handler for POST /end-cpu endpoint
async fn end_cpu_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> String {
    // Idempotent stop - always returns success
    state.is_running.store(false, Ordering::Relaxed);
    // Reset the counter and operations per second when stopping
    state.current_counter.store(0, Ordering::Relaxed);
    state.operations_per_second.store(0, Ordering::Relaxed);
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
    println!("       Example: curl -X POST http://localhost:8080/start-cpu -H 'Content-Type: application/json' -d '{{\"mode\":\"threaded\"}}'");
    println!("       Modes: \"threaded\" or \"fresh-process\"");
    println!("  POST http://localhost:8080/end-cpu   - Stop CPU stress test");
    println!("Query endpoint:");
    println!("  GET  http://localhost:8080/cpu-perf  - Get operations per second");
    println!();
    println!("CPU stress test is currently STOPPED. Send POST to /start-cpu with mode to begin.");
    println!();

    // Create shared state with CPU stress initially stopped, default to fresh-process mode
    let state = Arc::new(AppState {
        operations_per_second: AtomicU64::new(0),
        current_counter: Arc::new(AtomicU64::new(0)),
        is_running: AtomicBool::new(false),
        execution_mode: Mutex::new(ExecutionMode::FreshProcess),
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

    // Wait a moment for threads to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Build HTTP router
    let app = Router::new()
        .route("/cpu-perf", get(cpu_perf_handler))
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
