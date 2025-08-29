/**
 * MIT License
 *
 * Copyright (c) 2025 Takatoshi Kondo
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use std::sync::Once;

static INIT: Once = Once::new();

/// Automatic tracing initialization for ALL tests
///
/// Environment variables:
/// - `RUST_LOG`: Standard Rust logging (takes precedence if set)
/// - `MQTT_LOG_LEVEL`: Set log level (trace, debug, info, warn, error). Default: warn
///
/// Usage examples:
/// - `cargo test --features tracing` (default warn level)
/// - `MQTT_LOG_LEVEL=trace cargo test --features tracing`
/// - `RUST_LOG=debug cargo test --features tracing`
fn auto_init_tracing() {
    INIT.call_once(|| {
        // Try RUST_LOG first, then MQTT_LOG_LEVEL, then default to warn
        let filter = if let Ok(rust_log) = std::env::var("RUST_LOG") {
            tracing_subscriber::EnvFilter::new(rust_log)
        } else {
            let level = std::env::var("MQTT_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string());
            tracing_subscriber::EnvFilter::new(format!("mqtt_endpoint_tokio={level}"))
        };

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_test_writer()
            .init();
    });
}

pub fn init_tracing() {
    auto_init_tracing();
}
