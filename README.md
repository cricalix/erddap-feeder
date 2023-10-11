# ERDDAP Feeder

A written-for-me tool that accepts JSON packets over HTTP from AIS-catcher, mangles the data, and sends it onwards to an ERDDAP server that's configured with a HttpGet dataset.

# Design

The feeder program is designed around submitting to a single ERDDAP service, using a single ERDDAP author key. Multiple message types can be configured by type/dac/fid (DAC and FID apply to binary types only); you can restrict which fields are published, and rename fields as necessary.

# Setup

For this software to make any sense, you must have access to an ERDDAP service that has a HttpGet tabledap configured. A sample datasets.xml configuration is provided, which has fields for
* every IMO289 meteorological and hydrological value,
* the station_name provided by this software (from the MMSI lookup table),
* the mandatory time, command, and author fields.

This ERDDAP service must be available over HTTPS, otherwise it won't accept the message.

To initialize the new tabledap, use resources/init.jsonl and resources/datasets.xml. The XML file defines a dataset that should be updated based on the [ERDDAP datasets documentation](https://coastwatch.pfeg.noaa.gov/erddap/download/setupDatasetsXml.html).

Note that the **ioos_category** field in the datasets.xml file is not what you'll find at https://mmisw.org/ont/ioos/category. Instead, it comes from the hard-coded [IOOS_CATEGORIES](https://github.com/ERDDAP/erddap/blob/main/WEB-INF/classes/gov/noaa/pfel/erddap/variable/EDV.java) in the ERDDAP source code.

# Running ERDDAP Feeder

On the first run, a configuration file will be created with some default configuration. You'll want to edit the generated file. You can also specify `--config-file` if you want to have several configuration files.

### erddap_url

This should be a HTTPS URL that includes the full ERDDAP path to the tabledap, but should not include the `.insert` fragment that ERDDAP uses to detect an insertion.

### erddap_key

This should be provided by your ERDDAP administrator (or you, if you're the administrator), and is the `author_key` in ERDDAP parlance.

### rename_fields

This list of lists is used to rename fields from the datastructure into the names that the ERDDAP service is expecting. By default, "lat" and "lon" are converted to "latitude" and "longitude" - the IMO289 specification says lon/lat are the names, and this is what AIS-Catcher will send, but ERDDAP mandates that the fields are called longitude and latitude.

### message_config
#### type/dac/fid

This is the AIS message type. It's a required field for the `message_config` table.

For binary types, such as type 8, you will want to specify a Designated Area Code (dac) and Functional ID (fid). Non-binary types do not need these values defined (as in, you can delete those two lines).

#### ignore_mmsi

This is a list of Maritime Mobile Service Identifiers (MMSIs) that should be ignored by the feeder; they won't be submitted to the ERDDAP service.

#### publish_fields

If this list is empty, every field that's defined on a message will be sent to the ERDDAP instance.

If it's populated, it acts as a restriction filter and only fields from the datastructure for the type/dac/fid that match the list will be published to the ERDDAP service.

### mmsi_lookup

This array of tables (in TOML parlance) maps MMSIs to friendly names. The friendly name may contain spaces. If you don't know the name that goes with a MMSI, consult a tool like Marine Traffic or invent a name. The mapped name is emitted as `station_name` in the HTTP query fragment.
