use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use clap::Parser;
use erddap_feeder::{AisCatcherMessage, AisMessageIdentifier, AisStationData, AisType8Dac200Fid31};
use erddap_feeder::{AppConfig, ArgsState, PerMessageConfig};
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

#[derive(Parser, Debug)]
#[command(author = "Duncan Hill")]
#[command(version = "0.1")]
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

    /// Alternate configuration file to load
    #[arg(short, long)]
    config_file: Option<String>,

    /// Dump every received JSON packet (a packet can contain several messages)
    #[arg(long, default_value_t = false)]
    dump_all_packets: bool,

    /// Dump every accepted message's raw structure
    #[arg(long, default_value_t = false)]
    dump_accepted_messages: bool,

    /// A user manual of sorts
    #[arg(short, long, default_value_t = false)]
    user_manual: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if args.user_manual {
        user_manual();
        std::process::exit(0);
    }
    tracing_subscriber::fmt::init();
    let app_config = load_config(args.config_file);
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

/// Load a configuration file from the OS config dir location. If no config is present,
/// write a default configuration
fn load_config(config_file: Option<String>) -> AppConfig {
    let filename = match config_file {
        None => "default-config",
        Some(ref v) => v.as_str(),
    };
    // Knowing the file name is useful for the rest of the error messages.
    let cfg_file = match confy::get_configuration_file_path("erddap-feeder", filename) {
        Ok(buf) => buf,
        Err(error) => {
            tracing::error!("Could not get configuration file name: {}", error);
            std::process::exit(Exits::CouldNotGetConfigFilePath as i32);
        }
    };
    let cfg_file_name = cfg_file.as_os_str().to_str().unwrap();

    // Attempt loading the configuration file; it can not exist, and confy will not
    // consider that to be an error.
    let cfg: AppConfig = match confy::load(APP_NAME, filename) {
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
        create_config(cfg_file_name);
        std::process::exit(Exits::EmptyMmsiLookup as i32);
    } else {
        // The vector of mmsi lookups was not empty, but is the default present? If so,
        // the user needs to edit the file and set up the lookup properly.
        for lookup in &cfg.mmsi_lookup {
            if lookup.mmsi == DEFAULT_MMSI {
                tracing::error!(
                    "The configuration file {} has the default MMSI lookup. Please edit the file.",
                    cfg_file.as_os_str().to_str().unwrap()
                );
                std::process::exit(Exits::DefaultMmsiLookup as i32);
            }
        }
        if cfg.erddap_url == DEFAULT_URL {
            tracing::error!(
                "The configuration file {} has the default ERDDAP URL. Please edit the file.",
                cfg_file.as_os_str().to_str().unwrap()
            );
            std::process::exit(Exits::DefaultErddapUrl as i32);
        }
        if cfg.erddap_key == DEFAULT_KEY {
            tracing::error!(
                "The configuration file {} has the default ERDDAP key. Please edit the file.",
                cfg_file.as_os_str().to_str().unwrap()
            );
            std::process::exit(Exits::DefaultErddapKey as i32);
        }
    }

    cfg
}

/// Write a default configuration file out, and ask the user to edit it.
fn create_config(cfg_file_name: &str) {
    let basic_config = AppConfig::default();
    match confy::store("erddap-feeder", None, basic_config) {
        Ok(_) => tracing::info!("Wrote initial configuration file. Please edit it and adjust the [[mmsi_lookup]] entries."),
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
            tracing::info!("{:?}", asd);
            let awd = AisType8Dac200Fid31::from(&msg);
            tracing::info!("{:?}", awd);
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
    tracing::info!("{}", logmsg);
    (StatusCode::OK, Json(json!({"message": logmsg })))
}

async fn send_to_erddap(
    station: AisStationData,
    weather: AisType8Dac200Fid31,
    args: State<ArgsState>,
) {
    // FIXME From here to right before let client should be a function that returns the
    // query vector after processing it to remove fields, and rename fields.

    // Get the component vectors of kwargs
    let asd_query = station.as_query_arguments(&args.mmsi_lookup);
    let mut weather_query = weather.as_query_arguments();
    let author = vec![("author", args.author_key.to_string())];
    // Apply the filters specified in the TOML config. If the vector is empty, nothing is removed,
    // to avoid having to list ALL the fields.
    weather_query.retain(|&(key, _)| args.publish_fields.iter().any(|s| s == key));

    // Rename the keys for the HTTP request based on the configuration in the TOML file
    let result: Vec<(&str, String)> = weather_query
        .into_iter()
        .map(|(old_name, value)| {
            let new_name = args
                .rename_fields
                .get(old_name)
                .map(|s| s.as_str())
                .unwrap_or(&old_name);
            (new_name, value)
        })
        .collect();

    // Build the arg string
    let mut query_args = vec![];
    query_args.extend(asd_query);
    query_args.extend(result);
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

fn user_manual() {
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
        have all the fields configured, you'll want to use the `publish_fields` option for a [[message_config]] to list all of
        the fields that you want to send. If the list is empty, all fields are published.

        You cannot filter the required `time` field, or the `mmsi` field.
"
    }
}
