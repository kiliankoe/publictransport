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

# post <output file> <url> <xml body>
post() {
  curl --silent --show-error --fail --max-time 30 --user-agent "$user_agent" \
    --header 'Content-Type: application/xml' --data "$3" --output "$1" "$2"
}

# The requests are those of the library. They leave out RequestTimestamp.

# trias <service request>
trias() {
  print -r -- "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Trias xmlns=\"http://www.vdv.de/trias\" xmlns:siri=\"http://www.siri.org.uk/siri\" version=\"1.2\"><ServiceRequest><siri:RequestorRef>OpenService</siri:RequestorRef><RequestPayload>$1</RequestPayload></ServiceRequest></Trias>"
}

# ojp <service request>
ojp() {
  print -r -- "<?xml version=\"1.0\" encoding=\"UTF-8\"?><OJP xmlns=\"http://www.siri.org.uk/siri\" xmlns:ojp=\"http://www.vdv.de/ojp\" version=\"1.0\"><OJPRequest><ServiceRequest><RequestorRef>OpenService</RequestorRef>$1</ServiceRequest></OJPRequest></OJP>"
}

trias_departures() {
  trias "<StopEventRequest><Location><LocationRef><StopPointRef>$1</StopPointRef><LocationName><Text>$1</Text></LocationName></LocationRef></Location><Params><NumberOfResults>10</NumberOfResults><StopEventType>departure</StopEventType><IncludeRealtimeData>true</IncludeRealtimeData></Params></StopEventRequest>"
}

ojp_departures() {
  ojp "<ojp:OJPStopEventRequest><ojp:Location><ojp:PlaceRef><ojp:StopPlaceRef>$1</ojp:StopPlaceRef><ojp:LocationName><ojp:Text>$1</ojp:Text></ojp:LocationName></ojp:PlaceRef></ojp:Location><ojp:Params><ojp:NumberOfResults>10</ojp:NumberOfResults><ojp:StopEventType>departure</ojp:StopEventType><ojp:IncludeRealtimeData>true</ojp:IncludeRealtimeData></ojp:Params></ojp:OJPStopEventRequest>"
}

# The Dresden queries again, on VVO's TRIAS server.
vvo_trias=https://efa.vvo-online.de/std3/trias
post trias/vvo-locations.xml $vvo_trias "$(trias '<LocationInformationRequest><InitialInput><LocationName>Hauptbahnhof</LocationName></InitialInput><Restrictions><NumberOfResults>5</NumberOfResults></Restrictions></LocationInformationRequest>')"
post trias/vvo-locations-no-match.xml $vvo_trias "$(trias '<LocationInformationRequest><InitialInput><LocationName>qqqqxzy</LocationName></InitialInput><Restrictions><NumberOfResults>5</NumberOfResults></Restrictions></LocationInformationRequest>')"
post trias/vvo-departures.xml $vvo_trias "$(trias_departures de:14612:28)"
post trias/vvo-departures-unknown-stop.xml $vvo_trias "$(trias_departures de:14612:99999)"
post trias/vvo-trip.xml $vvo_trias "$(trias '<TripRequest><Origin><LocationRef><StopPointRef>de:14612:28</StopPointRef><LocationName><Text>de:14612:28</Text></LocationName></LocationRef></Origin><Destination><LocationRef><StopPointRef>de:14612:13</StopPointRef><LocationName><Text>de:14612:13</Text></LocationName></LocationRef></Destination><Params><NumberOfResults>3</NumberOfResults><IncludeIntermediateStops>true</IncludeIntermediateStops></Params></TripRequest>')"

# The Düsseldorf queries on VRR's servers. Its OJP server has departures,
# which VVO's lacks.
post trias/vrr-departures.xml https://efa.vrr.de/std3/trias "$(trias_departures de:05111:18235)"
vrr_ojp=https://efa.vrr.de/std3/ojp
post ojp/vrr-locations.xml $vrr_ojp "$(ojp '<ojp:OJPLocationInformationRequest><ojp:InitialInput><ojp:LocationName>Düsseldorf Hbf</ojp:LocationName></ojp:InitialInput><ojp:Restrictions><ojp:NumberOfResults>5</ojp:NumberOfResults></ojp:Restrictions></ojp:OJPLocationInformationRequest>')"
post ojp/vrr-departures.xml $vrr_ojp "$(ojp_departures de:05111:18235)"
post ojp/vrr-trip.xml $vrr_ojp "$(ojp '<ojp:OJPTripRequest><ojp:Origin><ojp:PlaceRef><ojp:StopPlaceRef>de:05111:18235</ojp:StopPlaceRef><ojp:LocationName><ojp:Text>de:05111:18235</ojp:Text></ojp:LocationName></ojp:PlaceRef></ojp:Origin><ojp:Destination><ojp:PlaceRef><ojp:StopPlaceRef>de:05113:9289</ojp:StopPlaceRef><ojp:LocationName><ojp:Text>de:05113:9289</ojp:Text></ojp:LocationName></ojp:PlaceRef></ojp:Destination><ojp:Params><ojp:NumberOfResults>3</ojp:NumberOfResults><ojp:IncludeIntermediateStops>true</ojp:IncludeIntermediateStops></ojp:Params></ojp:OJPTripRequest>')"
post ojp/vvo-departures.xml https://efa.vvo-online.de/std3/ojp "$(ojp_departures de:14612:28)"
