use axum::{
    extract::Multipart,
    http::{header, Method, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

#[derive(Serialize, Deserialize, Debug)]
pub struct AckResponse {
    pub status: String,
    pub message: String,
    pub bytes_received: usize,
    pub filename: Option<String>,
    pub timestamp: String,
}

#[tokio::main]
async fn main() {
    // Configure CORS so frontend running on localhost:5173 can communicate smoothly
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT]);

    let app = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/scan", post(handle_scan))
        .layer(cors);

    let addr = "127.0.0.1:3001";
    println!("🚀 [cognibridge-backend] Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind TCP listener on port 3001");

    println!("✅ [cognibridge-backend] Server is ready to receive documents at POST http://{}/api/scan", addr);

    axum::serve(listener, app)
        .await
        .expect("Server error");
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "online",
        "service": "cognibridge-backend"
    }))
}

async fn handle_scan(
    mut multipart: Multipart,
) -> Result<Json<AckResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut total_bytes = 0;
    let mut received_filename = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "photo" || name == "file" || name == "document" || name.is_empty() {
            if let Some(fname) = field.file_name() {
                received_filename = Some(fname.to_string());
            }

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

    if total_bytes > 0 {
        let now = chrono_now_string();
        println!(
            "✅ [cognibridge-backend] Successfully processed document photo ({} bytes). Sending ACK...",
            total_bytes
        );

        Ok(Json(AckResponse {
            status: "ACK".to_string(),
            message: "Backend ACK success".to_string(),
            bytes_received: total_bytes,
            filename: received_filename,
            timestamp: now,
        }))
    } else {
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

fn chrono_now_string() -> String {
    // Generate a simple timestamp without needing heavy external date libraries
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}s epoch", duration.as_secs(), duration.subsec_millis())
}
