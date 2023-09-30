use chrono::DateTime;
use chrono::FixedOffset;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

pub const DEFAULT_MMSI: &str = "00000";
pub const DEFAULT_URL: &str = "https://erddap.example.com/erddap/tabledap/data_set";
pub const DEFAULT_KEY: &str = "username_password";

#[derive(Deserialize, Debug)]
/// Data about the AIS receiver
pub struct AisCatcherReceiver {
    /// The description from AIS-catcher
    pub description: String,
    #[allow(dead_code)]
    /// Version of AIS-catcher
    pub version: u32,
    #[allow(dead_code)]
    /// Engine name from AIS-catcher
    pub engine: String,
    #[allow(dead_code)]
    /// ???
    pub setting: String,
}

#[derive(Deserialize, Debug)]
pub struct AisCatcherDevice {
    #[allow(dead_code)]
    pub product: String,
    #[allow(dead_code)]
    pub vendor: String,
    #[allow(dead_code)]
    pub serial: String,
    #[allow(dead_code)]
    pub setting: String,
}

#[derive(Deserialize, Debug)]
/// Messages received by AIS-catcher and decoded to JSON
pub struct AisMessage {
    #[serde(flatten)]
    /// The key-value map of the message
    pub msg: HashMap<String, serde_json::Value>,
}
#[derive(Deserialize, Debug)]
pub struct AisCatcherMessage {
    #[allow(dead_code)]
    pub protocol: String,
    #[allow(dead_code)]
    pub encodetime: String,
    // This is the name that AIS-catcher identifies itself with, not the station ID
    // of the broadcast source (for weather, only MMSI is present)
    pub stationid: String,
    // Details about the AIS-catcher receiver itself
    pub receiver: AisCatcherReceiver,
    #[allow(dead_code)]
    pub device: AisCatcherDevice,
    pub msgs: Vec<AisMessage>,
}

#[derive(Debug, Default)]
pub struct AisStationData {
    pub latitude: f64,
    pub longitude: f64,
    pub mmsi: u64,
    pub signal_power: f64,
    pub rxtime: DateTime<FixedOffset>,
}

impl From<&AisMessage> for AisStationData {
    fn from(f: &AisMessage) -> Self {
        // Deal with the fact that the string rxtime is not in any known format for auto
        // conversion.
        let chrono_ref =
            NaiveDateTime::parse_from_str(f.msg["rxtime"].as_str().unwrap(), "%Y%m%d%H%M%S")
                .unwrap();
        let tz_offset = FixedOffset::west_opt(0).unwrap();
        let dt_ref: DateTime<FixedOffset> = tz_offset.from_local_datetime(&chrono_ref).unwrap();
        AisStationData {
            latitude: f.msg["lat"].as_f64().unwrap(),
            longitude: f.msg["lon"].as_f64().unwrap(),
            mmsi: f.msg["mmsi"].as_u64().unwrap(),
            signal_power: f.msg["signalpower"].as_f64().unwrap(),
            rxtime: dt_ref,
        }
    }
}

impl AisStationData {
    pub fn as_query_arguments(&self, mmsi_lookup: &HashMap<String, String>) -> Vec<(&str, String)> {
        let unknown = "UNKNOWN".to_string();
        let station_id = match mmsi_lookup.get(&self.mmsi.to_string()) {
            Some(val) => val,
            _ => &unknown,
        };

        let qa = vec![
            // ERDDAP expects these keys as lower case
            ("latitude", format!("{:.3}", self.latitude)),
            ("longitude", format!("{:.3}", self.longitude)),
            ("time", self.rxtime.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            // ERDDAP expects these keys as upper cased
            ("Signal_Power", format!("{:.3}", self.signal_power)),
            ("Station_ID", station_id.to_string()),
            ("MMSI", self.mmsi.to_string()),
        ];
        return qa;
    }
}

#[derive(Debug, Default)]
pub struct AisWeatherData {
    pub wind_speed: u64,
    pub wind_gust_speed: u64,
    pub wind_direction: u64,
    pub wind_gust_direction: u64,
    pub wave_height: f64,
    pub wave_period: u64,
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
    pub fn as_query_arguments(&self) -> Vec<(&str, String)> {
        let qa = vec![
            // ERDDAP expects these keys as upper cased snake
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

/// Application configuration from file
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub erddap_url: String,
    pub erddap_key: String,
    pub ignore_mmsi: Vec<u64>,
    /// Map MMSIs to string names
    pub mmsi_lookup: Vec<MMSILookup>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MMSILookup {
    pub mmsi: String,
    pub station_id: String,
}

impl ::std::default::Default for AppConfig {
    fn default() -> Self {
        Self {
            erddap_url: DEFAULT_URL.to_string(),
            erddap_key: DEFAULT_KEY.to_string(),
            ignore_mmsi: vec![],
            mmsi_lookup: vec![MMSILookup {
                mmsi: DEFAULT_MMSI.to_string(),
                station_id: "MMSI Name".to_string(),
            }],
        }
    }
}

/// CLI state data for Axum to pass around; everything from args has to be in here
#[derive(Clone)]
pub struct ArgsState {
    pub url: String,
    pub author_key: String,
    pub dump_all_packets: bool,
    pub mmsi_lookup: HashMap<String, String>,
    pub ignore_mmsi: Vec<u64>,
}
