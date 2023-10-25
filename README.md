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

# Configuration

When run with the `initialize` subcommand, the program will generate a default configuration file. This file must be edited before the `run` subcommand will work.

You will need to set

* `erddap_url`
* `erddap_key`
* `mmsi_lookup`

at a minimum. The other fields are defaulted to a configuration that will provide basic weather data to an ERDDAP service that's configured appropriately.

## Options

### erddap_url

This should be a HTTPS URL that includes the full ERDDAP path to the tabledap, but should not include the `.insert` fragment that ERDDAP uses to detect an insertion.

### erddap_key

This should be provided by your ERDDAP administrator (or you, if you're the administrator), and is the `author_key` in ERDDAP parlance.

### rename_fields

This list of lists is used to rename fields from the datastructure into the names that the ERDDAP service is expecting. By default, "lat" and "lon" are converted to "latitude" and "longitude" (in the default configuration) - the IMO289 specification says lon/lat are the names, and this is what AIS-Catcher will send, but ERDDAP mandates that the fields are called longitude and latitude.

### publish_fields

If this list is empty, every field that's defined on a message will be sent to the ERDDAP instance.

If it's populated, it acts as a restriction filter and only fields from the datastructure that match the list will be published to the ERDDAP service.

### message_config
#### type/dac/fid

This is the AIS message type. It's a required field for the `message_config` table.

For binary types, such as type 8, you will want to specify a Designated Area Code (dac) and Functional ID (fid). Non-binary types do not need these values defined (as in, you can delete those two lines).

#### ignore_mmsi

This is a list of Maritime Mobile Service Identifiers (MMSIs) that should be ignored by the feeder; they won't be submitted to the ERDDAP service.

### mmsi_lookup

This array of tables (in TOML parlance) maps MMSIs to friendly names. The friendly name may contain spaces. If you don't know the name that goes with a MMSI, consult a tool like Marine Traffic or invent a name. The mapped name is emitted as `station_name` in the HTTP query fragment, and the ERDDAP instance will need to accept this field.

# Running ERDDAP Feeder

## Native from source

* `cargo build --release`
* `target/release/erddap-feeder initialize`
  * Edit the configuration file in the source directory
* `target/release/erddap-feeder run`

Both `initialize` and `run` accept `--config-file <name>` to set up additional configuration files. `initialize` will indicate the full path to the configuration file for editing.

The `run` subcommand has several flags, see `erddap-feeder run --help`.

## Docker

The Docker setup runs as a non-privileged user inside the container - `feeder`.

### Via registry

The Docker images are stored on the Github Container Registry - https://github.com/cricalix/erddap-feeder/pkgs/container/erddap-feeder

Images are currently built for **linux/amd64** and **linux/arm/v7**.

* `docker pull ghcr.io/cricalix/erddap-feeder:latest`
* `mkdir $HOME/.config/erddap-feeder`
  * Can be anywhere you want, that's just an example that mirrors the structure used by the program
* `docker run --mount type=bind,source=$HOME/.config/erddap-feeder,target=/home/feeder/.config/erddap-feeder ghcr.io/cricalix/erddap-feeder initialize`
  * Edit the configuration file in the source directory
* `docker run --init --mount type=bind,source=$HOME/.config/erddap-feeder,target=/home/feeder/.config/erddap-feeder -p 22022:22022 ghcr.io/cricalix/erddap-feeder run`
  * `--init` is required to have signals such as Ctrl-C passed through to the `erddap-feeder` process.

### Building

Building and running is similar to running on a bare OS - initialize, edit the configuration, run.

* `cargo build --release`
* `docker build -t erddap-feeder --build-arg release_name=target/release/erddap-feeder .`
* `mkdir $HOME/.config/erddap-feeder`
  * Can be anywhere you want, that's just an example that mirrors the structure used by the program
* `docker run --mount type=bind,source=$HOME/.config/erddap-feeder,target=/home/feeder/.config/erddap-feeder erddap-feeder initialize`
  * Edit the configuration file in the source directory
* `docker run --init --mount type=bind,source=$HOME/.config/erddap-feeder,target=/home/feeder/.config/erddap-feeder -p 22022:22022 erddap-feeder run`
  * `--init` is required to have signals such as Ctrl-C passed through to the `erddap-feeder` process.
