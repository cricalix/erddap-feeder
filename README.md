# ERDDAP Feeder

A written-for-me tool that accepts JSON packets over HTTP from AIS-catcher, mangles the data, and sends it onwards to an ERDDAP server that's configured with a HttpGet dataset.

# Setup

For this software to make any sense, you must have access to an ERDDAP service that has a HttpGet tabledap configured. A sample datasets.xml configuration is provided, which has fields for
* every IMO289 meteorological and hydrological value,
* the station_name provided by this software,
* the mandatory time, command, and author fields.

This ERDDAP service must be available over HTTPS, otherwise it won't accept the message.

To initialize the new tabledap, use resources/init.jsonl - this will provide one record that ERDDAP can ingest to set up the data storage based on the entries in resources/datasets.xml.

Note that the **ioos_category** field in the datasets.xml file is not what you'll find at https://mmisw.org/ont/ioos/category. Instead, it comes from the hard-coded (https://github.com/ERDDAP/erddap/blob/main/WEB-INF/classes/gov/noaa/pfel/erddap/variable/EDV.java)[IOOS_CATEGORIES] in the ERDDAP source code.
