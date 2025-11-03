use axum::{routing::{get, post}, Router};
use clap::Parser;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "distributed-cpu-stress-reporter")]
#[command(about = "CPU stress testing and performance reporting", long_about = None)]
struct Args {
    /// Use fresh-process mode to avoid scheduler catch-up bias
    #[arg(long)]
    fresh_process_mode: bool,

    /// Internal: Run as worker process (do not use directly)
    #[arg(long, hide = true)]
    worker: bool,

    /// Internal: Number of operations for worker to perform
    #[arg(long, hide = true, default_value = "100000")]
    worker_ops: u64,
}

// Shared state for performance metrics
struct AppState {
    operations_per_second: AtomicU64,
    current_counter: Arc<AtomicU64>,
    is_running: AtomicBool,
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
        // Check if we should be running
        if state.is_running.load(Ordering::Relaxed) {
            if is_prime(n) {
                state.current_counter.fetch_add(1, Ordering::Relaxed);
            }
            n = n.wrapping_add(1);
            if n < 2 {
                n = 2; // Reset on overflow
            }
        } else {
            // When not running, sleep briefly to avoid busy-waiting
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
        // Check if we should be running
        if !state.is_running.load(Ordering::Relaxed) {
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
) -> String {
    // Only start if not already running (ignore if already running)
    if !state.is_running.load(Ordering::Relaxed) {
        state.is_running.store(true, Ordering::Relaxed);
        println!("CPU stress test STARTED");
        "CPU stress test started\n".to_string()
    } else {
        "CPU stress test already running\n".to_string()
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
    if args.fresh_process_mode {
        println!("Mode: Fresh-Process (avoids scheduler catch-up bias)");
        println!("Worker processes: {} (one per core)", num_cores);
    } else {
        println!("Mode: Default (threaded)");
        println!("Worker threads: {} (one per core)", num_cores);
    }
    println!("HTTP server listening on [::]:8080 (IPv4 and IPv6)");
    println!();
    println!("Control endpoints:");
    println!("  POST http://localhost:8080/start-cpu - Start CPU stress test");
    println!("  POST http://localhost:8080/end-cpu   - Stop CPU stress test");
    println!("Query endpoint:");
    println!("  GET  http://localhost:8080/cpu-perf  - Get operations per second");
    println!();
    println!("CPU stress test is currently STOPPED. Send POST to /start-cpu to begin.");
    println!();

    // Create shared state with CPU stress initially stopped
    let state = Arc::new(AppState {
        operations_per_second: AtomicU64::new(0),
        current_counter: Arc::new(AtomicU64::new(0)),
        is_running: AtomicBool::new(false),
    });

    // Spawn workers based on mode
    if args.fresh_process_mode {
        // Fresh-process mode: spawn process spawners
        for i in 0..num_cores {
            let state_clone = Arc::clone(&state);
            let worker_ops = args.worker_ops;
            thread::spawn(move || {
                println!("Process spawner {} ready", i);
                process_spawner(state_clone, i, worker_ops);
            });
        }
    } else {
        // Default mode: spawn worker threads
        for i in 0..num_cores {
            let state_clone = Arc::clone(&state);
            thread::spawn(move || {
                println!("Worker thread {} ready", i);
                cpu_worker(state_clone);
            });
        }
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
