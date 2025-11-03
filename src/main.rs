use axum::{routing::{get, post}, Router};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
    let num_cores = num_cpus::get();

    println!("Distributed CPU Stress Reporter");
    println!("Worker threads ready on {} cores", num_cores);
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

    // Spawn worker threads for each CPU core
    for i in 0..num_cores {
        let state_clone = Arc::clone(&state);
        thread::spawn(move || {
            println!("Worker thread {} ready", i);
            cpu_worker(state_clone);
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
