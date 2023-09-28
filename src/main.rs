use axum::{http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use chrono::NaiveDateTime;
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::TimeZone;

use reqwest;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Deserialize, Debug)]
struct AisCatcherReceiver {
    description: String,
    #[allow(dead_code)]
    version: u32,
    #[allow(dead_code)]
    engine: String,
    #[allow(dead_code)]
    setting: String,
}

#[derive(Deserialize, Debug)]
struct AisCatcherDevice {
    #[allow(dead_code)]
    product: String,
    #[allow(dead_code)]
    vendor: String,
    #[allow(dead_code)]
    serial: String,
    #[allow(dead_code)]
    setting: String,
}

#[derive(Deserialize, Debug)]
struct AisMessage {
    #[serde(flatten)]
    msg: HashMap<String, serde_json::Value>,
}
#[derive(Deserialize, Debug)]
struct AisCatcherMessage {
    #[allow(dead_code)]
    protocol: String,
    #[allow(dead_code)]
    encodetime: String,
    stationid: String,
    receiver: AisCatcherReceiver,
    #[allow(dead_code)]
    device: AisCatcherDevice,
    msgs: Vec<AisMessage>,
}

#[derive(Debug, Default)]
struct AisStationData {
    latitude: f64,
    longitude: f64,
    mmsi: u64,
    signal_power: f64,
    rxtime: String,
}

impl From<&AisMessage> for AisStationData {
    fn from(f: &AisMessage) -> Self {
        AisStationData {
            latitude: f.msg["lat"].as_f64().unwrap(),
            longitude: f.msg["lon"].as_f64().unwrap(),
            mmsi: f.msg["mmsi"].as_u64().unwrap(),
            signal_power: f.msg["signalpower"].as_f64().unwrap(),
            // Ewww?
            rxtime: f.msg["rxtime"].as_str().unwrap().to_string(),
        }
    }
}

impl AisStationData {
    fn as_query_arguments(&self) -> Vec<(&str, String)> {
        let chrono_ref = NaiveDateTime::parse_from_str(self.rxtime.as_str(), "%Y%m%d%H%M%S").unwrap();
        let tz_offset = FixedOffset::west_opt(0).unwrap();
        let dt_ref: DateTime<FixedOffset> = tz_offset.from_local_datetime(&chrono_ref).unwrap();
        let station_id = match self.mmsi.to_string().as_str() {
            "992509976" =>  "TEST_CIL_ATON",
            "992501301" => "Dublin_Bay_Buoy",
            "992501017" => "Kish_Lighthouse",
            _ =>  "UNKNOWN"
        };
        let qa = vec![
            // ERDDAP expects these keys as lower case
            ("latitude", format!("{:.3}", self.latitude)),
            ("longitude", format!("{:.3}", self.longitude)),
            ("time", dt_ref.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            // ERDDAP expects this key as upper case
            ("Signal_Power", format!("{:.3}", self.signal_power)),
            ("Station_ID", station_id.to_string()),
            ("MMSI", self.mmsi.to_string()),
        ];
        return qa;
    }
}

#[derive(Debug, Default)]
struct AisWeatherData {
    wind_speed: u64,
    wind_gust_speed: u64,
    wind_direction: u64,
    wind_gust_direction: u64,
    wave_height: f64,
    wave_period: u64,
    // air_pressure: u64,
}

impl From<&AisMessage> for AisWeatherData {
    fn from(f: &AisMessage) -> Self {
        AisWeatherData {
            wind_speed: f.msg["wspeed"].as_u64().unwrap(),
            wind_gust_speed: f.msg["wgust"].as_u64().unwrap(),
            wind_direction: f.msg["wdir"].as_u64().unwrap(),
            wind_gust_direction: f.msg["wgustdir"].as_u64().unwrap(),
            wave_height: f.msg["waveheight"].as_f64().unwrap(),
            wave_period: f.msg["waveperiod"].as_u64().unwrap(),
            // air_pressure: f.msg["pressure"].as_u64().unwrap(),
        }
    }
}

impl AisWeatherData {
    fn as_query_arguments(&self) -> Vec<(&str, String)> {
        let qa = vec![
            // ERDDAP expects these keys as lower case
            ("Wind_Speed", self.wind_speed.to_string()),
            ("Wind_Gust_Speed", self.wind_gust_speed.to_string()),
            ("Wind_Direction", self.wind_direction.to_string()),
            ("Wind_Gust_Direction", self.wind_gust_direction.to_string()),
            ("Wave_Height", self.wave_height.to_string()),
            ("Wave_Period", self.wave_period.to_string()),
            // ("Air_Pressure", self.air_pressure.to_string()),
        ];
        return qa;
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let app = Router::new().route("/aiscatcher", post(process_ais_message));

    let addr = SocketAddr::from(([0, 0, 0, 0], 7878));
    tracing::info!("listening on {}", addr);
    // Enum with From impls is probably better
    // But then mapping enum to strings?
    let mut my_map = HashMap::new();
    my_map.insert(1, "Position Report Class A");
    my_map.insert(4, "Base Station Report");
    my_map.insert(5, "Static and Voyage Related Data");
    my_map.insert(8, "Binary Broadcast");
    my_map.insert(20, "Data Link Management");
    my_map.insert(21, "Aid-to-navigation Report");

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
    let body = client.get("https://erddap.home.arpa/erddap/tabledap/dublin_bay_ais_weather_data.insert").query(&query_args);
    let result = body.send().await.unwrap();
    match result.status() {
        StatusCode::OK => {tracing::info!("Successful submission");}
        _ => {tracing::error!("{:?}", result);}
    }
}
