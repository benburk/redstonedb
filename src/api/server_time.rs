use axum::Json;
use serde::Serialize;

use app::util::current_time_micros;

#[derive(Serialize)]
pub struct TimeResponse {
    timestamp: i64,
}

pub async fn get() -> Json<TimeResponse> {
    Json(TimeResponse {
        timestamp: current_time_micros(),
    })
}
