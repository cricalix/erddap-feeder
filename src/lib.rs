use chrono::DateTime;
use chrono::FixedOffset;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
pub const DEFAULT_MMSI: &str = "00000";
pub const DEFAULT_URL: &str = "https://erddap.example.com/erddap/tabledap/data_set";
pub const DEFAULT_KEY: &str = "username_password";

#[derive(Deserialize, Debug)]
/// Data about the AIS receiver software
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

/// Data about the AIS receiver device;
// Nothing in here is used by this program, but the struct is needed to decode
// the input, and might be useful some day.
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
    /// This is the name that AIS-catcher identifies itself with, not the station ID
    /// of the broadcast source (for weather, only MMSI is present)
    pub stationid: String,
    /// Details about the AIS-catcher receiver itself
    pub receiver: AisCatcherReceiver,
    #[allow(dead_code)]
    /// Details about the hardware used by AIS-catcher
    pub device: AisCatcherDevice,
    pub msgs: Vec<AisMessage>,
}

#[derive(Debug, Default)]
pub struct AisStationData {
    /// The Mobile Marine Service Identifier - 9 digits. ATON will start 99.
    pub mmsi: u64,
    /// The signal power reported by AIS-catcher - how strong the signal from the station is
    pub signal_power: f64,
    /// The received time of the message, set by AIS-catcher based on the local clock
    /// Time is UTC/Zulu.
    pub rxtime: DateTime<FixedOffset>,
}

/// Extracts fields from the AisMessage structure, and produces an AisStationData structure
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
            mmsi: f.msg["mmsi"].as_u64().unwrap(),
            signal_power: f.msg["signalpower"].as_f64().unwrap(),
            rxtime: dt_ref,
        }
    }
}

/// Converts an AisStationData into a set of key/value pairs that line up with what the ERDDAP
/// system is configured to store.
impl AisStationData {
    pub fn as_query_arguments(&self, mmsi_lookup: &HashMap<String, String>) -> Vec<(&str, String)> {
        let unknown = "UNKNOWN".to_string();
        let station_id = match mmsi_lookup.get(&self.mmsi.to_string()) {
            Some(val) => val,
            _ => &unknown,
        };

        vec![
            // ERDDAP expects these keys as lower case
            ("time", self.rxtime.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            // ERDDAP expects these keys as upper cased
            ("station_name", station_id.to_string()),
            ("mmsi", self.mmsi.to_string()),
        ]
    }
}

/// Structure to hold the data from an IMO289 weather packet, Type 8 DAC 200 FID 31.
/// AIS Catcher provides scaled data.
#[derive(Debug, Default)]
pub struct AisType8Dac200Fid31 {
    /// Longitude, east is positive, west is negative. Minutes * 0.001. 181.000 = N/A
    pub lon: f64,
    /// Latitude, north is positive, south is negative. Minutes * 0.001. 91.000 = N/A
    pub lat: f64,
    /// Wind speed in knots. 126 = wind >= 126 knots, 127 = N/A
    pub wspeed: u64,
    /// Wind gust speed in knots. 126 = wind >= 126 knots, 127 = N/A
    pub wgust: u64,
    /// Wind bearing in degrees true, 0-359, 360 = N/A
    pub wdir: u64,
    /// Wind gust bearing in degrees true, 0-359, 360 = N/A
    pub wgustdir: u64,
    /// Air temperature, dry bulb, -60 to +60 in 0.1C, -1024 = N/A
    pub airtemp: f64,
    /// Dew point, -20 to +50 in 0.1C, 501 = N/A
    pub dewpoint: f64,
    /// Air pressure, 800-1200 hPa, 0 = pressure <= 799, 402 = pressure >= 1201, 511 = N/A
    pub pressure: u64,
    /// Air pressure tendency, 0 steady, 1 decreasing, 2 increasing, 3 = N/A
    pub pressuretend: u64,
    /// Visibilty greater than something. Actually BOOL FIXME
    pub visgreater: f64,
    /// Visibility in nautical miles, 127 = N/A
    pub visibility: f64,
    /// Water level, -10.0 to +30.0 in 0.01m, 4001 = N/A
    pub waterlevel: f64,
    /// Water level trend, 0 steady, 1 decreasing, 2 increasing, 3 = N/A
    pub leveltrend: u64,
    // --------------------
    pub cspeed: f64,
    pub cdir: u64,
    pub cspeed2: f64,
    pub cdir2: u64,
    pub cdepth2: u64,
    pub cspeed3: f64,
    pub cdir3: u64,
    pub cdepth3: u64,
    /// Wave height in metres. 0 - 25m in 0.1. 251 = height >= 25.1. 255 = N/A
    pub waveheight: f64,
    /// Wave period in seconds. 0 - 60. 63 = N/A
    pub waveperiod: u64,
    pub swellheight: f64,
    pub swellperiod: u64,
    pub seastate: u64,
    pub watertemp: f64,
    /// Precipitation type, 1=Rain,2=Thunderstorm,3=Freezing Rain,4=Mixed/ice,5=Snow,7=N/A
    pub preciptype: u64,
    pub salinity: f64,
    // Ice, 0 No, 1 Yes, 2 Reserved, 3 = N/A
    pub ice: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct AisMessageIdentifier {
    /// Message type
    pub r#type: u64,
    /// Designated Area Code
    pub dac: Option<u64>,
    /// Functional ID
    pub fid: Option<u64>,
}

impl From<&AisMessage> for AisMessageIdentifier {
    fn from(f: &AisMessage) -> Self {
        let dac = match &f.msg.get("dac") {
            Some(serde_json::Value::Number(n)) => Some(n.as_u64().unwrap()),
            _ => None,
        };
        let fid = match &f.msg.get("fid") {
            Some(serde_json::Value::Number(n)) => Some(n.as_u64().unwrap()),
            _ => None,
        };
        AisMessageIdentifier {
            r#type: f.msg["type"].as_u64().unwrap(),
            dac,
            fid,
        }
    }
}

impl fmt::Display for AisMessageIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let dac = match self.dac {
            None => "None".to_string(),
            Some(val) => val.to_string(),
        };
        let fid = match self.fid {
            None => "None".to_string(),
            Some(val) => val.to_string(),
        };
        write!(f, "AisMessageIdentifier({}/{}/{})", self.r#type, dac, fid)
    }
}

/// Load from an optional Value/Number from the named field, defaulting to the supplied
/// value if the data was not present in the source JSON.
fn load_f64(msg: &HashMap<String, serde_json::Value>, field: &str, default: f64) -> f64 {
    match msg.get(field) {
        Some(n) => n.as_f64().unwrap(),
        _ => default,
    }
}

/// Load from an optional Value/Number from the named field, defaulting to the supplied
/// value if the data was not present in the source JSON.
fn load_u64(msg: &HashMap<String, serde_json::Value>, field: &str, default: u64) -> u64 {
    match msg.get(field) {
        // Be gracious in what's accepted. Perhaps the upstream sends a f64 value when the
        // spec details the field as u64. Trust that the significand is correct, and coerce
        // the floating poing value into an integer.
        // https://github.com/jvde-github/AIS-catcher/issues/179
        Some(n) => n
            .as_u64()
            .or_else(|| n.as_f64().map(|f| f as u64))
            .unwrap_or(default),
        None => default,
    }
}

/// Extracts fields from the AisMessage structure, and produces an AisType8Dac200Fid31 structure
impl From<&AisMessage> for AisType8Dac200Fid31 {
    fn from(f: &AisMessage) -> Self {
        // The default value is the *scaled* value from AIS Catcher, based on the details in
        // https://gpsd.gitlab.io/gpsd/AIVDM.html#_meteorological_and_hydrological_data_imo289
        // So, while the datastructure may document watertemp as 501 = N/A, the scaled value
        // is 50.1 (for example).
        AisType8Dac200Fid31 {
            airtemp: load_f64(&f.msg, "airtemp", -1024_f64),
            cdepth2: load_u64(&f.msg, "cdepth2", 31),
            cdepth3: load_u64(&f.msg, "cdepth3", 31),
            cdir2: load_u64(&f.msg, "cdir2", 360),
            cdir3: load_u64(&f.msg, "cdir3", 360),
            cdir: load_u64(&f.msg, "cdir", 360),
            cspeed2: load_f64(&f.msg, "cspeed2", 25.5),
            cspeed3: load_f64(&f.msg, "cspeed3", 25.5),
            cspeed: load_f64(&f.msg, "cspeed", 25.5),
            dewpoint: load_f64(&f.msg, "dewpoint", 50.1),
            ice: load_u64(&f.msg, "preciptype", 3),
            lat: load_f64(&f.msg, "lat", 91.000),
            leveltrend: load_u64(&f.msg, "leveltrend", 3),
            lon: load_f64(&f.msg, "lon", 181.000),
            preciptype: load_u64(&f.msg, "preciptype", 7),
            pressure: load_u64(&f.msg, "pressure", 511),
            pressuretend: load_u64(&f.msg, "pressuretend", 3),
            salinity: load_f64(&f.msg, "salinity", 511_f64),
            seastate: load_u64(&f.msg, "seastate", 13),
            swellheight: load_f64(&f.msg, "swellheight", 25.5),
            swellperiod: load_u64(&f.msg, "swellperiod", 360),
            visgreater: load_f64(&f.msg, "visgreater", 1_f64),
            visibility: load_f64(&f.msg, "visibility", 12.7),
            waterlevel: load_f64(&f.msg, "waterlevel", 30.01),
            watertemp: load_f64(&f.msg, "watertemp", 50.1),
            waveheight: load_f64(&f.msg, "waveheight", 25.5),
            waveperiod: load_u64(&f.msg, "waveperiod", 63),
            wdir: load_u64(&f.msg, "wdir", 360),
            wgust: load_u64(&f.msg, "wgust", 127),
            wgustdir: load_u64(&f.msg, "wgustdir", 360),
            wspeed: load_u64(&f.msg, "wspeed", 127),
        }
    }
}

/// Converts an AisType8Dac200Fid31 into a set of key/value pairs that line up with what the ERDDAP
/// system is configured to store.
impl AisType8Dac200Fid31 {
    pub fn as_query_arguments(&self) -> Vec<(&str, String)> {
        vec![
            ("airtemp", self.airtemp.to_string()),
            ("cdepth2", self.cdepth2.to_string()),
            ("cdepth3", self.cdepth3.to_string()),
            ("cdir", self.cdir.to_string()),
            ("cdir2", self.cdir2.to_string()),
            ("cdir3", self.cdir3.to_string()),
            ("cspeed", self.cspeed.to_string()),
            ("cspeed2", self.cspeed2.to_string()),
            ("cspeed3", self.cspeed3.to_string()),
            ("dewpoint", self.dewpoint.to_string()),
            ("ice", self.ice.to_string()),
            ("lat", format!("{:.3}", self.lat)),
            ("leveltrend", self.leveltrend.to_string()),
            ("lon", format!("{:.3}", self.lon)),
            ("preciptype", self.preciptype.to_string()),
            ("pressure", self.pressure.to_string()),
            ("pressuretend", self.pressuretend.to_string()),
            ("salinity", self.salinity.to_string()),
            ("seastate", self.seastate.to_string()),
            ("swellheight", self.swellheight.to_string()),
            ("swellperiod", self.swellperiod.to_string()),
            ("visgreater", self.visgreater.to_string()),
            ("visibility", self.visibility.to_string()),
            ("waterlevel", self.waterlevel.to_string()),
            ("watertemp", self.watertemp.to_string()),
            ("waveheight", self.waveheight.to_string()),
            ("waveperiod", self.waveperiod.to_string()),
            ("wdir", self.wdir.to_string()),
            ("wgust", self.wgust.to_string()),
            ("wgustdir", self.wgustdir.to_string()),
            ("wspeed", self.wspeed.to_string()),
        ]
    }
}

/// Application configuration from file
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    /// URL of the ERDDAP service, including protocol and path, not including .insert
    pub erddap_url: String,
    /// Username_Password author key for the ERDDAP service
    pub erddap_key: String,
    /// List of field names to publish to ERDDAP. This enables the ERDDAP service to be
    /// configured with a subset of the full IMO289 data, without having to recompile
    /// the code.
    pub publish_fields: Vec<String>,
    /// A remapping of field names to support requirements of the remote system
    pub rename_fields: Vec<(String, String)>,
    /// Defines a configuration of acceptable message types, and MMSIs to ignore
    pub message_config: Vec<AcceptedMessage>,
    /// Map MMSIs (Mobile Marine Service Identifier) to string names to provide a
    /// human-friendly station name in the data posted to ERDDAP.
    pub mmsi_lookup: Vec<MMSILookup>,
}

/// A TOML table entry for a MMSI and the station name to use for that MMSI
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MMSILookup {
    /// The Marine Mobile Service Identifier from the AIS message
    pub mmsi: String,
    /// The name to give the MMSI.
    pub station_name: String,
}

/// A TOML table entry for a packet to accept for decoding
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AcceptedMessage {
    /// The type number from the AIS specification, such as 8 for weather
    pub r#type: u64,
    /// Designated Area Code, only valid for binary messages (type 8 for instance)
    pub dac: Option<u64>,
    // Functional ID, only valid for binary messages (type 8 for instance)
    pub fid: Option<u64>,
    /// List of MMSIs to ignore, such as test ATONs.
    pub ignore_mmsi: Vec<u64>,
}

/// Used to pass configuration data into the ArgsState struct for passing around in the
/// program.
#[derive(Debug, Clone)]
pub struct PerMessageConfig {
    /// List of MMSIs to ignore, such as test ATONs.
    pub ignore_mmsi: Vec<u64>,
}

impl ::std::default::Default for AppConfig {
    fn default() -> Self {
        Self {
            erddap_url: DEFAULT_URL.to_string(),
            erddap_key: DEFAULT_KEY.to_string(),
            publish_fields: vec![
                "lat".to_string(),
                "lon".to_string(),
                "wspeed".to_string(),
                "wgust".to_string(),
                "wdir".to_string(),
                "wgustdir".to_string(),
                "waveheight".to_string(),
                "waveperiod".to_string(),
            ],
            rename_fields: vec![
                ("lat".to_string(), "latitude".to_string()),
                ("lon".to_string(), "longitude".to_string()),
            ],
            message_config: vec![AcceptedMessage {
                r#type: 8,
                dac: Some(200),
                fid: Some(31),
                ignore_mmsi: vec![],
            }],
            mmsi_lookup: vec![MMSILookup {
                mmsi: DEFAULT_MMSI.to_string(),
                station_name: "MMSI Name".to_string(),
            }],
        }
    }
}

// ERDDAP responds with camelCase, and for deserialization to work, this struct has to match.
#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
/// Structure for the ERDDAP submission response
pub struct ErddapResponse {
    pub status: String,
    pub nRowsReceived: u16,
    pub stringTimestamp: String,
    pub numericTimestamp: f64,
}

/// CLI state data for Axum to pass around; everything from args has to be in here
#[derive(Clone)]
pub struct ArgsState {
    pub url: String,
    pub author_key: String,
    pub publish_fields: Vec<String>,
    pub rename_fields: HashMap<String, String>,
    pub dump_all_packets: bool,
    pub dump_accepted_messages: bool,
    pub mmsi_lookup: HashMap<String, String>,
    pub message_config_lookup: HashMap<AisMessageIdentifier, PerMessageConfig>,
}
