use axum::{http::StatusCode, response::IntoResponse, routing::post, Json, Router};

use reqwest;
use serde_json::json;
use std::net::SocketAddr;
use erddap_feeder::{AisCatcherMessage, AisStationData, AisWeatherData};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let app = Router::new().route("/aiscatcher", post(process_ais_message));
    let addr = SocketAddr::from(([0, 0, 0, 0], 7878));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn process_ais_message(Json(payload): Json<AisCatcherMessage>) -> impl IntoResponse {
    // tracing::info!("{:?}", payload);
    tracing::info!(
        "Processing packet from {} running {}",
        payload.stationid,
        payload.receiver.description
    );

    // let mut ais_core = AisCoreData::default();
    for msg in payload.msgs {
        let msg_type = msg.msg["type"].as_u64().unwrap();
        match msg_type {
            8 => {
                let asd = AisStationData::from(&msg);
                tracing::info!("{:?}", asd);
                let awd = AisWeatherData::from(&msg);
                tracing::info!("{:?}", awd);
                send_to_erddap(asd, awd).await;
            }
            _ => {
                ();
            }
        }
    }

    (StatusCode::CREATED, Json(json!({"message": "Processed" })))
}

async fn send_to_erddap(station: AisStationData, weather: AisWeatherData) {
    // Build string from component bits
    let asd_query = station.as_query_arguments();
    let weather_query = weather.as_query_arguments();
    let author = vec![("author", "test_Quantum15".to_string())];
    let mut query_args = vec![];
    query_args.extend(asd_query);
    query_args.extend(weather_query);
    query_args.extend(author);
    let client = reqwest::Client::new();
    let body = client
        .get("https://erddap.home.arpa/erddap/tabledap/dublin_bay_ais_weather_data.insert")
        .query(&query_args);
    let result = body.send().await.unwrap();
    match result.status() {
        StatusCode::OK => {
            tracing::info!("Successful submission");
        }
        _ => {
            tracing::error!("{:?}", result);
        }
    }
}
