# Changelog

## 0.2.0 (2026-11-24)

### Breaking changes

- core: Make the ends and the duration of a trip non-optional
- core: Report how long a rate-limited server asks to wait
- core: Add the run of one vehicle
- core: List one network of each operator in the bundled registry

### Features

- core: Add conveniences to departures, leg ends and trips
- core: Add remarks to departures and legs
- core: Add arrivals at a stop
- core: Add a search for the stops near a position
- core: Give a stopover the helpers of a departure
- core: Give TRIAS and OJP stops the position their answer lists
- core: Add a language to a network
- kotlin: Add an Android library

### Fixes

- core: Treat an empty platform code from OpenTripPlanner as none
- core: Keep the times that EFA estimates for a walk
- core: Name the code of a TRIAS error that has no text
- core: Keep the headers of a network from a geocoder on another host
- kotlin: Parse an answer on the thread of OkHttp

## 0.1.2 (2026-11-20)

### Features

- kotlin: Add a Kotlin package
- typescript: Add a TypeScript package

## 0.1.1 (2026-11-19)

Also adds a rudimentary documentation page.

### Features

- core: Show a time on the clocks at a location

## 0.1.0 (2026-11-19)

Initial release 🙌

### Features

- core: Search for locations, list departures and plan trips, with paging for more trips
- core: Support EFA, MOTIS, TRIAS 1.2, OJP 1.0, HAFAS mgate and OpenTripPlanner with the Transmodel API
- core: Bundle 43 networks that were checked against their servers, named by `BundledNetwork`, with a registry that finds the networks for a position
- core: Read further networks from entries in the format of transport-apis
- core: Take a location from one network as a stop or place in another
- client: Add a client crate on reqwest
- python: Add a Python package with a sync and an async client on httpx
- swift: Add a Swift package with a client on URLSession
