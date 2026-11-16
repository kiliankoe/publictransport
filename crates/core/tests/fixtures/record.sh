#!/usr/bin/env zsh
# Records the fixtures again. Departures and trips are queried for the current
# time so that they contain real-time data; expect every value in them to
# change.
set -euo pipefail
cd "${0:A:h}"

# Transitous requires a User-Agent with application name, version and contact.
user_agent='publictransport/0.0.0 (+https://github.com/kiliankoe/publictransport; publictransport@kilko.de)'

# fetch <output file> <url>
fetch() {
  curl --silent --show-error --fail --max-time 30 --user-agent "$user_agent" --output "$1" "$2"
}

# efa <network> <endpoint> <stop search term> <origin stop id> <destination stop id>
efa() {
  local network=$1 endpoint=$2 term=$3 origin=$4 destination=$5
  local common='outputFormat=rapidJSON&coordOutputFormat=WGS84%5Bdd.ddddd%5D'
  fetch "efa/$network-stopfinder.json" "$endpoint/XML_STOPFINDER_REQUEST?type_sf=any&name_sf=$term&anyMaxSizeHitList=5&$common"
  fetch "efa/$network-departures.json" "$endpoint/XML_DM_REQUEST?type_dm=any&name_dm=$origin&mode=direct&useRealtime=1&limit=10&$common"
  fetch "efa/$network-trip.json" "$endpoint/XML_TRIP_REQUEST2?type_origin=any&name_origin=$origin&type_destination=any&name_destination=$destination&useRealtime=1&calcNumberOfTrips=3&$common"
}

# Dresden Hauptbahnhof to Albertplatz
efa vvo https://efa.vvo-online.de/std3 Hauptbahnhof de:14612:28 de:14612:13
# A stop that does not exist
fetch efa/vvo-departures-unknown-stop.json 'https://efa.vvo-online.de/std3/XML_DM_REQUEST?type_dm=any&name_dm=de:14612:99999&mode=direct&useRealtime=1&limit=10&outputFormat=rapidJSON&coordOutputFormat=WGS84%5Bdd.ddddd%5D'
# Düsseldorf Hbf to Essen Hbf
efa vrr https://efa.vrr.de/standard 'D%C3%BCsseldorf%20Hbf' de:05111:18235 de:05113:9289

# The same Dresden queries as for VVO. Transitous has no station-level entry
# for Albertplatz, so the trip ends at one of its platforms.
motis=https://api.transitous.org/api
fetch motis/transitous-geocode.json "$motis/v1/geocode?text=Dresden+Hauptbahnhof"
fetch motis/transitous-stoptimes.json "$motis/v6/stoptimes?stopId=de-DELFI_de:14612:28&n=10"
fetch motis/transitous-plan.json "$motis/v6/plan?fromPlace=de-DELFI_de:14612:28&toPlace=de-DELFI_de:14612:13:1:1&detailedTransfers=false"
