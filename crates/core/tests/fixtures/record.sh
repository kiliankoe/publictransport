#!/usr/bin/env zsh
# Records the EFA fixtures again. Departures and trips are queried for the
# current time so that they contain real-time data; expect every value in them
# to change.
set -euo pipefail
cd "${0:A:h}/efa"

common='outputFormat=rapidJSON&coordOutputFormat=WGS84%5Bdd.ddddd%5D'

# record <network> <endpoint> <stop search term> <origin stop id> <destination stop id>
record() {
  local network=$1 endpoint=$2 term=$3 origin=$4 destination=$5
  fetch() { curl --silent --show-error --fail --max-time 30 --user-agent curl --output "$1" "$endpoint/$2&$common" }
  fetch "$network-stopfinder.json" "XML_STOPFINDER_REQUEST?type_sf=any&name_sf=$term&anyMaxSizeHitList=5"
  fetch "$network-departures.json" "XML_DM_REQUEST?type_dm=any&name_dm=$origin&mode=direct&useRealtime=1&limit=10"
  fetch "$network-trip.json" "XML_TRIP_REQUEST2?type_origin=any&name_origin=$origin&type_destination=any&name_destination=$destination&useRealtime=1&calcNumberOfTrips=3"
}

# Dresden Hauptbahnhof to Albertplatz
record vvo https://efa.vvo-online.de/std3 Hauptbahnhof de:14612:28 de:14612:13
# Düsseldorf Hbf to Essen Hbf
record vrr https://efa.vrr.de/standard 'D%C3%BCsseldorf%20Hbf' de:05111:18235 de:05113:9289
