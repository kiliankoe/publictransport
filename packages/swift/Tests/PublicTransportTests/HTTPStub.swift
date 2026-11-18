import Foundation
import Synchronization

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// A server that exists only inside `URLSession`. Each stub has a host of its
/// own, so tests that run in parallel do not see each other's requests.
struct HTTPStub: Sendable {
    enum Answer: Sendable {
        /// The body is the given pieces in a row, without a `Content-Length`.
        case response(status: Int, headers: [String: String] = [:], body: [Data])
        case failure(URLError.Code)
    }

    struct Sent: Sendable {
        let request: URLRequest
        let body: Data?
    }

    let host = "\(UUID().uuidString.lowercased()).example"

    init(_ answer: Answer) {
        Self.exchanges.withLock { $0[host] = (answer, []) }
    }

    /// The requests that reached this stub.
    var sent: [Sent] {
        Self.exchanges.withLock { $0[host]?.sent ?? [] }
    }

    /// A session whose requests all go to the stubs.
    var session: URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [Loader.self]
        return URLSession(configuration: configuration)
    }

    private static let exchanges = Mutex<[String: (answer: Answer, sent: [Sent])]>([:])

    private final class Loader: URLProtocol {
        override class func canInit(with _: URLRequest) -> Bool { true }
        override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
        override func stopLoading() {}

        override func startLoading() {
            // A stream on Apple platforms, the data itself on Linux.
            let body = request.httpBody ?? request.httpBodyStream.map(Self.read)
            let sent = Sent(request: request, body: body)
            let answer = HTTPStub.exchanges.withLock { exchanges -> Answer? in
                guard let host = request.url?.host else { return nil }
                exchanges[host]?.sent.append(sent)
                return exchanges[host]?.answer
            }
            guard case .response(let status, let headers, let body) = answer,
                let url = request.url,
                let response = HTTPURLResponse(
                    url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers
                )
            else {
                let code = if case .failure(let code) = answer { code } else { URLError.Code.unsupportedURL }
                client?.urlProtocol(self, didFailWithError: URLError(code))
                return
            }
            #if !canImport(FoundationNetworking)
                // A redirect is offered to the session, which follows it unless
                // the client refuses. Foundation on Linux cannot be offered one.
                if (300..<400).contains(status), let location = headers["Location"],
                    let target = URL(string: location, relativeTo: url)
                {
                    client?.urlProtocol(
                        self, wasRedirectedTo: URLRequest(url: target), redirectResponse: response)
                }
            #endif
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            #if canImport(FoundationNetworking)
                // In one piece: `URLSession` on Linux keeps only the first.
                client?.urlProtocol(self, didLoad: body.reduce(Data(), +))
            #else
                for piece in body {
                    client?.urlProtocol(self, didLoad: piece)
                }
            #endif
            client?.urlProtocolDidFinishLoading(self)
        }

        /// `URLSession` hands a request body to a `URLProtocol` as a stream.
        private static func read(_ stream: InputStream) -> Data {
            stream.open()
            defer { stream.close() }
            var data = Data()
            var buffer = [UInt8](repeating: 0, count: 4096)
            while case let count = stream.read(&buffer, maxLength: buffer.count), count > 0 {
                data.append(buffer, count: count)
            }
            return data
        }
    }
}
