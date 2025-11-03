use axum::{routing::get, Router};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// Shared state for performance metrics
struct AppState {
    operations_per_second: AtomicU64,
    current_counter: Arc<AtomicU64>,
}

// Simple prime number check using trial division
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let limit = (n as f64).sqrt() as u64;
    for i in (3..=limit).step_by(2) {
        if n % i == 0 {
            return false;
        }
    }
    true
}

// CPU-bound worker that continuously calculates primes
fn cpu_worker(counter: Arc<AtomicU64>) {
    let mut n = 2u64;
    loop {
        if is_prime(n) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        n = n.wrapping_add(1);
        if n < 2 {
            n = 2; // Reset on overflow
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

#[tokio::main]
async fn main() {
    let num_cores = num_cpus::get();

    println!("Distributed CPU Stress Reporter");
    println!("Starting CPU stress on {} cores", num_cores);
    println!("HTTP server listening on 0.0.0.0:8080");
    println!("Query endpoint: http://localhost:8080/cpu-perf");
    println!();

    // Create shared state
    let state = Arc::new(AppState {
        operations_per_second: AtomicU64::new(0),
        current_counter: Arc::new(AtomicU64::new(0)),
    });

    // Spawn worker threads for each CPU core
    for i in 0..num_cores {
        let counter = Arc::clone(&state.current_counter);
        thread::spawn(move || {
            println!("Worker thread {} started", i);
            cpu_worker(counter);
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
        .with_state(state);

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind to port 8080");

    println!("Ready to serve requests");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
