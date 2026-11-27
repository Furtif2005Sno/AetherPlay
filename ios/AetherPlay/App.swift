import Combine
import SwiftUI
import UIKit

@main
struct AetherPlayApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView(model: model)
                .preferredColorScheme(.dark)
                .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
        }
    }
}

/// App state: discovery, the Wi-Fi link, pairing tokens per computer.
final class AppModel: ObservableObject {
    let discovery = Discovery()
    let link = Link()
    private var bag: Set<AnyCancellable> = []
    private var current: Server?

    init() {
        // Views read link / discovery through the model.
        link.objectWillChange.sink { [weak self] in self?.objectWillChange.send() }.store(in: &bag)
        discovery.objectWillChange.sink { [weak self] in self?.objectWillChange.send() }.store(in: &bag)
        // Keep the token AetherPlay issues at pairing.
        link.$token.sink { [weak self] token in
            guard let self, let s = self.current else { return }
            UserDefaults.standard.set(token, forKey: "token:" + s.name)
        }.store(in: &bag)
        discovery.start()
    }

    var lastServer: Server? {
        guard let d = UserDefaults.standard.dictionary(forKey: "last"),
              let name = d["name"] as? String, let host = d["host"] as? String, let port = d["port"] as? Int else { return nil }
        return Server(name: name, host: host, port: port)
    }

    func connect(_ s: Server) {
        current = s
        UserDefaults.standard.set(["name": s.name, "host": s.host, "port": s.port], forKey: "last")
        link.connect(s, token: UserDefaults.standard.string(forKey: "token:" + s.name))
    }

    /// Typed address: "192.168.1.20" or "192.168.1.20:47820".
    func connect(address: String) {
        let parts = address.trimmingCharacters(in: .whitespaces).split(separator: ":")
        guard let host = parts.first, !host.isEmpty else { return }
        let port = parts.count > 1 ? Int(parts[1]) ?? 47820 : 47820
        connect(Server(name: String(host), host: String(host), port: port))
    }
}

struct RootView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        if case .connected = model.link.status {
            PadScreen(model: model)
        } else {
            ConnectView(model: model)
        }
    }
}
