import Foundation
import Network
import UIKit

// Links to AetherPlay, as in the Android app (android/PROTOCOL.md):
// * Wi-Fi: Bonjour `_aetherplay._tcp`, WebSocket ws://host:47820/phone, JSON;
// (the RomM-server relay of the Android app only carries game info, so it
// is not used here: input needs the Wi-Fi link).
// iOS has no Bluetooth controller profile for apps, so the games always get
// this phone through AetherPlay's virtual controller (`"bt": false`).

struct Game: Equatable {
    var platform: String
    var name: String
    var cover: UIImage?
}

struct Server: Hashable, Identifiable {
    var name: String
    var host: String
    var port: Int
    var id: String { "\(host):\(port)" }
}

/// Finds AetherPlay on the local network and resolves it to an address.
final class Discovery: ObservableObject {
    @Published var servers: [Server] = []
    private var browser: NWBrowser?
    private var resolving: [NWConnection] = []

    func start() {
        guard browser == nil else { return }
        let params = NWParameters()
        params.includePeerToPeer = true
        let b = NWBrowser(for: .bonjour(type: "_aetherplay._tcp", domain: nil), using: params)
        b.browseResultsChangedHandler = { [weak self] results, _ in
            for r in results { self?.resolve(r.endpoint) }
        }
        b.start(queue: .main)
        browser = b
    }

    func stop() {
        browser?.cancel()
        browser = nil
        resolving.forEach { $0.cancel() }
        resolving.removeAll()
    }

    private func resolve(_ endpoint: NWEndpoint) {
        guard case let .service(name, _, _, _) = endpoint else { return }
        let params = NWParameters.tcp
        if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options { ip.version = .v4 }
        let c = NWConnection(to: endpoint, using: params)
        c.stateUpdateHandler = { [weak self, weak c] state in
            guard let self, let c else { return }
            switch state {
            case .ready:
                if case let .hostPort(host, port) = c.currentPath?.remoteEndpoint {
                    var h = "\(host)"
                    if let i = h.firstIndex(of: "%") { h = String(h[..<i]) }
                    let s = Server(name: name, host: h, port: Int(port.rawValue))
                    if !self.servers.contains(where: { $0.name == s.name }) { self.servers.append(s) }
                }
                c.cancel()
            case .failed, .cancelled:
                self.resolving.removeAll { $0 === c }
            default: break
            }
        }
        resolving.append(c)
        c.start(queue: .main)
    }
}

enum LinkStatus: Equatable {
    case disconnected, connecting, needPin
    case connected(String)
    case failed(String)
}

/// Wi-Fi link to AetherPlay. All state changes happen on the main queue.
final class Link: ObservableObject {
    @Published private(set) var status: LinkStatus = .disconnected
    @Published private(set) var game: Game?
    @Published private(set) var wifiInput = true
    @Published private(set) var screen: CGSize?
    @Published private(set) var frame: UIImage?
    @Published private(set) var player = 0
    /// Issued at the first pairing, kept per server.
    @Published private(set) var token: String?

    private let session = URLSession(configuration: .default)
    private var task: URLSessionWebSocketTask?
    private var target: Server?
    private var lastSent: PadState?
    private var pinger: Timer?
    private let name = UIDevice.current.name

    func connect(_ server: Server, token: String?) {
        disconnect()
        target = server
        self.token = token
        status = .connecting
        guard let url = URL(string: "ws://\(server.host):\(server.port)/phone") else { return }
        let t = session.webSocketTask(with: url)
        task = t
        t.resume()
        var hello: [String: Any] = ["t": "hello", "v": 1, "name": name]
        if let token { hello["token"] = token }
        send(hello)
        receive(t)
        pinger = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self, weak t] _ in
            t?.sendPing { error in
                if error != nil { DispatchQueue.main.async { self?.lost(t, error) } }
            }
        }
    }

    private func receive(_ t: URLSessionWebSocketTask) {
        t.receive { [weak self] result in
            DispatchQueue.main.async {
                guard let self, t === self.task else { return }
                switch result {
                case let .success(.string(text)):
                    if let data = text.data(using: .utf8),
                       let msg = try? JSONSerialization.jsonObject(with: data) as? [String: Any] { self.handle(msg) }
                    self.receive(t)
                case let .success(.data(data)):
                    if self.screen != nil, let img = UIImage(data: data) { self.frame = img }
                    self.receive(t)
                case let .failure(error):
                    self.lost(t, error)
                @unknown default:
                    self.receive(t)
                }
            }
        }
    }

    private func lost(_ t: URLSessionWebSocketTask?, _ error: Error?) {
        guard let t, t === task else { return }
        pinger?.invalidate()
        if case .connected = status {
            status = .failed("Connexion perdue")
        } else if status != .disconnected {
            status = .failed("AetherPlay injoignable (\(error?.localizedDescription ?? "erreur réseau"))")
        }
        game = nil
        screen = nil
        frame = nil
    }

    private func handle(_ msg: [String: Any]) {
        switch msg["t"] as? String {
        case "pin": status = .needPin
        case "welcome":
            if let t = msg["token"] as? String, !t.isEmpty { token = t }
            status = .connected(msg["server"] as? String ?? target?.name ?? "AetherPlay")
            lastSent = nil
        case "denied":
            token = nil
            status = (msg["retry"] as? Bool ?? false) ? .needPin : .failed(msg["reason"] as? String ?? "Refusé")
        case "game":
            let cover = (msg["cover"] as? String).flatMap { Data(base64Encoded: $0, options: .ignoreUnknownCharacters) }.flatMap(UIImage.init(data:))
            game = Game(platform: msg["platform"] as? String ?? "", name: msg["name"] as? String ?? "", cover: cover)
        case "idle": game = nil
        case "virtual": wifiInput = msg["ok"] as? Bool ?? false
        case "player": player = msg["n"] as? Int ?? 0
        case "screen":
            if msg["on"] as? Bool ?? false {
                screen = CGSize(width: msg["w"] as? Int ?? 256, height: msg["h"] as? Int ?? 192)
            } else {
                screen = nil
                frame = nil
            }
        default: break
        }
    }

    private func send(_ msg: [String: Any]) {
        guard let task, let data = try? JSONSerialization.data(withJSONObject: msg),
              let text = String(data: data, encoding: .utf8) else { return }
        task.send(.string(text)) { _ in }
    }

    func sendPin(_ pin: String) { send(["t": "hello", "v": 1, "name": name, "pin": pin]) }

    func sendPad(_ state: PadState) {
        guard case .connected = status, state != lastSent else { return }
        lastSent = state
        send(["t": "pad", "b": state.buttons, "bt": false, "a": state.axes.map { Double(Int($0 * 100)) / 100 }])
    }

    func sendTouch(x: CGFloat, y: CGFloat, down: Bool) {
        send(["t": "touch", "x": Double(x), "y": Double(y), "down": down])
    }

    func quitGame() { send(["t": "quit"]) }

    func disconnect() {
        pinger?.invalidate()
        task?.cancel(with: .normalClosure, reason: nil)
        task = nil
        game = nil
        screen = nil
        frame = nil
        player = 0
        status = .disconnected
    }
}

