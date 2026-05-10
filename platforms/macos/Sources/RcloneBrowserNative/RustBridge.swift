import CRcloneCore
import Foundation

enum RustBridgeError: LocalizedError {
    case unavailable
    case encoding(String)
    case decoding(String)
    case backend(String)

    var errorDescription: String? {
        switch self {
        case .unavailable: return "The Rust backend is unavailable."
        case .encoding(let detail): return "Could not prepare the request: \(detail)"
        case .decoding(let detail): return "The Rust backend returned invalid data: \(detail)"
        case .backend(let detail): return detail
        }
    }
}

private struct BridgeRequest<Payload: Encodable>: Encodable {
    var command: String
    var payload: Payload
}

private struct BridgeResponse<Result: Decodable>: Decodable {
    var ok: Bool
    var data: Result?
    var error: String?
}

private struct VoidBridgeResponse: Decodable {
    var ok: Bool
    var error: String?
}

enum RustBridge {
    static func call<Result: Decodable, Payload: Encodable>(
        _ command: String,
        payload: Payload,
        as type: Result.Type = Result.self
    ) throws -> Result {
        let request: Data
        do {
            request = try JSONEncoder().encode(BridgeRequest(command: command, payload: payload))
        } catch {
            throw RustBridgeError.encoding(error.localizedDescription)
        }
        guard let text = String(data: request, encoding: .utf8) else {
            throw RustBridgeError.encoding("The request is not valid UTF-8.")
        }
        let pointer = text.withCString { rb_call($0) }
        guard let pointer else { throw RustBridgeError.unavailable }
        defer { rb_string_free(pointer) }
        let responseData = Data(String(cString: pointer).utf8)
        let response: BridgeResponse<Result>
        do {
            response = try JSONDecoder().decode(BridgeResponse<Result>.self, from: responseData)
        } catch {
            throw RustBridgeError.decoding(error.localizedDescription)
        }
        guard response.ok else {
            throw RustBridgeError.backend(response.error ?? "The Rust backend reported an unknown error.")
        }
        guard let result = response.data else {
            throw RustBridgeError.decoding("The response did not contain a result.")
        }
        return result
    }

    static func callVoid<Payload: Encodable>(_ command: String, payload: Payload) throws {
        let request: Data
        do {
            request = try JSONEncoder().encode(BridgeRequest(command: command, payload: payload))
        } catch {
            throw RustBridgeError.encoding(error.localizedDescription)
        }
        guard let text = String(data: request, encoding: .utf8) else {
            throw RustBridgeError.encoding("The request is not valid UTF-8.")
        }
        let pointer = text.withCString { rb_call($0) }
        guard let pointer else { throw RustBridgeError.unavailable }
        defer { rb_string_free(pointer) }
        let response: VoidBridgeResponse
        do {
            response = try JSONDecoder().decode(VoidBridgeResponse.self, from: Data(String(cString: pointer).utf8))
        } catch {
            throw RustBridgeError.decoding(error.localizedDescription)
        }
        if !response.ok { throw RustBridgeError.backend(response.error ?? "The operation failed.") }
    }
}
