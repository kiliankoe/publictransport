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
# The arrivals there
fetch efa/vvo-arrivals.json 'https://efa.vvo-online.de/std3/XML_DM_REQUEST?type_dm=any&name_dm=de:14612:28&mode=direct&useRealtime=1&limit=10&itdDateTimeDepArr=arr&outputFormat=rapidJSON&coordOutputFormat=WGS84%5Bdd.ddddd%5D'
# The stops within 500 m of a position at Dresden Hauptbahnhof
fetch efa/vvo-nearby.json 'https://efa.vvo-online.de/std3/XML_COORD_REQUEST?coord=13.7325:51.0400:WGS84%5Bdd.ddddd%5D&inclFilter=1&type_1=STOP&radius_1=500&outputFormat=rapidJSON&coordOutputFormat=WGS84%5Bdd.ddddd%5D'
# A stop that does not exist
fetch efa/vvo-departures-unknown-stop.json 'https://efa.vvo-online.de/std3/XML_DM_REQUEST?type_dm=any&name_dm=de:14612:99999&mode=direct&useRealtime=1&limit=10&outputFormat=rapidJSON&coordOutputFormat=WGS84%5Bdd.ddddd%5D'
# Düsseldorf Hbf to Essen Hbf
efa vrr https://efa.vrr.de/standard 'D%C3%BCsseldorf%20Hbf' de:05111:18235 de:05113:9289

# The same Dresden queries as for VVO. Transitous has no station-level entry
# for Albertplatz, so the trip ends at one of its platforms.
motis=https://api.transitous.org/api
fetch motis/transitous-geocode.json "$motis/v1/geocode?text=Dresden+Hauptbahnhof"
fetch motis/transitous-stoptimes.json "$motis/v6/stoptimes?stopId=de-DELFI_de:14612:28&n=10"
fetch motis/transitous-arrivals.json "$motis/v6/stoptimes?stopId=de-DELFI_de:14612:28&n=10&arriveBy=true"
fetch motis/transitous-nearby.json "$motis/v1/reverse-geocode?place=51.0400,13.7325&type=STOP&numResults=5"
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
  trias "<StopEventRequest><Location><LocationRef><StopPointRef>$1</StopPointRef><LocationName><Text>$1</Text></LocationName></LocationRef></Location><Params><NumberOfResults>10</NumberOfResults><StopEventType>${2:-departure}</StopEventType><IncludeRealtimeData>true</IncludeRealtimeData></Params></StopEventRequest>"
}

ojp_departures() {
  ojp "<ojp:OJPStopEventRequest><ojp:Location><ojp:PlaceRef><ojp:StopPlaceRef>$1</ojp:StopPlaceRef><ojp:LocationName><ojp:Text>$1</ojp:Text></ojp:LocationName></ojp:PlaceRef></ojp:Location><ojp:Params><ojp:NumberOfResults>10</ojp:NumberOfResults><ojp:StopEventType>${2:-departure}</ojp:StopEventType><ojp:IncludeRealtimeData>true</ojp:IncludeRealtimeData></ojp:Params></ojp:OJPStopEventRequest>"
}

# The Dresden queries again, on VVO's TRIAS server.
vvo_trias=https://efa.vvo-online.de/std3/trias
post trias/vvo-locations.xml $vvo_trias "$(trias '<LocationInformationRequest><InitialInput><LocationName>Hauptbahnhof</LocationName></InitialInput><Restrictions><NumberOfResults>5</NumberOfResults></Restrictions></LocationInformationRequest>')"
post trias/vvo-locations-no-match.xml $vvo_trias "$(trias '<LocationInformationRequest><InitialInput><LocationName>qqqqxzy</LocationName></InitialInput><Restrictions><NumberOfResults>5</NumberOfResults></Restrictions></LocationInformationRequest>')"
post trias/vvo-nearby.xml $vvo_trias "$(trias '<LocationInformationRequest><InitialInput><GeoRestriction><Circle><Center><Longitude>13.7325</Longitude><Latitude>51.0400</Latitude></Center><Radius>500</Radius></Circle></GeoRestriction></InitialInput><Restrictions><Type>stop</Type><NumberOfResults>5</NumberOfResults></Restrictions></LocationInformationRequest>')"
# A circle without stops
post trias/vvo-nearby-empty.xml $vvo_trias "$(trias '<LocationInformationRequest><InitialInput><GeoRestriction><Circle><Center><Longitude>13.9000</Longitude><Latitude>51.2300</Latitude></Center><Radius>50</Radius></Circle></GeoRestriction></InitialInput><Restrictions><Type>stop</Type></Restrictions></LocationInformationRequest>')"
post trias/vvo-departures.xml $vvo_trias "$(trias_departures de:14612:28)"
post trias/vvo-arrivals.xml $vvo_trias "$(trias_departures de:14612:28 arrival)"
post trias/vvo-departures-unknown-stop.xml $vvo_trias "$(trias_departures de:14612:99999)"
post trias/vvo-trip.xml $vvo_trias "$(trias '<TripRequest><Origin><LocationRef><StopPointRef>de:14612:28</StopPointRef><LocationName><Text>de:14612:28</Text></LocationName></LocationRef></Origin><Destination><LocationRef><StopPointRef>de:14612:13</StopPointRef><LocationName><Text>de:14612:13</Text></LocationName></LocationRef></Destination><Params><NumberOfResults>3</NumberOfResults><IncludeIntermediateStops>true</IncludeIntermediateStops></Params></TripRequest>')"

# The Düsseldorf queries on VRR's servers. Its OJP server has departures,
# which VVO's lacks.
post trias/vrr-departures.xml https://efa.vrr.de/std3/trias "$(trias_departures de:05111:18235)"
vrr_ojp=https://efa.vrr.de/std3/ojp
post ojp/vrr-locations.xml $vrr_ojp "$(ojp '<ojp:OJPLocationInformationRequest><ojp:InitialInput><ojp:LocationName>Düsseldorf Hbf</ojp:LocationName></ojp:InitialInput><ojp:Restrictions><ojp:NumberOfResults>5</ojp:NumberOfResults></ojp:Restrictions></ojp:OJPLocationInformationRequest>')"
post ojp/vrr-nearby.xml $vrr_ojp "$(ojp '<ojp:OJPLocationInformationRequest><ojp:InitialInput><ojp:GeoRestriction><ojp:Circle><ojp:Center><Longitude>6.7943</Longitude><Latitude>51.2198</Latitude></ojp:Center><ojp:Radius>500</ojp:Radius></ojp:Circle></ojp:GeoRestriction></ojp:InitialInput><ojp:Restrictions><ojp:Type>stop</ojp:Type><ojp:NumberOfResults>5</ojp:NumberOfResults></ojp:Restrictions></ojp:OJPLocationInformationRequest>')"
post ojp/vrr-departures.xml $vrr_ojp "$(ojp_departures de:05111:18235)"
post ojp/vrr-arrivals.xml $vrr_ojp "$(ojp_departures de:05111:18235 arrival)"
post ojp/vrr-trip.xml $vrr_ojp "$(ojp '<ojp:OJPTripRequest><ojp:Origin><ojp:PlaceRef><ojp:StopPlaceRef>de:05111:18235</ojp:StopPlaceRef><ojp:LocationName><ojp:Text>de:05111:18235</ojp:Text></ojp:LocationName></ojp:PlaceRef></ojp:Origin><ojp:Destination><ojp:PlaceRef><ojp:StopPlaceRef>de:05113:9289</ojp:StopPlaceRef><ojp:LocationName><ojp:Text>de:05113:9289</ojp:Text></ojp:LocationName></ojp:PlaceRef></ojp:Destination><ojp:Params><ojp:NumberOfResults>3</ojp:NumberOfResults><ojp:IncludeIntermediateStops>true</ojp:IncludeIntermediateStops></ojp:Params></ojp:OJPTripRequest>')"
post ojp/vvo-departures.xml https://efa.vvo-online.de/std3/ojp "$(ojp_departures de:14612:28)"

# hafas <output file> <endpoint> <envelope without svcReqL> <method> <request>
hafas() {
  curl --silent --show-error --fail --max-time 30 --user-agent "$user_agent" \
    --header 'Content-Type: application/json' --output "$1" "$2" \
    --data "{$3,\"svcReqL\":[{\"meth\":\"$4\",\"req\":$5}]}"
}

# Berlin Alexanderplatz to Zoologischer Garten on VBB's HAFAS server, with
# the requests of the library.
vbb=(https://fahrinfo.vbb.de/bin/mgate.exe '"lang":"de","client":{"type":"WEB","id":"VBB","name":"VBB WebApp","l":"vs_webapp_vbb"},"ext":"VBB.1","ver":"1.33","auth":{"type":"AID","aid":"hafas-vbb-webapp"}')
mkdir -p hafas
hafas hafas/vbb-locations.json $vbb LocMatch '{"input":{"loc":{"type":"ALL","name":"Alexanderplatz?"},"maxLoc":5,"field":"S"}}'
hafas hafas/vbb-nearby.json $vbb LocGeoPos '{"ring":{"cCrd":{"x":13413215,"y":52521918},"maxDist":500},"getStops":true,"getPOIs":false,"maxLoc":5}'
hafas hafas/vbb-departures.json $vbb StationBoard '{"type":"DEP","stbLoc":{"lid":"A=1@L=900100003@"},"maxJny":10}'
hafas hafas/vbb-arrivals.json $vbb StationBoard '{"type":"ARR","stbLoc":{"lid":"A=1@L=900100003@"},"maxJny":10}'
hafas hafas/vbb-departures-unknown-stop.json $vbb StationBoard '{"type":"DEP","stbLoc":{"lid":"A=1@L=999999999@"},"maxJny":10}'
hafas hafas/vbb-trip.json $vbb TripSearch '{"depLocL":[{"lid":"A=1@L=900100003@"}],"arrLocL":[{"lid":"A=1@L=900023201@"}],"outFrwd":true,"numF":3,"getPasslist":true}'

# Oslo S to Oslo lufthavn on Entur: the stop search on its geocoder, the
# rest on its OpenTripPlanner server (Transmodel API). The queries are those
# of crates/core/src/otp.rs. Entur asks for the name of the client.
entur_client='ET-Client-Name: kiliankoe-publictransport'
mkdir -p otp
curl --silent --show-error --fail --max-time 30 --user-agent "$user_agent" --header "$entur_client" \
  --output otp/entur-geocode.json 'https://api.entur.io/geocoder/v1/autocomplete?text=Oslo+S&size=5'

quay='fragment quay on Quay{id name publicCode latitude longitude timeZone stopPlace{name}}'
line='fragment line on Line{publicCode name transportMode transportSubmode authority{name}}'
situation='fragment situation on PtSituationElement{situationNumber summary{value} description{value} infoLinks{uri} validityPeriods{startTime endTime}}'
call="fragment call on EstimatedCall{aimedArrivalTime expectedArrivalTime aimedDepartureTime expectedDepartureTime realtime cancellation destinationDisplay{frontText} quay{...quay} scheduledQuay{publicCode} serviceJourney{line{...line}}} $quay $line $situation"
departures="query(\$id:String!,\$limit:Int,\$start:DateTime){stopPlace(id:\$id){estimatedCalls(numberOfDepartures:\$limit,startTime:\$start,includeCancelledTrips:true){...call situations{...situation} notices{text}}}} $call"
trips="query(\$from:Location!,\$to:Location!,\$time:DateTime,\$arriveBy:Boolean,\$limit:Int,\$page:String){trip(from:\$from,to:\$to,dateTime:\$time,arriveBy:\$arriveBy,numTripPatterns:\$limit,pageCursor:\$page){previousPageCursor nextPageCursor tripPatterns{legs{mode aimedStartTime expectedStartTime aimedEndTime expectedEndTime realtime fromPlace{...place} toPlace{...place} line{...line} situations{...situation} fromEstimatedCall{...call notices{text}} toEstimatedCall{...call} intermediateEstimatedCalls{...call}}}}} fragment place on Place{name latitude longitude quay{...quay}} $call"

# otp <output file> <query> <variables>
otp() {
  curl --silent --show-error --fail --max-time 30 --user-agent "$user_agent" --header "$entur_client" \
    --header 'Content-Type: application/json' --output "$1" https://api.entur.io/journey-planner/v3/graphql \
    --data "{\"query\":\"$2\",\"variables\":$3}"
}
otp otp/entur-departures.json $departures '{"id":"NSR:StopPlace:59872","limit":10}'
otp otp/entur-arrivals.json ${departures/includeCancelledTrips:true/includeCancelledTrips:true,arrivalDeparture:arrivals} '{"id":"NSR:StopPlace:59872","limit":10}'
otp otp/entur-departures-unknown-stop.json $departures '{"id":"NSR:StopPlace:0"}'
otp otp/entur-trip.json $trips '{"from":{"place":"NSR:StopPlace:59872"},"to":{"place":"NSR:StopPlace:58211"},"limit":3}'
otp otp/entur-nearby.json '{nearest(latitude:59.9109,longitude:10.7530,maximumDistance:500,maximumResults:5,filterByPlaceTypes:[stopPlace]){edges{node{distance place{... on StopPlace{id name latitude longitude}}}}}}' '{}'
