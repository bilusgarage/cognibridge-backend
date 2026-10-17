// ==============================================================================
// 1. MODULE IMPORTS (CRATES & STANDARD LIBRARIES)
// ==============================================================================

// Axum is the modern web framework for Rust built on top of Tokio and Hyper.
use axum::{
    extract::Multipart,                    // Extractor to parse incoming file uploads (multipart/form-data)
    http::{header, Method, StatusCode},     // HTTP primitives (headers, GET/POST methods, status codes like 200, 400)
    response::Json,                        // Helper to serialize Rust data structures into JSON HTTP responses
    routing::{get, post},                  // Route builder functions for GET and POST HTTP methods
    Router,                                // The core router that maps URL paths (e.g. /api/scan) to functions
};

// Serde is the serialization/deserialization framework for Rust.
// Serialize: Converts Rust structs into JSON strings.
// Deserialize: Converts JSON strings into Rust structs.
use serde::{Deserialize, Serialize};

// Tower-HTTP provides middleware components, here used for CORS (Cross-Origin Resource Sharing).
use tower_http::cors::{Any, CorsLayer};

// ==============================================================================
// 2. DATA STRUCTURES (MODELS)
// ==============================================================================

// #[derive(...)] is a Rust macro that automatically generates implementation code for traits:
// - Serialize: allows Axum to convert this struct directly into a JSON response body.
// - Deserialize: allows converting JSON into this struct if needed.
// - Debug: allows printing this struct to the console using println!("{:?}", response).
#[derive(Serialize, Deserialize, Debug)]
pub struct AckResponse {
    pub status: String,            // "ACK" confirming success
    pub message: String,           // Human-readable message: "Backend ACK success"
    pub bytes_received: usize,     // Total size of the uploaded photo in bytes
    pub filename: Option<String>,  // The original filename sent by frontend (e.g. "document_123.jpg")
    pub timestamp: String,         // Time when the backend processed the photo
}

// ==============================================================================
// 3. APPLICATION ENTRY POINT
// ==============================================================================

// #[tokio::main] is a macro that initializes the Tokio async runtime.
// Because Rust's standard library does not include an async runtime by default,
// Tokio provides the multi-threaded event loop that handles thousands of concurrent requests.
#[tokio::main]
async fn main() {
    // --------------------------------------------------------------------------
    // Step A: Configure CORS (Cross-Origin Resource Sharing)
    // By default, web browsers block web pages on http://localhost:5173 from making
    // requests to servers on a different port like http://localhost:3001.
    // This CORS layer tells the browser: "Requests from any origin are welcome here!"
    // --------------------------------------------------------------------------
    let cors = CorsLayer::new()
        .allow_origin(Any) // Allow any client (like our Vite dev server on port 5173)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS]) // Allow GET, POST, and preflight OPTIONS
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT]); // Allow common HTTP headers

    // --------------------------------------------------------------------------
    // Step B: Build the Router
    // We register URL endpoints and connect them to their handler functions:
    // - GET  /api/health -> health_check
    // - POST /api/scan   -> handle_scan
    // --------------------------------------------------------------------------
    let app = Router::new()
        .route("/api/health", get(health_check)) // Simple health check endpoint
        .route("/api/scan", post(handle_scan))   // Document photo upload endpoint
        .layer(cors);                            // Attach the CORS middleware to all routes

    // --------------------------------------------------------------------------
    // Step C: Bind TCP Listener & Start Listening on Port 3001
    // --------------------------------------------------------------------------
    let addr = "127.0.0.1:3001";
    println!("🚀 [cognibridge-backend] Starting server on http://{}", addr);

    // Bind a TCP socket listener to 127.0.0.1:3001 using Tokio
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind TCP listener on port 3001");

    println!("✅ [cognibridge-backend] Server is ready to receive documents at POST http://{}/api/scan", addr);

    // axum::serve runs the server event loop indefinitely, accepting incoming HTTP requests
    axum::serve(listener, app)
        .await
        .expect("Server error");
}

// ==============================================================================
// 4. ROUTE HANDLERS
// ==============================================================================

// Simple health check handler: returns {"status": "online", "service": "cognibridge-backend"}
async fn health_check() -> Json<serde_json::Value> {
    // serde_json::json! is a handy macro to write inline JSON without defining a dedicated struct
    Json(serde_json::json!({
        "status": "online",
        "service": "cognibridge-backend"
    }))
}

// Handler for POST /api/scan: receives the photo and responds with "ACK"
// Return type is a Result:
// - Ok:  Json<AckResponse> (Status 200 OK with structured ACK)
// - Err: (StatusCode, Json<Value>) (Status 400 or 500 with failure reason)
async fn handle_scan(
    mut multipart: Multipart, // Axum automatically extracts and parses the multipart/form-data stream
) -> Result<Json<AckResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut total_bytes = 0;              // Counter for total bytes of photo data received
    let mut received_filename = None;     // Holds the filename from the client, if provided

    // Loop through each field inside the multipart form data as they stream in asynchronously
    while let Ok(Some(field)) = multipart.next_field().await {
        // Read field name (e.g., 'photo', 'file', or 'document')
        let name = field.name().unwrap_or("").to_string();

        if name == "photo" || name == "file" || name == "document" || name.is_empty() {
            // Extract the original filename if the browser provided one
            if let Some(fname) = field.file_name() {
                received_filename = Some(fname.to_string());
            }

            // Read the binary byte chunk of the uploaded image
            match field.bytes().await {
                Ok(bytes) => {
                    total_bytes += bytes.len();
                    println!(
                        "📸 [cognibridge-backend] Received photo chunk: {} bytes (filename: {:?})",
                        bytes.len(),
                        received_filename
                    );
                }
                Err(err) => {
                    // If network stream gets interrupted or corrupted, return a 500 Internal Server Error
                    eprintln!("❌ [cognibridge-backend] Error reading multipart bytes: {}", err);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "status": "NACK",
                            "message": format!("No ACK: failed - {}", err)
                        })),
                    ));
                }
            }
        }
    }

    // If we received valid photo bytes, send back the ACK acknowledgment
    if total_bytes > 0 {
        let now = chrono_now_string();
        println!(
            "✅ [cognibridge-backend] Successfully processed document photo ({} bytes). Sending ACK...",
            total_bytes
        );

        // Return HTTP 200 OK with the ACK response payload
        Ok(Json(AckResponse {
            status: "ACK".to_string(),
            message: "Backend ACK success".to_string(),
            bytes_received: total_bytes,
            filename: received_filename,
            timestamp: now,
        }))
    } else {
        // If the request had no photo bytes attached, return HTTP 400 Bad Request
        eprintln!("⚠️ [cognibridge-backend] Received scan request without photo bytes");
        Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "NACK",
                "message": "No ACK: failed - Empty photo payload received"
            })),
        ))
    }
}

// ==============================================================================
// 5. HELPER FUNCTIONS
// ==============================================================================

// Generates a readable timestamp string using the Rust standard library
fn chrono_now_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}s epoch", duration.as_secs(), duration.subsec_millis())
}
