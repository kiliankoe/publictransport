//! Remarks on departures and on the legs of a trip: disruption messages and
//! hints about a ride. The HAFAS ones are in `hafas.rs`.

use publictransport::{Departure, HttpResponse, Network, Protocol, Remark, RemarkKind, Timestamp};

const EFA_VVO: &[u8] = include_bytes!("fixtures/efa/vvo-departures.json");
const EFA_VRR: &[u8] = include_bytes!("fixtures/efa/vrr-departures.json");
const EFA_TRIP: &[u8] = include_bytes!("fixtures/efa/vrr-trip.json");
const TRIAS_VVO: &[u8] = include_bytes!("fixtures/trias/vvo-departures.xml");
const TRIAS_VRR: &[u8] = include_bytes!("fixtures/trias/vrr-departures.xml");
const OJP_VRR: &[u8] = include_bytes!("fixtures/ojp/vrr-departures.xml");
const OJP_TRIP: &[u8] = include_bytes!("fixtures/ojp/vrr-trip.xml");

const XML: [(Protocol, &[u8]); 2] = [(Protocol::Trias, TRIAS_VRR), (Protocol::Ojp1, OJP_VRR)];

/// EFA reads the validity of a message on the clocks of this time zone.
fn network(protocol: Protocol) -> Network {
    Network::new("test", "Test", "https://example.org/base", protocol)
        .with_timezone("Europe/Berlin")
        .with_requestor_ref("OpenService")
}

fn response(body: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

fn parse(protocol: Protocol, body: &[u8]) -> Vec<Departure> {
    let parsed = network(protocol).parse_departures(&response(body));
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or_default()
}

/// The remarks of the first departure of the line `name`.
fn of_line<'a>(departures: &'a [Departure], name: &str) -> &'a [Remark] {
    let departure = departures.iter().find(|d| d.line.name == name);
    assert!(departure.is_some(), "no {name}");
    departure.map_or(&[], |departure| &departure.remarks)
}

/// The remarks of the departure towards `direction`.
fn towards<'a>(departures: &'a [Departure], direction: &str) -> &'a [Remark] {
    let departure = departures
        .iter()
        .find(|d| d.direction.as_deref() == Some(direction));
    assert!(departure.is_some(), "no {direction}");
    departure.map_or(&[], |departure| &departure.remarks)
}

/// A time of day on 2026-11-16 in UTC, the day the fixtures were recorded.
fn utc(hour: i64, minute: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_794_787_200 + hour * 3600 + minute * 60,
    }
}

/// Kind, title, text, url, id and validity of a remark. The text has one
/// space for each run of white space, because its layout is not fixed.
type Fields<'a> = (
    &'a RemarkKind,
    Option<&'a str>,
    Option<String>,
    Option<&'a str>,
    Option<&'a str>,
    Option<Timestamp>,
    Option<Timestamp>,
);

fn fields(remark: &Remark) -> Fields<'_> {
    let text = remark.text.as_deref();
    (
        &remark.kind,
        remark.title.as_deref(),
        text.map(|text| text.split_whitespace().collect::<Vec<_>>().join(" ")),
        remark.url.as_deref(),
        remark.id.as_deref(),
        remark.valid_from,
        remark.valid_until,
    )
}

/// The remarks of one kind, in their order. The order between a disruption
/// and a hint is not fixed: EFA sends them in two lists.
fn of_kind(remarks: &[Remark], kind: RemarkKind) -> Vec<Fields<'_>> {
    let matching = remarks.iter().filter(|remark| remark.kind == kind);
    matching.map(fields).collect()
}

fn disruptions(remarks: &[Remark]) -> Vec<Fields<'_>> {
    of_kind(remarks, RemarkKind::Disruption)
}

fn hints(remarks: &[Remark]) -> Vec<Fields<'_>> {
    of_kind(remarks, RemarkKind::Hint)
}

/// A hint has its text and nothing else.
fn hint(text: &str) -> Fields<'static> {
    let text = Some(text.to_owned());
    (&RemarkKind::Hint, None, text, None, None, None, None)
}

/// The text of the one remark in `remarks`, a disruption, which has no markup
/// left. Each run of white space in it is one space here.
fn plain_text(remarks: &[Remark]) -> String {
    assert_eq!(remarks.len(), 1, "{remarks:?}");
    let disruptions = of_kind(remarks, RemarkKind::Disruption);
    let text: String = disruptions
        .into_iter()
        .filter_map(|fields| fields.2)
        .collect();
    let text = text.as_str();
    assert!(!text.is_empty(), "{remarks:?}");
    for markup in ["<", "&nbsp;", "\r"] {
        assert!(!text.contains(markup), "{markup:?} in {text:?}");
    }
    assert_eq!(text, text.trim(), "white space around the text");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn efa_reads_a_disruption_with_its_link_and_validity_and_a_hint_with_its_text() {
    let departures = parse(Protocol::Efa, EFA_VRR);

    // The title repeats as the subtitle. The validity is on Berlin's clocks.
    let train = of_line(&departures, "RE13");
    assert_eq!(train.len(), 3, "{train:?}");
    assert_eq!(
        disruptions(train),
        [(
            &RemarkKind::Disruption,
            Some("Strecke zw. Boisheim und Venlo beeinträchtigt - Reparatur an einer Weiche"),
            Some(
                "Strecke zw. Boisheim und Venlo beeinträchtigt. Bisher keine Informationen zur \
                 Dauer der Beeinträchtigung. Grund: Reparatur an einer Weiche. Der Halt Boisheim \
                 entfällt. Bitte überprüfen Sie den Zuglauf in der Onlinereiseauskunft."
                    .to_owned()
            ),
            Some("http://ems.vrr.de/info-link/864a15f5-07ec-5c1f-861d-84beed3d69a7"),
            Some("ZTP-PROD-141281"),
            Some(utc(11, 0)),
            Some(utc(15, 0)),
        )]
    );
    assert_eq!(
        hints(train),
        [
            hint("Linie RE13: Klimaanlage"),
            hint("Vorfahrt eines anderen Zuges")
        ]
    );

    // Another subtitle follows the title, and the text is one link.
    let subway = of_line(&departures, "U75");
    let text = plain_text(subway);
    assert!(text.starts_with("Hier alle Infos"), "{text:?}");
    assert!(
        text.contains("https://www.rheinbahn.de/Burgunderstrasse"),
        "{text:?}"
    );
    assert_eq!(
        subway[0].title.as_deref(),
        Some(
            "Fahrleitungsarbeiten in Heerdt für einen verlässlichen Betrieb: Busse statt Bahnen \
             auf der U75"
        )
    );
    // Until 2026-11-20 at 14:00 in Berlin.
    assert_eq!(
        (subway[0].valid_from, subway[0].valid_until),
        (Some(utc(7, 0)), Some(utc(4 * 24 + 13, 0)))
    );
}

#[test]
fn efa_turns_html_into_plain_text_and_drops_an_address_inside_the_operators_network() {
    // VVO links to `http://192.168.150.204:8081/...` and sends no validity.
    let departures = parse(Protocol::Efa, EFA_VVO);
    let bus = of_line(&departures, "360");
    let text = plain_text(bus);
    for words in [
        "Aufgrund des Dresdner Striezelmarktes wird die Wilsdruffer Straße",
        "Die „GuteNachtLinien“ können daher",
        "Dresden, Postplatz",
        "Dresden, Pirnaischer Platz",
    ] {
        assert!(text.contains(words), "{words:?} in {text:?}");
    }
    let (kind, title, _, url, id, valid_from, valid_until) = fields(&bus[0]);
    assert_eq!(
        (kind, title, url, id, valid_from, valid_until),
        (
            &RemarkKind::Disruption,
            Some("Linienaenderung Sperrung der Wilsdruffer Straße in Dresden"),
            None,
            Some("28134"),
            None,
            None
        )
    );

    // VRR writes `&bdquo;Nordstra&szlig;e&ldquo;` and `durchgef&uuml;hrt`.
    let departures = parse(Protocol::Efa, EFA_VRR);
    let text = plain_text(of_line(&departures, "U79"));
    assert!(
        text.contains("im U-Bahnhof „Nordstraße“ ab Sommer"),
        "{text:?}"
    );
    assert!(text.contains("durchgeführt"), "{text:?}");
    assert!(!text.contains('&'), "{text:?}");
}

#[test]
fn efa_keeps_the_order_separates_paragraphs_and_titles_a_message_by_its_link_text() {
    let info = |id: u8, url: &str| {
        format!(
            r#"{{"id":"{id}","infoLinks":[{{"urlText":"Umleitung","url":"{url}","content":"<p>Eins &amp; zwei</p><p>drei</p>"}}]}}"#
        )
    };
    // Only 10.0.0.0/8, 172.16.0.0/12 and 192.168.0.0/16 are private.
    let body = format!(
        r#"{{"version":"11.0.6.72","stopEvents":[{{"location":{{"id":"de:14612:28","isGlobalId":true,"name":"Dresden Hauptbahnhof","type":"stop"}},"departureTimePlanned":"2026-11-16T13:24:00Z","transportation":{{"number":"8","product":{{"class":4,"name":"Straßenbahn"}}}},"infos":[{},{},{}]}}]}}"#,
        info(1, "http://10.1.2.3/x"),
        info(2, "http://172.16.0.1/x"),
        info(3, "http://172.32.0.1/x"),
    );
    let departures = parse(Protocol::Efa, body.as_bytes());
    let remarks = &departures[0].remarks;
    let urls: Vec<_> = remarks
        .iter()
        .map(|r| (r.id.as_deref(), r.url.as_deref()))
        .collect();
    assert_eq!(
        urls,
        [
            (Some("1"), None),
            (Some("2"), None),
            (Some("3"), Some("http://172.32.0.1/x"))
        ]
    );
    assert_eq!(remarks[0].title.as_deref(), Some("Umleitung"));
    let text = remarks[0].text.as_deref().expect("a text");
    let lines = text.split('\n').map(str::trim);
    let paragraphs: Vec<_> = lines.filter(|line| !line.is_empty()).collect();
    assert_eq!(paragraphs, ["Eins & zwei", "drei"]);
}

/// Munich's server titles a message in `subtitle` alone, and several servers
/// send a link that nobody can open: no host, a made-up host, a path.
#[test]
fn efa_titles_a_message_by_its_subtitle_and_drops_a_link_that_cannot_open() {
    let info = |url: &str| {
        format!(
            r#"{{"infoLinks":[{{"title":null,"subtitle":"S2: Ersatzverkehr (bis 30.11.)","urlText":"S2","url":"{url}","content":null}}]}}"#
        )
    };
    let urls = [
        "http://",
        "http://nohost",
        "http://localhost:8080/ics",
        "/27d3d5c8",
        "https://www.vgn.de/27d3d5c8",
    ];
    let body = format!(
        r#"{{"version":"11.0.6.72","stopEvents":[{{"location":{{"id":"de:09162:6","name":"Hbf","type":"stop"}},"departureTimePlanned":"2026-11-16T13:24:00Z","transportation":{{"number":"S2","product":{{"class":1,"name":"S-Bahn"}}}},"infos":[{}]}}]}}"#,
        urls.map(info).join(",")
    );
    let departures = parse(Protocol::Efa, body.as_bytes());
    let remarks = &departures[0].remarks;
    assert_eq!(
        remarks[0].title.as_deref(),
        Some("S2: Ersatzverkehr (bis 30.11.)")
    );
    let kept: Vec<_> = remarks.iter().filter_map(|r| r.url.as_deref()).collect();
    assert_eq!(kept, ["https://www.vgn.de/27d3d5c8"]);
}

/// Both fixtures have the RE 1 from Essen with the broken elevator there.
/// EFA sends its validity on Berlin's clocks and a page, OJP sends it in UTC.
#[test]
fn efa_and_ojp_describe_the_remarks_of_the_same_leg_alike() {
    let trips = |protocol, body| {
        network(protocol)
            .parse_trips(
                &publictransport::TripsQuery::new("from", "to"),
                &response(body),
            )
            .unwrap()
    };
    let (efa, ojp) = (
        trips(Protocol::Efa, EFA_TRIP),
        trips(Protocol::Ojp1, OJP_TRIP),
    );
    let efa = &efa.trips[0].legs[0].remarks;
    let ojp = &ojp.trips[0].legs[0].remarks;

    let elevator = |url| {
        (
            &RemarkKind::Disruption,
            Some("Haltestelle Essen Hauptbahnhof: Aufzug außer Betrieb."),
            Some(
                "An der Haltestelle Essen Hauptbahnhof ist der Aufzug von Verteilerebene zum \
                 Bahnsteig bis auf Weiteres außer Betrieb."
                    .to_owned(),
            ),
            url,
            Some("VRR-de:05113:9289:32"),
            Some(utc(1, 8)),
            Some(utc(25, 0)),
        )
    };
    assert_eq!(
        disruptions(efa),
        [elevator(Some(
            "http://ems.vrr.de/info-link/21d409e6-66a1-57b9-8054-6b22d5365450"
        ))]
    );
    assert_eq!(disruptions(ojp), [elevator(None)]);

    assert_eq!(hints(efa), hints(ojp));
    assert_eq!(hints(ojp).len(), 8, "{ojp:?}");
    assert_eq!(hints(ojp)[0], hint("Linie RE1: Laptop-Steckdosen"));
    // Only a hint has a code: the one the operator gives its kind.
    let codes = |remarks: &[Remark]| -> Vec<_> {
        let coded = remarks.iter().filter(|remark| remark.code.is_some());
        coded
            .map(|remark| (remark.kind, remark.code.clone()))
            .collect()
    };
    assert_eq!(codes(efa), codes(ojp));
    assert_eq!(codes(ojp).len(), 8);
    assert_eq!(codes(ojp)[0], (RemarkKind::Hint, Some("A__LS".to_owned())));
}

#[test]
fn trias_and_ojp_resolve_the_situations_of_a_departure_and_read_attributes_as_hints() {
    for (protocol, body) in XML {
        let departures = parse(protocol, body);
        let train = towards(&departures, "Aachen, Hbf");
        assert_eq!(train.len(), 3, "{protocol:?} {train:?}");
        assert_eq!(
            disruptions(train),
            [(
                &RemarkKind::Disruption,
                Some("Strecke zw. Düsseldorf Hbf und Düsseldorf-Bilk beeinträchtigt - Bauarbeiten"),
                Some(
                    "Strecke zw. Düsseldorf Hbf und Düsseldorf-Bilk beeinträchtigt. \
                     Beeinträchtigung von 16.11.2026, 22:00 Uhr bis vsl. 17.11.2026, 05:00 Uhr. \
                     Grund: Bauarbeiten. Kurzfristige Änderungen im Zuglauf möglich. Es kommt zu \
                     Verspätungen, Teilausfällen und Haltausfällen. Bitte Reiseverbindung prüfen."
                        .to_owned()
                ),
                None,
                Some("ZTP-PROD-141302"),
                Some(utc(21, 0)),
                Some(utc(28, 0)),
            )],
            "{protocol:?}"
        );
        assert_eq!(
            hints(train),
            [
                hint("Linie RE4: WLAN verfügbar"),
                hint("Linie RE4: Klimaanlage")
            ],
            "{protocol:?}"
        );
        // Most departures refer to no situation and have no attribute.
        assert!(towards(&departures, "Hagen Hbf").is_empty(), "{protocol:?}");
    }

    // Two situations in the order of their references, with HTML in them.
    let departures = parse(Protocol::Trias, TRIAS_VVO);
    let train = towards(&departures, "Pirna Bahnhof");
    let ids: Vec<_> = train.iter().map(|remark| remark.id.as_deref()).collect();
    assert_eq!(ids, [Some("27979"), Some("27784")]);
    let text = plain_text(&train[..1]);
    assert!(text.contains("Sehr geehrte Damen und Herren,"), "{text:?}");
}

#[test]
fn trias_and_ojp_skip_a_situation_the_answer_lacks_and_fall_back_to_the_description() {
    let number = "<siri:SituationNumber>ZTP-PROD-141302</siri:SituationNumber><siri:Version>";
    let description = "beeinträchtigt - Bauarbeiten</siri:Description>";
    for (protocol, body) in XML {
        let body = String::from_utf8_lossy(body);
        assert!(body.contains(number) && body.contains(description));

        // The reference of the departure now leads nowhere.
        let lacking = body.replace(number, &number.replace("141302", "0"));
        let departures = parse(protocol, lacking.as_bytes());
        let train = towards(&departures, "Aachen, Hbf");
        assert!(disruptions(train).is_empty(), "{protocol:?} {train:?}");
        assert_eq!(hints(train).len(), 2, "{protocol:?}");

        let without_detail = body
            .replace("siri:Detail", "siri:Unread")
            .replace(description, "beeinträchtigt (kurz)</siri:Description>");
        let departures = parse(protocol, without_detail.as_bytes());
        let train = towards(&departures, "Aachen, Hbf");
        assert_eq!(
            disruptions(train)[0].2.as_deref(),
            Some("Strecke zw. Düsseldorf Hbf und Düsseldorf-Bilk beeinträchtigt (kurz)"),
            "{protocol:?}"
        );
    }
}

/// A remark comes next to a departure, so a server that writes one in a way
/// no recorded answer showed must not cost the departures.
#[test]
fn remark_of_an_unknown_shape_costs_only_itself() {
    let complete = parse(Protocol::Efa, EFA_VRR).len();
    let odd = String::from_utf8_lossy(EFA_VRR)
        .replacen(r#""infos":["#, r#""infos":{"entries":5},"other":["#, 1)
        .replacen(r#""hints":["#, r#""hints":[5,null,{"content":7},"#, 1);
    assert_eq!(parse(Protocol::Efa, odd.as_bytes()).len(), complete);

    for (protocol, body) in [(Protocol::Trias, TRIAS_VRR), (Protocol::Ojp1, OJP_VRR)] {
        let complete = parse(protocol, body);
        // A summary with tags that the server did not escape.
        let odd = String::from_utf8_lossy(body)
            .replace("</siri:Summary>", " <b>fett</b> nach</siri:Summary>");
        let departures = parse(protocol, odd.as_bytes());
        assert_eq!(departures.len(), complete.len(), "{protocol:?}");
        assert!(
            departures
                .iter()
                .all(|departure| disruptions(&departure.remarks).is_empty())
        );
    }
}

/// A text without tags and entities is no HTML, and converting it would
/// join its lines and take the address for a tag.
#[test]
fn text_that_is_no_html_stays_as_it_is() {
    let body = r#"{"version":"11.0.6.72","stopEvents":[{"location":{"id":"de:14612:28","isGlobalId":true,"name":"Dresden Hauptbahnhof","type":"stop"},"departureTimePlanned":"2026-11-16T13:24:00Z","transportation":{"number":"8","product":{"class":4,"name":"Straßenbahn"}},"hints":[{"content":" Zeile 1\nZeile 2: 3 < 4, Q&A <info@example.org> "}]}]}"#;
    let departures = parse(Protocol::Efa, body.as_bytes());
    let texts: Vec<_> = departures
        .iter()
        .flat_map(|departure| &departure.remarks)
        .map(|remark| remark.text.as_deref())
        .collect();
    assert_eq!(
        texts,
        [Some("Zeile 1\nZeile 2: 3 < 4, Q&A <info@example.org>")]
    );
}
