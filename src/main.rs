use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use clap::Parser;
use confy;
use erddap_feeder::AppConfig;
use erddap_feeder::ArgsState;
use erddap_feeder::{AisCatcherMessage, AisStationData, AisWeatherData};
use erddap_feeder::{DEFAULT_KEY, DEFAULT_MMSI, DEFAULT_URL};
use reqwest;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;

const APP_NAME: &str = "erddap-feeder";
enum EXITS {
    CouldNotLoadConfigFile = 1,
    CouldNotCreateConfigFile = 2,
    CouldNotGetConfigFilePath = 3,
    EmptyMmsiLookup = 4,
    DefaultMmsiLookup = 5,
    DefaultErddapUrl = 6,
    DefaultErddapKey = 7,
}

#[derive(Parser, Debug)]
#[command(author = "Duncan Hill")]
#[command(version = "0.1")]
// #[command(about = "Processes JSON AIS weather data, sends to ERDDAP")]

/// Processes JSON AIS weather data emitted from AIS-catcher in HTTP mode, then
/// sends the processed data as a HTTP GET request to an ERDDAP server. The
/// ERDDAP server must be operating in a TLS-secured manner for the GET to be
/// accepted as a data write.
///
/// By default, this tool listens on the wildcard IPv4 address - you can control
/// that with --bind-address.
///
/// To see all packets in their raw form, use --dump-all-packets
struct Args {
    /// IP address and socket to listen on.
    #[arg(long, default_value_t = SocketAddr::from(([0,0,0,0], 22022)))]
    bind_address: SocketAddr,

    /// Dump every received JSON packet
    #[arg(short, long)]
    dump_all_packets: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let app_config = load_config();
    tracing::info!("ERDDAP URL: {}", app_config.erddap_url);
    // Convert the config.mmsi_lookup vector of objects to a map of name to station id
    // This enables the station data as_query_arguments function to map the MMSI in the
    // input to a station name without hardcoding.
    let mmsi_to_station_id_map = build_mmsi_to_station_id_map(&app_config);

    // Axum/tokio can pass a state object to every handler that's invoked. Here, it's
    // used to pass the configuration of the program to every handler (and it must come
    // after the route).
    let args_state = ArgsState {
        url: app_config.erddap_url.into(),
        author_key: app_config.erddap_key.into(),
        dump_all_packets: args.dump_all_packets.into(),
        mmsi_lookup: mmsi_to_station_id_map.into(),
    };


    // Start a router for the POST requests that AIS-catcher sends.
    let app = Router::new()
        .route("/aiscatcher", post(process_ais_message))
        .with_state(args_state);

    // let addr = SocketAddr::from((args.bind_address, args.bind_port));
    tracing::info!("Listening on {}", args.bind_address);

    // Let's go!
    axum::Server::bind(&args.bind_address)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

/// Convert the TOMLified table of mmsi to name into a map for rapid lookups.
fn build_mmsi_to_station_id_map(app_config: &AppConfig) -> HashMap<String, String> {
    let mut mmsi_to_station_id_map = HashMap::new();

    for entry in &app_config.mmsi_lookup {
        mmsi_to_station_id_map.insert(entry.mmsi.clone(), entry.station_id.clone());
        tracing::info!("Mapped MMSI '{}' to '{}'", entry.mmsi, entry.station_id);
    }

    mmsi_to_station_id_map
}

/// Load a configuration file from the OS config dir location. If no config is present,
/// write a default configuration
fn load_config() -> AppConfig {
    // Knowing the file name is useful for the rest of the error messages.
    let cfg_file = match confy::get_configuration_file_path("erddap-feeder", None) {
        Ok(buf) => buf,
        Err(error) => {
            tracing::error!("Could not get configuration file name: {}", error);
            std::process::exit(EXITS::CouldNotGetConfigFilePath as i32);
        }
    };
    let cfg_file_name = cfg_file.as_os_str().to_str().unwrap();

    // Attempt loading the configuration file; it can not exist, and confy will not
    // consider that to be an error.
    let cfg: AppConfig = match confy::load(APP_NAME, None) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(
                "Could not load configuration file {}: {}",
                cfg_file_name,
                error
            );
            std::process::exit(EXITS::CouldNotLoadConfigFile as i32);
        }
    };

    // Empty vector means triggering the creation of a default configuration file.
    if cfg.mmsi_lookup.is_empty() {
        tracing::error!(
            "The configuration file {} does not have any MMSI lookups defined.",
            cfg_file_name
        );
        create_config(&cfg_file_name);
        std::process::exit(EXITS::EmptyMmsiLookup as i32);
    } else {
        // The vector of mmsi lookups was not empty, but is the default present? If so,
        // the user needs to edit the file and set up the lookup properly.
        for lookup in &cfg.mmsi_lookup {
            if lookup.mmsi == DEFAULT_MMSI {
                tracing::error!(
                    "The configuration file {} has the default MMSI lookup. Please edit the file.",
                    cfg_file.as_os_str().to_str().unwrap()
                );
                std::process::exit(EXITS::DefaultMmsiLookup as i32);
            }
        }
        if cfg.erddap_url == DEFAULT_URL {
            tracing::error!(
                "The configuration file {} has the default ERDDAP URL. Please edit the file.",
                cfg_file.as_os_str().to_str().unwrap()
            );
            std::process::exit(EXITS::DefaultErddapUrl as i32);
        }
        if cfg.erddap_key == DEFAULT_KEY {
            tracing::error!(
                "The configuration file {} has the default ERDDAP key. Please edit the file.",
                cfg_file.as_os_str().to_str().unwrap()
            );
            std::process::exit(EXITS::DefaultErddapKey as i32);
        }
    }

    cfg
}

/// Write a default configuration file out, and ask the user to edit it.
fn create_config(cfg_file_name: &str) {
    let basic_config = AppConfig::default();
    let _ = match confy::store("erddap-feeder", None, basic_config) {
        Ok(_) => tracing::info!("Wrote initial configuration file. Please edit it and adjust the [[mmsi_lookup]] entries."),
        Err(error) => {
            tracing::error!("Could not create configuration file {}: {}", cfg_file_name, error);
            std::process::exit(EXITS::CouldNotCreateConfigFile as i32);
        }
    };
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
    let asd_query = station.as_query_arguments(&args.mmsi_lookup);
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
