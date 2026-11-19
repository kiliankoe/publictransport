// An enum without values is stored as one string. The generated code calls
// these for each such enum, with what gives the string of a case.

func decodeCase<Value: CaseIterable>(
    from decoder: any Decoder, named name: (Value) -> String
) throws -> Value {
    let container = try decoder.singleValueContainer()
    let stored = try container.decode(String.self)
    guard let value = Value.allCases.first(where: { name($0) == stored }) else {
        throw DecodingError.dataCorruptedError(
            in: container, debugDescription: "\(Value.self) has no case \"\(stored)\"")
    }
    return value
}

func encodeCase<Value>(_ value: Value, to encoder: any Encoder, named name: (Value) -> String) throws {
    var container = encoder.singleValueContainer()
    try container.encode(name(value))
}
