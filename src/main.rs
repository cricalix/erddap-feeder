use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use clap::{Args, Parser, Subcommand};
use erddap_feeder::{AisCatcherMessage, AisMessageIdentifier, AisStationData, AisType8Dac200Fid31};
use erddap_feeder::{AppConfig, ArgsState, ErddapResponse, PerMessageConfig};
use erddap_feeder::{DEFAULT_KEY, DEFAULT_MMSI, DEFAULT_URL};
use indoc::printdoc;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;

const APP_NAME: &str = "erddap-feeder";
enum Exits {
    CouldNotLoadConfigFile = 1,
    CouldNotCreateConfigFile = 2,
    CouldNotGetConfigFilePath = 3,
    EmptyMmsiLookup = 4,
    DefaultMmsiLookup = 5,
    DefaultErddapUrl = 6,
    DefaultErddapKey = 7,
}

/// Processes JSON AIS weather data emitted from AIS-catcher in HTTP mode, then
/// sends the processed data as a HTTP GET request to an ERDDAP server. The
/// ERDDAP server must be operating in a TLS-secured manner for the GET to be
/// accepted as a data write.
#[derive(Parser)]
#[command(author = clap::crate_authors!())]
#[command(version = clap::crate_version!())]
#[command(propagate_version = false)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Initialize(Initialize),
    Manual(Manual),
    Run(Run),
}

/// Initialize a configuration file; overwrites any existing configuration of the same name.
/// The confy crate is used to determine the platform-appropriate directory to store the
/// configuration file, and it will automatically add `.toml`.
#[derive(Args)]
struct Initialize {
    /// Alternate configuration file to create; just the base name without .toml
    #[arg(short, long, default_value_t = String::from("default-config"))]
    config_file: String,
}

/// Show a user manual of sorts
#[derive(Args)]
struct Manual {}

/// Run the HTTP listener to accept JSON packets and send them to ERDDAP
#[derive(Args)]
struct Run {
    /// IP address and socket to listen on.
    #[arg(long, default_value_t = SocketAddr::from(([0,0,0,0], 22022)))]
    bind_address: SocketAddr,

    /// Alternate configuration file to load
    #[arg(short, long, default_value_t = String::from("default-config"))]
    config_file: String,

    /// Dump every received JSON packet (a packet can contain several messages)
    #[arg(long, default_value_t = false)]
    dump_all_packets: bool,

    /// Dump every accepted message's raw structure
    #[arg(long, default_value_t = false)]
    dump_accepted_messages: bool,
}

/// Dispatch the subcommands
#[tokio::main]
async fn main() {
    let args = Cli::parse();
    tracing_subscriber::fmt::init();
    match &args.command {
        Commands::Initialize(init) => {
            exec_init(init).await;
        }
        Commands::Manual(_) => {
            exec_user_manual();
        }
        Commands::Run(run) => {
            exec_run(run).await;
        }
    }
}

/// Initialize the configuration file that the feeder will use.
async fn exec_init(args: &Initialize) {
    create_config(&args.config_file);
}

/// Process the configuration file into a state object, then start the webserver,
/// listen for requests, process them, and send the resulting data to ERDDAP.
async fn exec_run(args: &Run) {
    // load config
    let app_config = load_config(&args.config_file);
    tracing::info!("ERDDAP URL: {}", app_config.erddap_url);
    // Convert the config.mmsi_lookup vector of objects to a map of name to station id
    // This enables the station data as_query_arguments function to map the MMSI in the
    // input to a station name without hardcoding.
    let mmsi_to_station_id_map = build_mmsi_to_station_id_map(&app_config);

    // Convert the config.message_config vector into a map of AIS message identifier to
    // ignored MMSIs for that message type.
    let message_config = build_message_config_lookup(&app_config);

    // Convert the config.rename_fields vector of string tuples to a looup map.
    let rename_fields_map = build_field_rename_map(&app_config);

    // Axum/tokio can pass a state object to every handler that's invoked. Here, it's
    // used to pass the configuration of the program to every handler (and it must come
    // after the route).
    let args_state = ArgsState {
        url: app_config.erddap_url,
        author_key: app_config.erddap_key,
        publish_fields: app_config.publish_fields,
        rename_fields: rename_fields_map,
        dump_all_packets: args.dump_all_packets,
        dump_accepted_messages: args.dump_accepted_messages,
        mmsi_lookup: mmsi_to_station_id_map,
        message_config_lookup: message_config,
    };

    // Start a router for the POST requests that AIS-catcher sends.
    let app = Router::new()
        .route("/aiscatcher", post(process_aiscatcher_submission))
        .with_state(args_state);

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
        tracing::info!(
            "MMSI lookups - mapped MMSI '{}' to '{}'",
            entry.mmsi,
            entry.station_name
        );
        mmsi_to_station_id_map.insert(entry.mmsi.clone(), entry.station_name.clone());
    }

    mmsi_to_station_id_map
}

fn build_field_rename_map(app_config: &AppConfig) -> HashMap<String, String> {
    let mut renames: HashMap<String, String> = HashMap::new();

    for (source, target) in &app_config.rename_fields {
        tracing::info!("Field renames - mapped source '{}' to '{}'", source, target);
        renames.insert(source.clone(), target.clone());
    }

    renames
}

/// Convert the TOMLified table of accepted packets into something that can be matched
fn build_message_config_lookup(
    app_config: &AppConfig,
) -> HashMap<AisMessageIdentifier, PerMessageConfig> {
    let mut lookup = HashMap::new();
    for entry in &app_config.message_config {
        let ami = AisMessageIdentifier {
            r#type: entry.r#type,
            dac: entry.dac,
            fid: entry.fid,
        };
        let pmc = PerMessageConfig {
            ignore_mmsi: entry.ignore_mmsi.clone(),
        };
        tracing::info!("Ignore list - mapped {} to {:?}", ami, entry.ignore_mmsi);
        lookup.insert(ami, pmc);
    }
    lookup
}

/// Get the on-disk filename for a config file
fn get_config_path(config_file: &String) -> String {
    // Knowing the file name is useful for the rest of the error messages.
    let cfg_file = match confy::get_configuration_file_path("erddap-feeder", config_file.as_str()) {
        Ok(buf) => buf,
        Err(error) => {
            tracing::error!("Could not get configuration file name: {}", error);
            std::process::exit(Exits::CouldNotGetConfigFilePath as i32);
        }
    };
    cfg_file.into_os_string().into_string().unwrap()
}

/// Load a configuration file from the OS config dir location. If no config is present,
/// write a default configuration
fn load_config(config_file: &String) -> AppConfig {
    let cfg_file_name = get_config_path(&config_file);

    // Attempt loading the configuration file; it can not exist, and confy will not
    // consider that to be an error.
    let cfg: AppConfig = match confy::load(APP_NAME, config_file.as_str()) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(
                "Could not load configuration file {}: {}",
                cfg_file_name,
                error
            );
            std::process::exit(Exits::CouldNotLoadConfigFile as i32);
        }
    };

    // Empty vector means triggering the creation of a default configuration file.
    if cfg.mmsi_lookup.is_empty() {
        tracing::error!(
            "The configuration file {} does not have any MMSI lookups defined.",
            cfg_file_name
        );
        std::process::exit(Exits::EmptyMmsiLookup as i32);
    } else {
        // The vector of mmsi lookups was not empty, but is the default present? If so,
        // the user needs to edit the file and set up the lookup properly.
        for lookup in &cfg.mmsi_lookup {
            if lookup.mmsi == DEFAULT_MMSI {
                tracing::error!(
                    "The configuration file {} has the default MMSI lookup. Please edit the file.",
                    cfg_file_name
                );
                std::process::exit(Exits::DefaultMmsiLookup as i32);
            }
        }
        if cfg.erddap_url == DEFAULT_URL {
            tracing::error!(
                "The configuration file {} has the default ERDDAP URL. Please edit the file.",
                cfg_file_name
            );
            std::process::exit(Exits::DefaultErddapUrl as i32);
        }
        if cfg.erddap_key == DEFAULT_KEY {
            tracing::error!(
                "The configuration file {} has the default ERDDAP key. Please edit the file.",
                cfg_file_name
            );
            std::process::exit(Exits::DefaultErddapKey as i32);
        }
    }

    cfg
}

/// Write a default configuration file out, and ask the user to edit it.
fn create_config(config_file: &String) {
    let cfg_file_name = get_config_path(&config_file);
    let basic_config = AppConfig::default();
    match confy::store("erddap-feeder", config_file.as_str(), basic_config) {
        Ok(_) => tracing::info!("Wrote initial configuration file {}. Please edit it and adjust the [[mmsi_lookup]] entries.", cfg_file_name),
        Err(error) => {
            tracing::error!("Could not create configuration file {}: {}", cfg_file_name, error);
            std::process::exit(Exits::CouldNotCreateConfigFile as i32);
        }
    };
}

async fn process_aiscatcher_submission(
    State(args): State<ArgsState>,
    Json(payload): Json<AisCatcherMessage>,
) -> impl IntoResponse {
    if args.dump_all_packets {
        tracing::info!("{:?}", payload);
    }
    let mut processed_count = 0;
    let mut skipped_count = 0;
    let mut ignored_count = 0;
    let mut total_count = 0;
    for msg in payload.msgs {
        total_count += 1;
        let ami = AisMessageIdentifier::from(&msg);
        // Is the message identifier allowed by the TOML setup?
        if args.message_config_lookup.contains_key(&ami) {
            tracing::info!("Message meets acceptance criteria {}, converting it", ami);
            if args.dump_accepted_messages {
                tracing::debug!("{:?}", msg);
            }
            let asd = AisStationData::from(&msg);
            tracing::debug!("{:?}", asd);
            let awd = AisType8Dac200Fid31::from(&msg);
            tracing::debug!("{:?}", awd);
            if args.message_config_lookup[&ami]
                .ignore_mmsi
                .contains(&asd.mmsi)
            {
                tracing::debug!("Ignored message from {}", asd.mmsi);
                ignored_count += 1;
            } else {
                send_to_erddap(asd, awd, axum::extract::State(args.clone())).await;
            }
            processed_count += 1;
        } else {
            tracing::debug!("Ignoring message with identifier {}", ami);
            skipped_count += 1;
        }
    }
    let logmsg = format!(
        "Received {} messages, submitted {}, skipped {}, ignored {}",
        total_count, processed_count, skipped_count, ignored_count
    );
    tracing::debug!("{}", logmsg);
    (StatusCode::OK, Json(json!({"message": logmsg })))
}

fn build_and_filter_weather_data(
    weather: AisType8Dac200Fid31,
    args: &State<ArgsState>,
) -> Vec<(String, String)> {
    let mut weather_query = weather.as_query_arguments();
    // Apply the filters specified in the TOML config. If the vector is empty, nothing is removed,
    // to avoid having to list ALL the fields.
    weather_query.retain(|(key, _)| args.publish_fields.iter().any(|s| s == key));
    weather_query
}

fn rename_weather_keys(
    weather_query: Vec<(String, String)>,
    renames: &HashMap<String, String>,
) -> Vec<(String, String)> {
    // Rename the keys for the HTTP request based on the configuration in the TOML file
    let result: Vec<(String, String)> = weather_query
        .into_iter()
        .map(|(old_name, value)| {
            let new_name = renames
                .get(&old_name)
                .map(|s| s.as_str())
                .unwrap_or(&old_name);
            (new_name.to_string(), value)
        })
        .collect();
    result
}

fn build_query_args(
    station: AisStationData,
    weather: AisType8Dac200Fid31,
    args: &State<ArgsState>,
) -> Vec<(String, String)> {
    let station_query = station.as_query_arguments(&args.mmsi_lookup);
    let weather_query = build_and_filter_weather_data(weather, &args);
    let weather_query = rename_weather_keys(weather_query, &args.rename_fields);
    let author = vec![("author".to_string(), args.author_key.to_string())];

    // Build the arg string
    let mut query_args = vec![];
    query_args.extend(station_query);
    query_args.extend(weather_query);
    query_args.extend(author);
    let result_vector: Vec<(String, String)> = query_args
        .into_iter()
        .map(|(first, second)| (first.to_string(), second))
        .collect();

    result_vector
}
async fn send_to_erddap(
    station: AisStationData,
    weather: AisType8Dac200Fid31,
    args: State<ArgsState>,
) {
    let query_args = build_query_args(station, weather, &args);

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
                tracing::info!(
                    "ERDDAP said {}",
                    result.json::<ErddapResponse>().await.unwrap().status
                );
            }
            StatusCode::NOT_FOUND => {
                tracing::error!(
                    "URL not found. Please check hostname and path. It's also possible the requested URL \
                    has fields that the ERDDAP server is not configured to accept ({}).",
                    result.url()
                );
            }
            _ => {
                tracing::error!("{:?}", result);
            }
        }
    }
}

fn exec_user_manual() {
    printdoc! {
        "User Manual
        ############

        Bind Address
        ============
        The bind address, or listen address, is the IP address and port that this software should listen on for connections from
        AIS-catcher. The default is the IPv4 syntax for 'any valid interface', namely '0.0.0.0'. If you're in an IPv6 world, and
        want to listen to both IPv4 and IPv6 ports, use '--bind-address [::]:22022' (or any other port of your choice).

        TLS
        ===
        This program does not do TLS on the listen address. It probably shouldn't be exposed to the Internet either. If you need
        TLS support, use something like Caddy or nginx to provide a reverse proxy.

        Required fields
        ===============

        ERDDAP's HttpGet table format has some mandatory fields - time, timestamp, command, author.
        - time comes from AIS-catcher's rxtime data
        - timestamp is created by ERDDAP itself, and is not supplied by this program. Don't send it.
        - command is created by ERDDAP itself, and is not supplied by this program. Don't send it.
        - author is based on the `erddap_key` data stored in the configuration file

        Different configuration files
        =============================

        The default path is OS-dependent, with a default file name of default-config.toml. This program will create a file for you
        that you must then edit.

        Filtering fields
        ================

        This tool tries to send the entire IMO289 meteorological data set over to the ERDDAP service. If your service doesn't
        have all the fields configured, you'll want to use the `publish_fields` option to list all of
        the fields that you want to send. If the list is empty, all fields are published.

        You cannot filter the required `time` field, or the `mmsi` field.
"
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename_weather_keys() {
        //weather_query: Vec<(String, String)>,
        //args: &State<ArgsState>,
        let wq = vec![("renameable".to_string(), "value".to_string())];
        let mut renames = HashMap::new();
        renames.insert("renameable".to_string(), "renamed".to_string());
        let x = rename_weather_keys(wq, &renames);
        let expected = vec![("renamed".to_string(), "value".to_string())];
        assert_eq!(x, expected);
    }
}
