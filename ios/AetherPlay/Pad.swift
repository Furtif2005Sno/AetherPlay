import SwiftUI

// Controller state and per-console layouts: same as the Android app
// (android/…/pad/PadState.kt, Layouts.kt), so both phones behave alike.

/// W3C "standard gamepad" order, as AetherPlay reads it. Face buttons are
/// positional: "south" is Xbox A, Sony Cross, Nintendo B.
enum Btn {
    static let south = 0, east = 1, west = 2, north = 3
    static let lb = 4, rb = 5, lt = 6, rt = 7
    static let select = 8, start = 9, l3 = 10, r3 = 11
    static let up = 12, down = 13, left = 14, right = 15, home = 16
    static let count = 17
}

/// Left X, left Y, right X, right Y in -1…1 (Y down positive).
struct PadState: Equatable {
    var buttons: UInt32 = 0
    var axes: [Float] = [0, 0, 0, 0]
}

extension Color {
    /// 0xAARRGGBB, like the Android colours.
    init(argb: UInt32) {
        self.init(
            .sRGB,
            red: Double((argb >> 16) & 0xFF) / 255,
            green: Double((argb >> 8) & 0xFF) / 255,
            blue: Double(argb & 0xFF) / 255,
            opacity: Double((argb >> 24) & 0xFF) / 255
        )
    }
}

enum Control: Equatable {
    /// `axis` >= 0: instead of a button, push that axis to `axisValue` (N64 C buttons).
    case button(id: Int, label: String, x: CGFloat, y: CGFloat, r: CGFloat, fill: UInt32, text: UInt32, pill: Bool, axis: Int, axisValue: Float)
    case dpad(x: CGFloat, y: CGFloat, r: CGFloat)
    /// `axis`: 0 for the left stick (axes 0/1), 2 for the right one.
    case stick(axis: Int, x: CGFloat, y: CGFloat, r: CGFloat, tint: UInt32)

    var center: CGPoint {
        switch self {
        case let .button(_, _, x, y, _, _, _, _, _, _): return CGPoint(x: x, y: y)
        case let .dpad(x, y, _): return CGPoint(x: x, y: y)
        case let .stick(_, x, y, _, _): return CGPoint(x: x, y: y)
        }
    }

    var radius: CGFloat {
        switch self {
        case let .button(_, _, _, _, r, _, _, pill, _, _): return pill ? r * 2.3 : r
        case let .dpad(_, _, r): return r
        case let .stick(_, _, _, r, _): return r
        }
    }

    /// Moved / resized copy (Personnaliser).
    func adjusted(dx: CGFloat, dy: CGFloat, scale: CGFloat) -> Control {
        let fx: (CGFloat) -> CGFloat = { min(0.98, max(0.02, $0 + dx)) }
        let fy: (CGFloat) -> CGFloat = { min(0.96, max(0.04, $0 + dy)) }
        switch self {
        case let .button(id, label, x, y, r, fill, text, pill, axis, value):
            return .button(id: id, label: label, x: fx(x), y: fy(y), r: r * scale, fill: fill, text: text, pill: pill, axis: axis, axisValue: value)
        case let .dpad(x, y, r):
            return .dpad(x: fx(x), y: fy(y), r: r * scale)
        case let .stick(axis, x, y, r, tint):
            return .stick(axis: axis, x: fx(x), y: fy(y), r: r * scale, tint: tint)
        }
    }
}

enum BodyShape { case none, dogbone, brick, trident, handles, joycons, handheld, panel }

/// Controller silhouette: shape, main body, inner panel / screen, accent.
struct Body: Equatable {
    var shape: BodyShape
    var main: UInt32
    var second: UInt32
    var accent: UInt32
    init(_ shape: BodyShape, _ main: UInt32, _ second: UInt32? = nil, _ accent: UInt32? = nil) {
        self.shape = shape
        self.main = main
        self.second = second ?? main
        self.accent = accent ?? main
    }
}

/// Where the game's second screen is shown (fractions of the phone screen).
struct ScreenArea: Equatable { var x0, y0, x1, y1: CGFloat }

struct PadLayout: Equatable {
    var title: String
    var background: UInt32
    var accent: UInt32
    var controls: [Control]
    var body = Body(.none, 0)
    var screen: ScreenArea?
}

// MARK: - Building blocks

private let dark: UInt32 = 0xFF2C2C36
private let white: UInt32 = 0xFFFFFFFF

private func button(_ id: Int, _ label: String, _ x: CGFloat, _ y: CGFloat, _ r: CGFloat = 0.075, _ fill: UInt32 = dark, _ text: UInt32 = white) -> Control {
    .button(id: id, label: label, x: x, y: y, r: r, fill: fill, text: text, pill: false, axis: -1, axisValue: 0)
}

private func cButton(_ label: String, _ x: CGFloat, _ y: CGFloat, axis: Int, value: Float) -> Control {
    .button(id: -1, label: label, x: x, y: y, r: 0.055, fill: 0xFFE2B93B, text: 0xFF1E2028, pill: false, axis: axis, axisValue: value)
}

private func pill(_ id: Int, _ label: String, _ x: CGFloat, _ y: CGFloat, _ fill: UInt32 = dark) -> Control {
    .button(id: id, label: label, x: x, y: y, r: 0.05, fill: fill, text: white, pill: true, axis: -1, axisValue: 0)
}

private struct Face { var labels: [String]; var fills: [UInt32]; var texts: [UInt32] }

private let xbox = Face(labels: ["A", "B", "X", "Y"], fills: [0xFF2E9D4F, 0xFFD64545, 0xFF2F7FE0, 0xFFE0B21F], texts: Array(repeating: white, count: 4))
private let nintendo = Face(labels: ["B", "A", "Y", "X"], fills: Array(repeating: dark, count: 4), texts: Array(repeating: white, count: 4))
private let snes = Face(labels: ["B", "A", "Y", "X"], fills: [0xFFE2B93B, 0xFFCF4A45, 0xFF3F9E4D, 0xFF3D63C9], texts: Array(repeating: white, count: 4))
private let sony = Face(labels: ["✕", "○", "□", "△"], fills: Array(repeating: dark, count: 4), texts: [0xFF8FA8F5, 0xFFF0707A, 0xFFE68FD0, 0xFF52D3A8])
private let dreamcast = Face(labels: ["A", "B", "X", "Y"], fills: [0xFFD9443F, 0xFF3C6FD1, 0xFFE8B930, 0xFF3CA55C], texts: Array(repeating: white, count: 4))

private func face(_ f: Face, x: CGFloat = 0.85, y: CGFloat = 0.44, gap: CGFloat = 0.085, r: CGFloat = 0.072) -> [Control] {
    [
        button(Btn.south, f.labels[0], x, y + gap * 1.6, r, f.fills[0], f.texts[0]),
        button(Btn.east, f.labels[1], x + gap, y, r, f.fills[1], f.texts[1]),
        button(Btn.west, f.labels[2], x - gap, y, r, f.fills[2], f.texts[2]),
        button(Btn.north, f.labels[3], x, y - gap * 1.6, r, f.fills[3], f.texts[3]),
    ]
}

private func shoulders(_ lb: String?, _ rb: String?, _ lt: String? = nil, _ rt: String? = nil) -> [Control] {
    var out: [Control] = []
    if let lt { out.append(pill(Btn.lt, lt, 0.07, 0.09)) }
    if let lb { out.append(pill(Btn.lb, lb, lt != nil ? 0.2 : 0.09, 0.09)) }
    if let rb { out.append(pill(Btn.rb, rb, rt != nil ? 0.8 : 0.91, 0.09)) }
    if let rt { out.append(pill(Btn.rt, rt, 0.93, 0.09)) }
    return out
}

private func center(_ select: String?, _ start: String?, _ home: String? = nil) -> [Control] {
    var out: [Control] = []
    if let select { out.append(pill(Btn.select, select, 0.4, 0.9)) }
    if let home { out.append(button(Btn.home, home, 0.5, 0.9, 0.045)) }
    if let start { out.append(pill(Btn.start, start, 0.6, 0.9)) }
    return out
}

private func sticksHigh(_ tint: UInt32 = 0xFF3A3A46) -> [Control] {
    [.stick(axis: 0, x: 0.15, y: 0.42, r: 0.16, tint: tint), .dpad(x: 0.31, y: 0.78, r: 0.14), .stick(axis: 2, x: 0.68, y: 0.78, r: 0.14, tint: tint)]
}

private func sticksLow() -> [Control] {
    [.dpad(x: 0.15, y: 0.42, r: 0.17), .stick(axis: 0, x: 0.33, y: 0.78, r: 0.14, tint: 0xFF3A3A46), .stick(axis: 2, x: 0.67, y: 0.78, r: 0.14, tint: 0xFF3A3A46)]
}

private let defaultDpad = Control.dpad(x: 0.17, y: 0.58, r: 0.17)

private func xboxLike(_ title: String, bg: UInt32 = 0xFF15181D, accent: UInt32 = 0xFF2E9D4F, body: UInt32 = 0xFF2A2D33) -> PadLayout {
    PadLayout(title: title, background: bg, accent: accent,
              controls: sticksHigh() + face(xbox) + shoulders("LB", "RB", "LT", "RT") + center("◧", "≡", "⌂"),
              body: Body(.handles, body, 0xFF1E2024))
}

private func sonyLayout(_ title: String, home: Bool, bg: UInt32 = 0xFF1B1F2B, body: UInt32 = 0xFF1E1F26, panel: UInt32 = 0xFF15161B) -> PadLayout {
    PadLayout(title: title, background: bg, accent: 0xFF8FA8F5,
              controls: sticksLow() + face(sony) + shoulders("L1", "R1", "L2", "R2") + center("SELECT", "START", home ? "PS" : nil),
              body: Body(.handles, body, panel))
}

private func twoButtons(_ title: String, _ bg: UInt32, _ accent: UInt32, _ b: String, _ a: String, _ fill: UInt32,
                        l: String? = nil, r: String? = nil, body: Body = Body(.none, 0)) -> PadLayout {
    PadLayout(title: title, background: bg, accent: accent,
              controls: [defaultDpad, button(Btn.south, b, 0.76, 0.64, 0.085, fill), button(Btn.east, a, 0.9, 0.52, 0.085, fill)]
                  + shoulders(l, r) + center("SELECT", "START"),
              body: body)
}

/// Controller for the AetherPlay menus: A confirms, B goes back.
let menuLayout = PadLayout(
    title: "AetherPlay", background: 0xFF0D0B14, accent: 0xFF8B5CF6,
    controls: [defaultDpad, .stick(axis: 2, x: 0.68, y: 0.78, r: 0.12, tint: 0xFF3A3A46)] + face(xbox) + shoulders("LB", "RB") + center("◧", "≡")
)

/// Layout for `platform`; with `screen`, room in the middle for the game's
/// second screen (DS / 3DS bottom screen, Wii U GamePad).
func layoutFor(_ platform: String?, screen: Bool = false) -> PadLayout {
    var base = baseLayout(platform)
    guard screen else { return base }
    switch platform {
    case "nds", "3ds", "n3ds":
        base.screen = ScreenArea(x0: 0.27, y0: 0.12, x1: 0.72, y1: 0.84)
    case "wiiu":
        base.controls = [.stick(axis: 0, x: 0.1, y: 0.36, r: 0.13, tint: 0xFF3A3A46), .dpad(x: 0.1, y: 0.76, r: 0.12),
                         .stick(axis: 2, x: 0.9, y: 0.78, r: 0.11, tint: 0xFF3A3A46)]
            + face(nintendo, x: 0.88, y: 0.36, gap: 0.065, r: 0.065) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂")
        base.screen = ScreenArea(x0: 0.2, y0: 0.14, x1: 0.77, y1: 0.84)
    default: break
    }
    return base
}

private func baseLayout(_ platform: String?) -> PadLayout {
    switch platform ?? "" {
    case "": return menuLayout
    case "nes", "famicom":
        return twoButtons("NES", 0xFF26262B, 0xFFD13B3B, "B", "A", 0xFFC83737, body: Body(.brick, 0xFFBDBDC2, 0xFF1E1E21, 0xFFC83737))
    case "snes", "sfam":
        return PadLayout(title: "Super Nintendo", background: 0xFF2A2735, accent: 0xFF7B68D9,
                         controls: [defaultDpad] + face(snes) + shoulders("L", "R") + center("SELECT", "START"),
                         body: Body(.dogbone, 0xFFCFCDD6, 0xFFB4B0C2, 0xFF9C94BE))
    case "n64":
        return PadLayout(title: "Nintendo 64", background: 0xFF1E2028, accent: 0xFF2F6FDB,
                         controls: [.dpad(x: 0.1, y: 0.34, r: 0.11), .stick(axis: 0, x: 0.25, y: 0.66, r: 0.16, tint: 0xFF3A3A46),
                                    button(Btn.south, "A", 0.8, 0.72, 0.09, 0xFF2F6FDB), button(Btn.west, "B", 0.69, 0.58, 0.075, 0xFF2E9D4F),
                                    cButton("▲", 0.87, 0.26, axis: 3, value: -1), cButton("▼", 0.87, 0.48, axis: 3, value: 1),
                                    cButton("◀", 0.8, 0.37, axis: 2, value: -1), cButton("▶", 0.94, 0.37, axis: 2, value: 1),
                                    pill(Btn.lt, "Z", 0.36, 0.3)] + shoulders("L", "R") + center(nil, "START"),
                         body: Body(.trident, 0xFF8E8E97, 0xFF6C6C75))
    case "gb": return twoButtons("Game Boy", 0xFF2B2E26, 0xFF9BBC0F, "B", "A", 0xFF8E2A5C, body: Body(.handheld, 0xFFC9C5BA, 0xFF8BAC0F))
    case "gbc": return twoButtons("Game Boy Color", 0xFF2A2338, 0xFF8E5CD9, "B", "A", 0xFF8E2A5C, body: Body(.handheld, 0xFF6B4FA0, 0xFF9EB8A0))
    case "gba": return twoButtons("Game Boy Advance", 0xFF27234A, 0xFF5B54C9, "B", "A", 0xFF3A3480, l: "L", r: "R", body: Body(.handheld, 0xFF4B44A8, 0xFF9AA6C0))
    case "nds":
        return PadLayout(title: "Nintendo DS", background: 0xFF22242A, accent: 0xFFB8BCC6,
                         controls: [defaultDpad] + face(nintendo) + shoulders("L", "R") + center("SELECT", "START"),
                         body: Body(.handheld, 0xFF3A3D45, 0xFF5B6270))
    case "3ds", "n3ds":
        return PadLayout(title: "Nintendo 3DS", background: 0xFF2A1F22, accent: 0xFFD6404B,
                         controls: [.stick(axis: 0, x: 0.15, y: 0.4, r: 0.14, tint: 0xFF3A3A46), .dpad(x: 0.17, y: 0.78, r: 0.13)]
                             + face(nintendo) + shoulders("L", "R", "ZL", "ZR") + center("SELECT", "START", "⌂"),
                         body: Body(.handheld, 0xFF8E1F2A, 0xFF5B6270))
    case "switch":
        return PadLayout(title: "Nintendo Switch", background: 0xFF1A1A20, accent: 0xFFFF4554,
                         controls: sticksHigh() + face(nintendo) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂"),
                         body: Body(.joycons, 0xFF00A8D6, 0xFFE8424F, 0xFF2B2B33))
    case "switch-2":
        return PadLayout(title: "Nintendo Switch 2", background: 0xFF16181F, accent: 0xFF4FA3FF,
                         controls: sticksHigh() + face(nintendo) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂"),
                         body: Body(.joycons, 0xFF2E3139, 0xFF2E3139, 0xFF4FA3FF))
    case "wiiu":
        return PadLayout(title: "Wii U", background: 0xFF1B2228, accent: 0xFF1FA7D9,
                         controls: sticksHigh() + face(nintendo) + shoulders("L", "R", "ZL", "ZR") + center("−", "+", "⌂"),
                         body: Body(.handheld, 0xFFE4E6E8, 0xFF3A4652))
    case "ngc", "wii":
        return PadLayout(title: platform == "ngc" ? "GameCube" : "Wii", background: 0xFF2E2655, accent: 0xFF7E6BE0,
                         controls: [.stick(axis: 0, x: 0.15, y: 0.42, r: 0.16, tint: 0xFF3A3A46), .dpad(x: 0.3, y: 0.8, r: 0.12),
                                    .stick(axis: 2, x: 0.68, y: 0.8, r: 0.12, tint: 0xFFB89A1E),
                                    button(Btn.south, "A", 0.83, 0.52, 0.1, 0xFF2E9D4F), button(Btn.west, "B", 0.71, 0.62, 0.065, 0xFFD64545),
                                    button(Btn.east, "X", 0.92, 0.42, 0.065, 0xFF8A8A96), button(Btn.north, "Y", 0.8, 0.3, 0.065, 0xFF8A8A96)]
                             + shoulders(nil, "Z", "L", "R") + center(nil, "START"),
                         body: Body(.handles, 0xFF4B3F8F, 0xFF3A3070))
    case "genesis-slash-megadrive":
        return PadLayout(title: "Mega Drive", background: 0xFF3B3B42, accent: 0xFFD32F2F,
                         controls: [defaultDpad, button(Btn.west, "A", 0.7, 0.66, 0.07), button(Btn.south, "B", 0.81, 0.6, 0.07),
                                    button(Btn.east, "C", 0.92, 0.54, 0.07)] + center(nil, "START"),
                         body: Body(.dogbone, 0xFF141416, 0xFF242428, 0xFF242428))
    case "sms": return twoButtons("Master System", 0xFF181818, 0xFFD32F2F, "1", "2", dark, body: Body(.brick, 0xFF1F1F22, 0xFF151517, 0xFFD32F2F))
    case "gamegear": return twoButtons("Game Gear", 0xFF181818, 0xFFD32F2F, "1", "2", dark, body: Body(.handheld, 0xFF1F1F22, 0xFF3A4652))
    case "turbografx16--1": return twoButtons("PC Engine", 0xFF1C1C22, 0xFFE07B1F, "II", "I", dark, body: Body(.brick, 0xFFE2E2E4, 0xFF26262A, 0xFFE07B1F))
    case "saturn":
        return PadLayout(title: "Saturn", background: 0xFF24242C, accent: 0xFF9AA0B5,
                         controls: [defaultDpad, button(Btn.south, "A", 0.7, 0.66, 0.065), button(Btn.east, "B", 0.81, 0.6, 0.065),
                                    button(Btn.rb, "C", 0.92, 0.54, 0.065), button(Btn.west, "X", 0.7, 0.46, 0.055),
                                    button(Btn.north, "Y", 0.81, 0.4, 0.055), button(Btn.lb, "Z", 0.92, 0.34, 0.055)]
                             + shoulders(nil, nil, "L", "R") + center(nil, "START"),
                         body: Body(.dogbone, 0xFF3A3A44, 0xFF2C2C34, 0xFF4A4A56))
    case "dc":
        return PadLayout(title: "Dreamcast", background: 0xFF1F2830, accent: 0xFFF57C00,
                         controls: Array(sticksHigh().prefix(2)) + face(dreamcast) + shoulders(nil, nil, "L", "R") + center(nil, "START"),
                         body: Body(.handles, 0xFFE6E6E8, 0xFF3A4652))
    case "neogeoaes", "arcade":
        return PadLayout(title: platform == "arcade" ? "Arcade" : "Neo Geo", background: 0xFF141414, accent: 0xFFE53935,
                         controls: [.stick(axis: 0, x: 0.2, y: 0.58, r: 0.18, tint: 0xFF3A3A46),
                                    button(Btn.south, "A", 0.64, 0.68, 0.075, 0xFFD64545), button(Btn.east, "B", 0.75, 0.6, 0.075, 0xFFE0B21F),
                                    button(Btn.west, "C", 0.84, 0.5, 0.075, 0xFF2E9D4F), button(Btn.north, "D", 0.91, 0.39, 0.075, 0xFF2F7FE0)]
                             + center("COIN", "START"),
                         body: Body(.panel, 0xFF1A1A1D, 0xFF101012, 0xFFE53935))
    case "atari2600":
        return PadLayout(title: "Atari 2600", background: 0xFF2E2118, accent: 0xFFE0662F,
                         controls: [.stick(axis: 0, x: 0.2, y: 0.58, r: 0.18, tint: 0xFF3A3A46), button(Btn.south, "FIRE", 0.82, 0.58, 0.11, 0xFFC62828)]
                             + center("SELECT", "RESET"),
                         body: Body(.panel, 0xFF5A3A24, 0xFF1A1512, 0xFFE0662F))
    case "psx": return sonyLayout("PlayStation", home: false, body: 0xFFBDBDC3, panel: 0xFFA4A4AC)
    case "ps2": return sonyLayout("PlayStation 2", home: false)
    case "ps3": return sonyLayout("PlayStation 3", home: true)
    case "ps4--1": return sonyLayout("PlayStation 4", home: true, bg: 0xFF141B2B)
    case "psvita":
        return PadLayout(title: "PS Vita", background: 0xFF181C26, accent: 0xFF8FA8F5,
                         controls: sticksLow() + face(sony) + shoulders("L", "R") + center("SELECT", "START", "PS"),
                         body: Body(.handheld, 0xFF1E1F26, 0xFF3A4652))
    case "psp":
        return PadLayout(title: "PSP", background: 0xFF1A1A1F, accent: 0xFF8FA8F5,
                         controls: [.dpad(x: 0.15, y: 0.42, r: 0.17), .stick(axis: 0, x: 0.16, y: 0.82, r: 0.11, tint: 0xFF3A3A46)]
                             + face(sony) + shoulders("L", "R") + center("SELECT", "START", "⌂"),
                         body: Body(.handheld, 0xFF1E1F26, 0xFF3A4652))
    case "xbox": return xboxLike("Xbox", bg: 0xFF121A12)
    case "xbox360": return xboxLike("Xbox 360", bg: 0xFF15191D, body: 0xFFE4E5E8)
    default: return xboxLike("Manette")
    }
}

// MARK: - Personnaliser

/// A control moved (fractions of the screen) and resized by the player.
struct Tweak: Codable, Equatable {
    var dx: CGFloat = 0
    var dy: CGFloat = 0
    var scale: CGFloat = 1
}

extension PadLayout {
    func tweaked(_ tweaks: [Int: Tweak]) -> PadLayout {
        guard !tweaks.isEmpty else { return self }
        var out = self
        out.controls = controls.enumerated().map { i, c in
            guard let t = tweaks[i] else { return c }
            return c.adjusted(dx: t.dx, dy: t.dy, scale: t.scale)
        }
        return out
    }
}

/// Saved adjustments, one set per console layout.
enum TweakStore {
    static func load(_ layout: PadLayout) -> [Int: Tweak] {
        guard let data = UserDefaults.standard.data(forKey: "layout:" + layout.title),
              let map = try? JSONDecoder().decode([String: Tweak].self, from: data) else { return [:] }
        return Dictionary(uniqueKeysWithValues: map.compactMap { k, v in Int(k).map { ($0, v) } })
    }

    static func save(_ layout: PadLayout, _ tweaks: [Int: Tweak]) {
        if tweaks.isEmpty {
            UserDefaults.standard.removeObject(forKey: "layout:" + layout.title)
            return
        }
        let map = Dictionary(uniqueKeysWithValues: tweaks.map { (String($0.key), $0.value) })
        if let data = try? JSONEncoder().encode(map) { UserDefaults.standard.set(data, forKey: "layout:" + layout.title) }
    }
}
