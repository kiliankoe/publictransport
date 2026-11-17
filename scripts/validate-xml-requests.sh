#!/usr/bin/env zsh
# Checks the TRIAS and OJP requests that the library builds against the
# official schemas. The schemas are fetched from the VDV repositories and not
# kept in this repository.
#
# The library leaves out RequestTimestamp, which the schemas require (see
# crates/core/src/trias.rs). This script adds the element before it validates,
# so that everything else is still checked.
#
# A trip request asks for no trips on one side of the time with a count of
# zero, which the servers take and the schema of TRIAS does not allow. This
# script turns the zero into a one for TRIAS.
set -euo pipefail
cd "${0:A:h}/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git -c advice.detachedHead=false clone --quiet --depth 1 --branch v1.2 https://github.com/VDVde/TRIAS "$work/trias"
git -c advice.detachedHead=false clone --quiet --depth 1 --branch v1.0.3 https://github.com/VDVde/OJP "$work/ojp"

mkdir "$work/requests"
cargo run --quiet -p publictransport --example xml_requests -- "$work/requests"

timestamp='RequestTimestamp>2026-11-16T13:24:00Z</'
for request in "$work"/requests/*.xml; do
  case ${request:t} in
    trias-*)
      schema=$(print -l "$work"/trias/**/Trias.xsd | head -1)
      sed -i.bak -E "s|<siri:RequestorRef>|<siri:$timestamp""siri:RequestTimestamp><siri:RequestorRef>|; s|(<NumberOfResults[A-Za-z]+>)0<|\\11<|g" "$request"
      ;;
    ojp-*)
      schema=$(print -l "$work"/ojp/**/OJP.xsd | head -1)
      # Once in the envelope and once at the start of the request itself.
      sed -i.bak -E "s|<RequestorRef>|<$timestamp""RequestTimestamp><RequestorRef>|; s|(<ojp:OJP[A-Za-z]+Request>)|\1<$timestamp""RequestTimestamp>|" "$request"
      ;;
  esac
  # The schemas import SIRI several times, which xmllint warns about.
  xmllint --noout --schema "$schema" "$request" 2>&1 |
    grep --invert-match 'Schemas parser warning' | sed "s|$work/requests/||"
done
