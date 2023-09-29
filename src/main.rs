use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use clap::Parser;
use erddap_feeder::ArgsState;
use erddap_feeder::{AisCatcherMessage, AisStationData, AisWeatherData};
use reqwest;
use serde_json::json;
use std::net::SocketAddr;

#[derive(Parser, Debug)]
#[command(author = "Duncan Hill")]
#[command(version = "0.1")]
#[command(about = "Processes JSON AIS weather data, sends to ERDDAP")]
#[command(long_about = None)]
struct Args {
    /// ERDDAP URL, in the form https://server.name/erddap/tabledap/table_name
    #[arg(short, long)]
    url: String,

    /// Author key issued by the ERDDAP administrator
    #[arg(short, long)]
    author_key: String,

    /// What port to listen on for packets from AIS Catcher
    #[arg(short, long)]
    listen_port: u16,

    /// Dump every received JSON packet
    #[arg(short, long)]
    dump_all_packets: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let args_state = ArgsState {
        url: args.url.into(),
        author_key: args.author_key.into(),
        dump_all_packets: args.dump_all_packets.into(),
    };
    let app = Router::new()
        .route("/aiscatcher", post(process_ais_message))
        .with_state(args_state);
    let addr = SocketAddr::from(([0, 0, 0, 0], args.listen_port));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn process_ais_message(
    State(args): State<ArgsState>,
    Json(payload): Json<AisCatcherMessage>,
) -> impl IntoResponse {
    if args.dump_all_packets {
        tracing::info!("{:?}", payload);
    }
    tracing::info!(
        "Processing packet from {} running {}",
        payload.stationid,
        payload.receiver.description
    );
    let mut count = 0;
    for msg in payload.msgs {
        let msg_type = msg.msg["type"].as_u64().unwrap();
        match msg_type {
            // Only want binary broadcast types
            8 => {
                let fid = msg.msg["fid"].as_u64().unwrap();
                match fid {
                    // And then the subset that is IMO289 weather
                    31 => {
                        let asd = AisStationData::from(&msg);
                        tracing::info!("{:?}", asd);
                        let awd = AisWeatherData::from(&msg);
                        tracing::info!("{:?}", awd);
                        send_to_erddap(asd, awd, axum::extract::State(args.clone())).await;
                        count += 1;
                    }
                    _ => {
                        tracing::debug!("Not a weather packet? {:?}", msg);
                    }
                }
            }
            _ => {
                ();
            }
        }
    }

    (
        StatusCode::OK,
        Json(json!({"message": format!("Processed {} messages", count) })),
    )
}

async fn send_to_erddap(station: AisStationData, weather: AisWeatherData, args: State<ArgsState>) {
    // Get the component vectors of kwargs
    let asd_query = station.as_query_arguments();
    let weather_query = weather.as_query_arguments();
    let author = vec![("author", args.author_key.to_string())];
    // Build the arg string
    let mut query_args = vec![];
    query_args.extend(asd_query);
    query_args.extend(weather_query);
    query_args.extend(author);
    // Off to ERDDAP we go
    let client = reqwest::Client::new();
    let body = client
        .get(format!("{}.insert", args.url))
        .query(&query_args);
    let response = body.send().await;
    // Errors can happen
    if let Err(e) = response {
        if e.is_request() {
            tracing::error!("Request failed: {:?}", e);
        }
    } else {
        let result = response.unwrap();
        match result.status() {
            StatusCode::OK => {
                tracing::info!("Successful submission");
            }
            StatusCode::NOT_FOUND => {
                tracing::error!("URL not found. Please check hostname, and path, of the --url.");
            }
            _ => {
                tracing::error!("{:?}", result);
            }
        }
    }
}
