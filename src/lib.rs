use serde::Deserialize;
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct AisCatcherReceiver {
    pub description: String,
    #[allow(dead_code)]
    pub version: u32,
    #[allow(dead_code)]
    pub engine: String,
    #[allow(dead_code)]
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
pub struct AisMessage {
    #[serde(flatten)]
    pub msg: HashMap<String, serde_json::Value>,
}
#[derive(Deserialize, Debug)]
pub struct AisCatcherMessage {
    #[allow(dead_code)]
    pub protocol: String,
    #[allow(dead_code)]
    pub encodetime: String,
    pub stationid: String,
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
            // Ewww?
            rxtime: dt_ref,
        }
    }
}

impl AisStationData {
    pub fn as_query_arguments(&self) -> Vec<(&str, String)> {
        let station_id = match self.mmsi.to_string().as_str() {
            "992509976" => "TEST_CIL_ATON",
            "992501301" => "Dublin_Bay_Buoy",
            "992501017" => "Kish_Lighthouse",
            _ => "UNKNOWN",
        };
        let qa = vec![
            // ERDDAP expects these keys as lower case
            ("latitude", format!("{:.3}", self.latitude)),
            ("longitude", format!("{:.3}", self.longitude)),
            ("time", self.rxtime.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            // ERDDAP expects this key as upper case
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

// CLI state data for Axum to pass around; everything from args has to be in here
#[derive(Clone)]
pub struct ArgsState {
    pub url: String,
    pub author_key: String,
    pub dump_all_packets: bool,
}
