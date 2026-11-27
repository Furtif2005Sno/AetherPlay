import SwiftUI

/// Finding AetherPlay and pairing (code shown in Réglages > AetherPlay).
struct ConnectView: View {
    @ObservedObject var model: AppModel
    @State private var pin = ""
    @State private var address = ""
    @State private var triedLast = false

    private let accent = Color(argb: 0xFF8B5CF6)

    var body: some View {
        ZStack {
            Color(argb: 0xFF0D0B14).ignoresSafeArea()
            HStack(alignment: .top, spacing: 32) {
                VStack(alignment: .leading, spacing: 10) {
                    Image("Logo").resizable().scaledToFit().frame(width: 64, height: 64)
                        .clipShape(RoundedRectangle(cornerRadius: 14))
                    Text("AetherPlay").font(.system(size: 26, weight: .heavy))
                    Text("Manette pour AetherPlay. Le téléphone et l'ordinateur doivent être sur le même réseau Wi-Fi (ou l'ordinateur sur le partage de connexion de l'iPhone).")
                        .font(.system(size: 13)).foregroundColor(.white.opacity(0.7))
                    statusLine
                }
                .frame(maxWidth: 300)

                VStack(alignment: .leading, spacing: 12) {
                    if model.link.status == .needPin {
                        pinForm
                    } else {
                        serverList
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .padding(.horizontal, 48)
            .padding(.vertical, 24)
        }
        .foregroundColor(.white)
        .onAppear {
            // Reconnect to the last computer once.
            if !triedLast, let s = model.lastServer {
                triedLast = true
                model.connect(s)
            }
        }
    }

    @ViewBuilder private var statusLine: some View {
        switch model.link.status {
        case .connecting:
            HStack { ProgressView(); Text("Connexion…") }.font(.system(size: 13))
        case let .failed(reason):
            Text(reason).font(.system(size: 13)).foregroundColor(.orange)
        default:
            EmptyView()
        }
    }

    private var serverList: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Ordinateurs trouvés").font(.system(size: 15, weight: .bold))
            if model.discovery.servers.isEmpty {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Recherche d'AetherPlay sur le réseau…").font(.system(size: 13)).foregroundColor(.white.opacity(0.7))
                }
            }
            ScrollView {
                VStack(spacing: 8) {
                    ForEach(model.discovery.servers) { s in
                        Button { model.connect(s) } label: {
                            HStack {
                                Image(systemName: "desktopcomputer")
                                VStack(alignment: .leading) {
                                    Text(s.name).font(.system(size: 15, weight: .semibold))
                                    Text(s.host).font(.system(size: 11)).foregroundColor(.white.opacity(0.6))
                                }
                                Spacer()
                                Image(systemName: "chevron.right")
                            }
                            .padding(12)
                            .background(RoundedRectangle(cornerRadius: 12).fill(Color.white.opacity(0.08)))
                        }
                    }
                }
            }
            .frame(maxHeight: 160)
            Text("Ou par adresse").font(.system(size: 13, weight: .semibold))
            HStack {
                TextField("192.168.1.20", text: $address)
                    .keyboardType(.numbersAndPunctuation).autocorrectionDisabled().textInputAutocapitalization(.never)
                    .padding(10).background(RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.08)))
                Button("Se connecter") { model.connect(address: address) }
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    .background(RoundedRectangle(cornerRadius: 10).fill(accent))
                    .disabled(address.isEmpty)
            }
        }
    }

    private var pinForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Code d'appairage").font(.system(size: 15, weight: .bold))
            Text("Tape le code à 6 chiffres affiché dans AetherPlay, Réglages > AetherPlay.")
                .font(.system(size: 13)).foregroundColor(.white.opacity(0.7))
            HStack {
                TextField("123456", text: $pin)
                    .keyboardType(.numberPad).font(.system(size: 24, weight: .bold, design: .monospaced))
                    .padding(10).background(RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.08)))
                    .onChange(of: pin) { v in
                        let digits = String(v.filter(\.isNumber).prefix(6))
                        if digits != v { pin = digits }
                    }
                Button("Valider") {
                    model.link.sendPin(pin)
                    pin = ""
                }
                .padding(.horizontal, 14).padding(.vertical, 12)
                .background(RoundedRectangle(cornerRadius: 10).fill(accent))
                .disabled(pin.count != 6)
            }
            Button("Annuler") { model.link.disconnect() }.foregroundColor(.white.opacity(0.7))
        }
    }
}
