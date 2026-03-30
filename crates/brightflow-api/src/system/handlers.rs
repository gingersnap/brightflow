use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::time;

use crate::state::AppState;

/// WebSocket upgrade handler for /api/system/ws
pub async fn system_ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_system_socket(socket, state))
}

async fn handle_system_socket(socket: WebSocket, state: AppState) {
    tracing::info!("System WebSocket client connected");
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to log broadcast channel
    let mut log_rx = state.log_sender.subscribe();

    // Metrics ticker — every 2 seconds
    let mut metrics_interval = time::interval(Duration::from_secs(2));

    // Helper to build metrics JSON
    let build_metrics_msg = |snapshot: &crate::system::sampler::SystemSnapshot| {
        json!({
            "type": "systemMetrics",
            "processRssBytes": snapshot.process_rss_bytes,
            "processAnonBytes": snapshot.process_anon_bytes,
            "systemUsedBytes": snapshot.system_used_bytes,
            "systemTotalBytes": snapshot.system_total_bytes,
            "cpuPercent": snapshot.cpu_percent,
            "uptimeSecs": snapshot.uptime_secs,
        })
    };

    // Send initial metrics snapshot immediately
    {
        let snapshot = state.system_metrics.read().await;
        let msg = build_metrics_msg(&snapshot);
        if sender
            .send(Message::Text(msg.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }

    loop {
        tokio::select! {
            _ = metrics_interval.tick() => {
                let snapshot = state.system_metrics.read().await;
                let msg = build_metrics_msg(&snapshot);
                if sender.send(Message::Text(msg.to_string().into())).await.is_err() {
                    break;
                }
            }
            result = log_rx.recv() => {
                match result {
                    Ok(entry) => {
                        let msg = json!({
                            "type": "logEntry",
                            "timestamp": entry.timestamp,
                            "level": entry.level,
                            "target": entry.target,
                            "message": entry.message,
                        });
                        if sender.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("System WS log subscriber lagged by {n} messages");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Drain incoming messages (handle close frames)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    tracing::info!("System WebSocket client disconnected");
}
