#!/usr/bin/env python3
"""Makes the Swift code that UniFFI generates read like Swift.

UniFFI's generator has no settings for what this script changes, so it
edits the generated file. Every edit has to find its place: if a new
version of UniFFI writes the code differently, or the Rust library gains
something the script has no answer for, it fails and says which edit no
longer fits.

Usage: swift-idioms.py <generated Swift file>
"""

import re
import sys
from pathlib import Path

MARKER = "// Adapted for Swift by scripts/swift-idioms.py.\n"
# The types that crates/core/uniffi.toml renames for Swift.
RENAMED = {
    "Error": "PublicTransportError",
    "Protocol": "NetworkProtocol",
    "Operation": "NetworkOperation",
    "Method": "HttpMethod",
    "Network": "TransitNetwork",
}
# Methods without arguments that Swift writes as properties, with the
# number of types that have one. They only read fields.
PROPERTIES = {
    "expectedTime": 3,
    "delaySeconds": 3,
    "platform": 4,
    "durationSeconds": 1,
    "departure": 2,
    "arrival": 2,
    "transfers": 1,
    "stationIds": 1,
    "id": 1,
}
# Methods without arguments that stay methods: the first two read the
# entries of networks, which is work, the next two are internal, see
# `hide_helpers`, and the last is UniFFI's own.
METHODS = {
    "networks",
    "network",
    "localScheduledTime",
    "localExpectedTime",
    "uniffiEnsurePublictransportInitialized",
}
ERROR = "PublicTransportError"
# Members that have another name in Swift than the comments from Rust say.
MEMBERS = {
    "Registry/get": "Registry/network(id:)",
    "Registry/covering": "Registry/covering(at:)",
    "Coverage/tierAt": "Coverage/tier(at:)",
    "Coordinate/distanceMeters": "Coordinate/distanceMeters(to:)",
    "Location/sameStation": "Location/isSameStation(with:)",
    "Location/timezone": "Location/timeZoneIdentifier",
    # Internal in Swift, see `hide_helpers`.
    "Location/localTime": "Location/timeZone",
    "TransitNetwork/timezone": "TransitNetwork/timeZoneIdentifier",
    "TransitNetwork/fromJson": "TransitNetwork/init(id:json:)",
}
# Types that are not `Codable`: they describe a network, which has its own
# JSON form and can hold credentials, or an HTTP exchange.
NOT_CODABLE = {
    "TransitNetwork",
    "NetworkProtocol",
    "NetworkOperation",
    "HafasOptions",
    "HafasProduct",
    "Header",
    "HttpRequest",
    "HttpResponse",
    "HttpMethod",
    "Coverage",
    "CoverageTier",
    "Area",
    "Polygon",
    "Attribution",
    "CoveringNetwork",
}
# Enums without values are stored as the name of their case, except these,
# which are stored as the named property.
CODING_NAMES = {"BundledNetwork": "id"}


def fail(what: str) -> None:
    sys.exit(f"swift-idioms: {what}")


class Code:
    def __init__(self, text: str) -> None:
        self.text = text

    def replace(self, what: str, pattern: str, replacement: str, count: int | None = None) -> int:
        """Replace every match, which have to be `count` many, or at least one."""
        self.text, found = re.subn(pattern, replacement, self.text, flags=re.MULTILINE)
        if found == 0 or (count is not None and found != count):
            fail(f"{what}: found {found} places, expected {count or 'some'}")
        return found


def lower_first(name: str) -> str:
    return name[0].lower() + name[1:]


def camel(name: str) -> str:
    first, *rest = name.split("_")
    return first + "".join(part.capitalize() for part in rest)


def error_cases(code: Code) -> None:
    """Write the cases of the error in lower camel case, as Swift does."""
    start = code.text.find(f"enum {ERROR}:")
    end = code.text.find(f"public func FfiConverterType{ERROR}_lower")
    if start < 0 or end < start:
        fail("the error type and its converter")
    region = Code(code.text[start:end])
    cases = re.findall(r"^    case ([A-Z]\w*)", region.text, flags=re.MULTILINE)
    if not cases:
        fail("the cases of the error")
    for case in set(cases):
        region.replace(f"error case {case}", rf"(case |\.){case}\b", rf"\1{lower_first(case)}")
    code.text = code.text[:start] + region.text + code.text[end:]


def enums_without_values(code: Code) -> None:
    """Let the enums whose cases hold no values list their cases."""
    found = 0
    for enum in re.finditer(r"^public enum (\w+): Equatable, Hashable\b.*\{$", code.text, flags=re.MULTILINE):
        body = code.text[enum.end() : code.text.index("\n}\n", enum.end())]
        cases = re.findall(r"^    case .*$", body, flags=re.MULTILINE)
        if cases and not any("(" in case for case in cases):
            found += code.replace(
                f"cases of {enum[1]}",
                rf"^public enum {enum[1]}: Equatable, Hashable\b",
                f"public enum {enum[1]}: Equatable, Hashable, CaseIterable",
                1,
            )
    if found == 0:
        fail("enums without values")


def debug_text(code: Code) -> None:
    """Drop the debug text from Rust, so that Swift prints a value with its
    own names: `globalIds`, not `global_ids`. A type that also has a
    description keeps both: Rust leaves a secret out of them there, so a
    type that hides a field has to export `Display` next to `Debug`."""
    declaration = re.compile(
        r"^(public (?:struct|enum) \w+: [^{\n]*?), CustomDebugStringConvertible( \{)$", flags=re.MULTILINE
    )
    implementation = re.compile(
        # The call into Rust closes once, then the property.
        r"\n *\n// The local Rust `Debug` implementation\.\npublic var debugDescription: String \{\n"
        r".*?\n    \)\n\}\n    \)\n\}\n",
        flags=re.DOTALL,
    )
    removed = 0
    while match := declaration.search(code.text):
        body = implementation.search(code.text, match.end())
        following = declaration.search(code.text, match.end())
        if body is None or (following and following.start() < body.start()):
            fail(f"debug text of {match[1]}")
        code.text = (
            code.text[: match.start()]
            + match[1]
            + match[2]
            + code.text[match.end() : body.start()]
            + "\n"
            + code.text[body.end() :]
        )
        removed += 1
    if removed == 0:
        fail("debug text")
    kept = len(re.findall(r"CustomDebugStringConvertible, CustomStringConvertible", code.text))
    if len(re.findall(r"public var debugDescription", code.text)) != kept:
        fail("debug text: an implementation is left without its type")


def codable(code: Code) -> None:
    """Make the values of queries and answers `Codable`. Swift writes the
    code for a type with fields. An enum without values gets code that
    stores it as one string; Swift's own form is an object per case."""
    declaration = re.compile(r"^public (struct|enum) (\w+): Equatable, Hashable([^{\n]*) \{$", flags=re.MULTILINE)
    seen = set()
    extensions = []

    def conform(match: re.Match[str]) -> str:
        kind, name, rest = match.groups()
        seen.add(name)
        if name in NOT_CODABLE:
            return match[0]
        if "CaseIterable" not in rest:
            return f"public {kind} {name}: Equatable, Hashable, Codable{rest} {{"
        if "CustomStringConvertible" in rest and name not in CODING_NAMES:
            fail(f"codable: {name} has a description, so the name of a case needs another source")
        coding_name = f"\\.{CODING_NAMES[name]}" if name in CODING_NAMES else "{ String(describing: $0) }"
        extensions.append(
            f"extension {name}: Codable {{\n"
            f"    public init(from decoder: any Decoder) throws {{\n"
            f"        self = try decodeCase(from: decoder, named: {coding_name})\n"
            f"    }}\n\n"
            f"    public func encode(to encoder: any Encoder) throws {{\n"
            f"        try encodeCase(self, to: encoder, named: {coding_name})\n"
            f"    }}\n"
            f"}}\n"
        )
        return match[0]

    code.text = declaration.sub(conform, code.text)
    if stale := (NOT_CODABLE | set(CODING_NAMES)) - seen:
        fail(f"codable: no such types: {sorted(stale)}")
    types = set(re.findall(r"^public (?:struct|enum) (\w+)", code.text, flags=re.MULTILINE))
    if missed := {name for name in types - seen - {ERROR} if not name.startswith("FfiConverter")}:
        fail(f"codable: decide for {sorted(missed)}")
    if not extensions:
        fail("codable: enums without values")
    code.text += "\n// Enums without values are stored as one string, see Codable.swift.\n" + "\n".join(extensions)


def hide_helpers(code: Code) -> None:
    """Keep what only the generated code calls out of the public names."""
    code.replace("converter types", r"^public struct (FfiConverterType\w+)", r"struct \1")
    code.replace("converter functions", r"^public func (FfiConverterType\w+_(?:lift|lower))", r"func \1")
    code.replace("initialization", r"^public func (uniffiEnsure\w+)", r"func \1", 1)
    # Nobody outside can make a registry from a handle or a subclass of
    # it, so the members UniFFI needs for that can stay inside.
    code.replace("registry class", r"^open class Registry\b", "public final class Registry", 1)
    code.replace("methods of the registry", r"^open func ", "public func ")
    code.replace("empty handle", r"^    public struct NoHandle \{\n        public init\(\) \{\}", "    struct NoHandle {\n        init() {}", 1)
    code.replace("initializer without a handle", r"^    public init\(noHandle:", "    init(noHandle:", 1)
    code.replace("initializer from a handle", r"^    required public init\(unsafeFromHandle", "    required init(unsafeFromHandle", 1)
    code.replace("handle clone", r"^    public func uniffiCloneHandle\(", "    func uniffiCloneHandle(", 1)
    # Nothing takes the protocol of the registry, and nobody can adopt it.
    code.replace("registry protocol", r"^public protocol RegistryProtocol\b", "protocol RegistryProtocol", 1)
    # Swift shows a `Date` in the `TimeZone` of a location, see
    # `Location.timeZone`, and has no use for the local time as text.
    code.replace("local time", r"^public func localTime\(", "func localTime(", 1)
    for name in ("localScheduledTime", "localExpectedTime"):
        code.replace(name, rf"^public func {name}\(", f"func {name}(", 3)
    code.replace("local time type", r"^public typealias LocalTime\b", "typealias LocalTime", 1)
    declared = r"^(?:public|open) (?:final )?(?:struct|enum|class|protocol|func|typealias) (\w+)"
    for name in re.findall(declared, code.text, flags=re.MULTILINE):
        if re.search(r"Ffi|[Uu]niffi|Protocol$", name) and name not in RENAMED.values():
            fail(f"{name} is a helper and still public")


def documentation(code: Code) -> None:
    """Turn the links and words of rustdoc in the comments into Swift's."""

    def link(match: re.Match[str]) -> str:
        owner, member = match[1], match[2]
        owner = RENAMED.get(owner, owner)
        if member is None:
            return f"``{owner}``"
        member = camel(member) if member[0].islower() else lower_first(member)
        return f"``{MEMBERS.get(f'{owner}/{member}', f'{owner}/{member}')}``"

    def comment(match: re.Match[str]) -> str:
        line = re.sub(r"\[`(\w+)(?:::(\w+))?`\](?:\([^)\s]*\))?", link, match[0])
        line = line.replace("`None`", "`nil`")
        return re.sub(r"\bcrate\b", "library", line)

    # The lines of a block comment start with a star.
    before = code.text
    code.text = re.sub(r"^\s*(?:/\*\*|\*).*$", comment, code.text, flags=re.MULTILINE)
    if code.text == before:
        fail("documentation")


def main() -> None:
    path = Path(sys.argv[1])
    code = Code(path.read_text())
    if MARKER in code.text:
        fail(f"{path} was adapted before; generate it again")

    # UniFFI 0.32.2 renames the types but not their converters where a
    # method of a record or an enum throws or returns the type, or a trait
    # of the type is exported. Nor does it rename the type that a method of
    # an enum returns. Without this the code does not compile.
    for old, new in RENAMED.items():
        code.replace(
            f"converter of {old}",
            rf"FfiConverterType{old}_(lift|lower)",
            rf"FfiConverterType{new}_\1",
        )
    code.replace(
        "return type of BundledNetwork.network()",
        r"^public func network\(\) -> Network\b",
        "public func network() -> TransitNetwork",
        1,
    )

    # The message of an error is the one from Rust, which the generated
    # code has as `description`, where it would print the case.
    code.replace("error description", r"String\(reflecting: self\)", "description", 1)

    for name, count in PROPERTIES.items():
        code.replace(
            f"property {name}",
            rf"^public func {name}\(\) -> ([^{{\n]+?) *\{{$",
            rf"public var {name}: \1 {{",
            count,
        )
    for method in re.findall(r"^(?:public|open) func (\w+)\(\)", code.text, flags=re.MULTILINE):
        if method not in METHODS:
            fail(f"{method}() has no arguments: make it a property or keep it a method")

    # UniFFI's rename table reaches the methods of objects only.
    code.replace(
        "label of tier(at:)",
        r"public func tierAt\(position: Coordinate\)",
        "public func tier(at position: Coordinate)",
        1,
    )
    code.replace(
        "label of isSameStation(with:)",
        r"public func sameStation\(other: Location\)",
        "public func isSameStation(with other: Location)",
        1,
    )
    code.replace(
        "label of distanceMeters(to:)",
        r"public func distanceMeters\(other: Coordinate\)",
        "public func distanceMeters(to other: Coordinate)",
        1,
    )
    # `TransitNetwork(id:json:)` in Client.swift takes its place.
    code.replace("networkFromJson", r"^public func networkFromJson\(", "func networkFromJson(", 1)

    error_cases(code)
    enums_without_values(code)
    debug_text(code)
    codable(code)
    hide_helpers(code)
    documentation(code)

    path.write_text(MARKER + code.text)


if __name__ == "__main__":
    main()
