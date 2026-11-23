#!/usr/bin/env python3
"""Makes the Kotlin code that UniFFI generates compile and read like Kotlin.

UniFFI's generator has no settings for what this script changes, so it
edits the generated code. Every edit has to find its place: if a new
version of UniFFI writes the code differently, or the Rust library gains
something the script has no answer for, it fails and says which edit no
longer fits.

Usage: kotlin-idioms.py <generated Kotlin file> <adapted Kotlin file>
"""

import re
import sys
from pathlib import Path

# The name that crates/core/uniffi.toml gives the error of the library.
ERROR = "PublicTransportException"
# Methods without arguments that Kotlin writes as properties, with the
# number of types that have one. They only read fields.
PROPERTIES = {
    "expectedTime": 3,
    "localScheduledTime": 3,
    "localExpectedTime": 3,
    "delaySeconds": 3,
    "platform": 3,
    "durationSeconds": 1,
    "departure": 1,
    "arrival": 1,
    "transfers": 1,
    "stationIds": 1,
    "id": 1,
}
# Methods without arguments that stay methods: they read the entries of
# networks, which is work. With the number of places that declare one.
METHODS = {"networks": 2, "network": 1, "bundled": 1}


def fail(what: str) -> None:
    sys.exit(f"kotlin-idioms: {what}")


class Code:
    def __init__(self, text: str) -> None:
        self.text = text

    def replace(self, what: str, pattern: str, replacement: str, count: int | None = None) -> None:
        """Replace every match, which have to be `count` many, or at least one."""
        self.text, found = re.subn(pattern, replacement, self.text, flags=re.MULTILINE)
        if found == 0 or (count is not None and found != count):
            fail(f"{what}: found {found} places, expected {count or 'some'}")


def main() -> None:
    code = Code(Path(sys.argv[1]).read_text())

    # UniFFI 0.32.2 renames the error but not its uses where a method of a
    # record throws it or a trait of it is exported. Without this the code
    # does not compile.
    code.replace("thrown error", r"@Throws\(Exception::class\)", f"@Throws({ERROR}::class)")
    code.replace("handler of the error", r"uniffiRustCallWithError\(Exception\)", f"uniffiRustCallWithError({ERROR})")
    code.replace("converter of the error", r"\bFfiConverterTypeError\b", "FfiConverterTypePublicTransportError")

    # The message of an exception is the sentence from Rust, which the
    # generated code has as `toString`, where it would list the fields.
    code.replace("messages of the error cases", r"^ +override val message\n +get\(\) = \".*\"\n", "")
    code.replace(
        "message of the error",
        rf"^(sealed class {ERROR}: kotlin\.Exception\(\) \{{\n)",
        r"\1    override val message: String\n        get() = toString()\n",
        1,
    )

    for name, count in PROPERTIES.items():
        code.replace(f"property {name}", rf"\bfun `{name}`\(\): (.+) \{{$", rf"val `{name}`: \1 get() {{", count)
    found = re.findall(r"\bfun `(\w+)`\(\)", code.text)
    for method in sorted(set(found)):
        if found.count(method) != METHODS.get(method):
            fail(f"{method}() has no arguments: make it a property or keep it a method")

    Path(sys.argv[2]).write_text(code.text)


if __name__ == "__main__":
    main()
