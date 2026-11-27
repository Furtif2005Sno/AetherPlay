import SwiftUI
import UIKit

// Controller screen: the console silhouette (or the cover), the controls,
// and every finger turned into a controller state. Port of PadScreen.kt.

enum Backdrop: String { case controller, cover }

/// How buttons are drawn (Personnaliser > Style): the console's colours by default.
enum ButtonStyle: String, CaseIterable, Codable {
    case original, contrast, outline
    var label: String {
        switch self {
        case .original: return "Origine"
        case .contrast: return "Contraste élevé"
        case .outline: return "Contour"
        }
    }
}

private let contrastFill = Color(argb: 0xFFF2F2F5)
private let contrastText = Color(argb: 0xFF111114)

/// What the fingers currently press, for drawing.
final class PadTouches: ObservableObject {
    @Published var pressed: UInt32 = 0
    @Published var axisDown: Set<Int> = []
    @Published var sticks: [Int: CGPoint] = [:]
}

// MARK: - Geometry (shared by drawing and touch)

private func controlPoint(_ c: Control, _ size: CGSize) -> CGPoint {
    CGPoint(x: c.center.x * size.width, y: c.center.y * size.height)
}

func screenRect(_ area: ScreenArea, _ image: CGSize, _ size: CGSize) -> CGRect {
    let aw = (area.x1 - area.x0) * size.width
    let ah = (area.y1 - area.y0) * size.height
    let scale = min(aw / image.width, ah / image.height)
    let iw = image.width * scale, ih = image.height * scale
    return CGRect(x: area.x0 * size.width + (aw - iw) / 2, y: area.y0 * size.height + (ah - ih) / 2, width: iw, height: ih)
}

private func hitButton(_ c: Control, _ p: CGPoint, _ size: CGSize) -> Bool {
    guard case let .button(_, _, x, y, r0, _, _, pill, _, _) = c else { return false }
    let cx = x * size.width, cy = y * size.height, r = r0 * size.height
    if pill { return abs(p.x - cx) <= r * 2.4 && abs(p.y - cy) <= r * 1.5 }
    return hypot(p.x - cx, p.y - cy) <= r * 1.3
}

private func dpadBits(_ c: Control, _ p: CGPoint, _ size: CGSize) -> UInt32 {
    guard case let .dpad(x, y, r0) = c else { return 0 }
    let r = r0 * size.height
    let dx = p.x - x * size.width, dy = p.y - y * size.height
    if hypot(dx, dy) > r * 1.3 { return 0 }
    let dead = r * 0.28
    var bits: UInt32 = 0
    if dy < -dead { bits |= 1 << Btn.up }
    if dy > dead { bits |= 1 << Btn.down }
    if dx < -dead { bits |= 1 << Btn.left }
    if dx > dead { bits |= 1 << Btn.right }
    return bits
}

/// Nearest control under `p` (edit mode).
func controlAt(_ layout: PadLayout, _ p: CGPoint, _ size: CGSize) -> Int? {
    let scored = layout.controls.enumerated().map { i, c in (i, c, hypot(p.x - controlPoint(c, size).x, p.y - controlPoint(c, size).y)) }
    guard let best = scored.min(by: { $0.2 < $1.2 }) else { return nil }
    return best.2 <= best.1.radius * size.height * 1.4 ? best.0 : nil
}

// MARK: - Multitouch

/// UIKit view that reads every finger (SwiftUI gestures only see one).
final class TouchSurface: UIView {
    var layout = menuLayout
    var screenImage: CGSize?
    var onState: (PadState) -> Void = { _ in }
    var onTouch: (CGFloat, CGFloat, Bool) -> Void = { _, _, _ in }
    var touches: PadTouches?

    private var owners: [ObjectIdentifier: Int] = [:]   // touch -> stick axis
    private var stylus: ObjectIdentifier?
    private var active: Set<UITouch> = []
    private let haptic = UIImpactFeedbackGenerator(style: .light)

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) { fatalError() }

    private var imageRect: CGRect? {
        guard let area = layout.screen, let img = screenImage else { return nil }
        return screenRect(area, img, bounds.size)
    }

    override func touchesBegan(_ set: Set<UITouch>, with event: UIEvent?) {
        let size = bounds.size
        for t in set {
            active.insert(t)
            let p = t.location(in: self)
            if stylus == nil, let r = imageRect, r.contains(p) {
                stylus = ObjectIdentifier(t)
                continue
            }
            for c in layout.controls {
                if case let .stick(axis, _, _, r, _) = c, hypot(p.x - controlPoint(c, size).x, p.y - controlPoint(c, size).y) <= r * size.height * 1.35 {
                    owners[ObjectIdentifier(t)] = axis
                    break
                }
            }
        }
        update(set, ended: false)
    }

    override func touchesMoved(_ set: Set<UITouch>, with event: UIEvent?) { update(set, ended: false) }

    override func touchesEnded(_ set: Set<UITouch>, with event: UIEvent?) {
        update(set, ended: true)
        for t in set {
            active.remove(t)
            owners.removeValue(forKey: ObjectIdentifier(t))
            if stylus == ObjectIdentifier(t) { stylus = nil }
        }
        recompute()
    }

    override func touchesCancelled(_ set: Set<UITouch>, with event: UIEvent?) { touchesEnded(set, with: event) }

    private func update(_ changed: Set<UITouch>, ended: Bool) {
        if let id = stylus, let t = changed.first(where: { ObjectIdentifier($0) == id }), let r = imageRect {
            let p = t.location(in: self)
            onTouch(min(1, max(0, (p.x - r.minX) / r.width)), min(1, max(0, (p.y - r.minY) / r.height)), !ended)
        }
        if !ended { recompute() }
    }

    func reset() {
        active.removeAll()
        owners.removeAll()
        stylus = nil
        recompute()
    }

    private func recompute() {
        let size = bounds.size
        guard size.width > 0 else { return }
        var buttons: UInt32 = 0
        var axes: [Float] = [0, 0, 0, 0]
        var sticks: [Int: CGPoint] = [:]
        var axisHits: Set<Int> = []
        for t in active {
            let id = ObjectIdentifier(t)
            if id == stylus { continue }
            let p = t.location(in: self)
            if let axis = owners[id], let c = layout.controls.first(where: { if case let .stick(a, _, _, _, _) = $0 { return a == axis }; return false }) {
                let center = controlPoint(c, size)
                let radius = c.radius * size.height
                var d = CGPoint(x: (p.x - center.x) / radius, y: (p.y - center.y) / radius)
                let len = hypot(d.x, d.y)
                if len > 1 { d = CGPoint(x: d.x / len, y: d.y / len) }
                axes[axis] = Float(d.x)
                axes[axis + 1] = Float(d.y)
                sticks[axis] = d
                continue
            }
            for (i, c) in layout.controls.enumerated() {
                switch c {
                case let .button(bid, _, _, _, _, _, _, _, axis, value):
                    if hitButton(c, p, size) {
                        if axis >= 0 {
                            axes[axis] = value
                            axisHits.insert(i)
                        } else if bid >= 0 {
                            buttons |= 1 << UInt32(bid)
                        }
                    }
                case .dpad: buttons |= dpadBits(c, p, size)
                case .stick: break
                }
            }
        }
        if let touches {
            if buttons & ~touches.pressed != 0 || !axisHits.subtracting(touches.axisDown).isEmpty { haptic.impactOccurred() }
            if touches.pressed != buttons { touches.pressed = buttons }
            if touches.axisDown != axisHits { touches.axisDown = axisHits }
            if touches.sticks != sticks { touches.sticks = sticks }
        }
        onState(PadState(buttons: buttons, axes: axes))
    }
}

struct TouchLayer: UIViewRepresentable {
    var layout: PadLayout
    var screenImage: CGSize?
    var touches: PadTouches
    var onState: (PadState) -> Void
    var onTouch: (CGFloat, CGFloat, Bool) -> Void

    func makeUIView(context: Context) -> TouchSurface {
        let v = TouchSurface()
        v.touches = touches
        return v
    }

    func updateUIView(_ v: TouchSurface, context: Context) {
        let changed = v.layout != layout
        v.layout = layout
        v.screenImage = screenImage
        v.onState = onState
        v.onTouch = onTouch
        // After this view update: reset() publishes the cleared touches.
        if changed { DispatchQueue.main.async { v.reset() } }
    }
}

// MARK: - Drawing

private func lerpWhite(_ argb: UInt32) -> Color {
    func ch(_ s: UInt32) -> Double { let v = Double((argb >> s) & 0xFF) / 255; return v + (1 - v) * 0.35 }
    return Color(.sRGB, red: ch(16), green: ch(8), blue: ch(0), opacity: 1)
}

private func label(_ ctx: inout GraphicsContext, _ text: String, _ at: CGPoint, _ size: CGFloat, _ color: Color) {
    ctx.draw(Text(text).font(.system(size: max(6, size), weight: .bold)).foregroundColor(color), at: at)
}

private func circle(_ c: CGPoint, _ r: CGFloat) -> Path { Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r)) }

func drawControls(_ ctx: inout GraphicsContext, _ size: CGSize, _ layout: PadLayout, _ t: PadTouches, _ global: ButtonStyle,
                  _ styles: [Int: ButtonStyle] = [:]) {
    let accent = Color(argb: layout.accent)
    for (i, c) in layout.controls.enumerated() {
        let style = styles[i] ?? global
        let pc = controlPoint(c, size)
        switch c {
        case let .button(id, text, _, _, r0, fill, textColor, pill, axis, _):
            let r = r0 * size.height
            let down = axis >= 0 ? t.axisDown.contains(i) : (id >= 0 && t.pressed & (1 << UInt32(id)) != 0)
            let base: Color
            let baseDown: Color
            switch style {
            case .original: base = Color(argb: fill); baseDown = lerpWhite(fill)
            case .contrast: base = contrastFill; baseDown = accent
            case .outline: base = Color.black.opacity(0.25); baseDown = Color.white.opacity(0.45)
            }
            let fg: Color = style == .original ? Color(argb: textColor) : (style == .contrast && !down ? contrastText : .white)
            let shape: Path
            if pill {
                let rect = CGRect(x: pc.x - r * 2.2, y: pc.y - r * 1.1, width: r * 4.4, height: r * 2.2)
                shape = Path(roundedRect: rect, cornerRadius: r * 1.1)
            } else {
                ctx.fill(circle(CGPoint(x: pc.x, y: pc.y + r * 0.08), r), with: .color(.black.opacity(0.35)))
                shape = circle(pc, r)
            }
            ctx.fill(shape, with: .color(down ? baseDown : base))
            if style == .outline { ctx.stroke(shape, with: .color(.white), lineWidth: 2.5) }
            if down { ctx.stroke(shape, with: .color(accent), lineWidth: 2.5) }
            label(&ctx, text, pc, r * (pill ? 0.8 : 0.9), fg)

        case let .dpad(_, _, r0):
            let r = r0 * size.height
            let arm = r * 0.36
            let corner = arm * 0.4
            let base: Color = style == .original ? Color(argb: 0xFF16161B) : (style == .contrast ? contrastFill : Color.black.opacity(0.25))
            ctx.fill(circle(pc, r * 1.12), with: .color(.black.opacity(0.18)))
            ctx.stroke(circle(pc, r * 1.12), with: .color(.white.opacity(0.08)), lineWidth: 1)
            let vertical = Path(roundedRect: CGRect(x: pc.x - arm, y: pc.y - r, width: arm * 2, height: r * 2), cornerRadius: corner)
            let horizontal = Path(roundedRect: CGRect(x: pc.x - r, y: pc.y - arm, width: r * 2, height: arm * 2), cornerRadius: corner)
            ctx.fill(vertical, with: .color(base))
            ctx.fill(horizontal, with: .color(base))
            // Always outlined, so the cross reads on dark backgrounds too.
            if style != .contrast {
                let edge: Color = style == .original ? .white.opacity(0.35) : .white
                ctx.stroke(vertical, with: .color(edge), lineWidth: style == .outline ? 2 : 1.3)
                ctx.stroke(horizontal, with: .color(edge), lineWidth: style == .outline ? 2 : 1.3)
            }
            let arrow: Color = style == .contrast ? contrastText : .white.opacity(0.7)
            let a = arm * 0.45, tip = r * 0.82
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] as [(CGFloat, CGFloat)] {
                var p = Path()
                p.move(to: CGPoint(x: pc.x + dx * tip, y: pc.y + dy * tip))
                let back = CGPoint(x: pc.x + dx * (tip - a), y: pc.y + dy * (tip - a))
                p.addLine(to: CGPoint(x: back.x + dy * a * 0.8, y: back.y + dx * a * 0.8))
                p.addLine(to: CGPoint(x: back.x - dy * a * 0.8, y: back.y - dx * a * 0.8))
                p.closeSubpath()
                ctx.fill(p, with: .color(arrow))
            }
            let hl = accent.opacity(0.85)
            let parts: [(Int, CGRect)] = [
                (Btn.up, CGRect(x: pc.x - arm, y: pc.y - r, width: arm * 2, height: r - arm)),
                (Btn.down, CGRect(x: pc.x - arm, y: pc.y + arm, width: arm * 2, height: r - arm)),
                (Btn.left, CGRect(x: pc.x - r, y: pc.y - arm, width: r - arm, height: arm * 2)),
                (Btn.right, CGRect(x: pc.x + arm, y: pc.y - arm, width: r - arm, height: arm * 2)),
            ]
            for (bit, rect) in parts where t.pressed & (1 << UInt32(bit)) != 0 {
                ctx.fill(Path(roundedRect: rect, cornerRadius: corner), with: .color(hl))
            }

        case let .stick(axis, _, _, r0, tint):
            let r = r0 * size.height
            let value = t.sticks[axis] ?? .zero
            ctx.fill(circle(pc, r), with: .color(.black.opacity(0.3)))
            ctx.stroke(circle(pc, r), with: .color(.white.opacity(style == .original ? 0.12 : 0.8)), lineWidth: style == .original ? 1.5 : 2)
            let knob = CGPoint(x: pc.x + value.x * r * 0.6, y: pc.y + value.y * r * 0.6)
            let activeKnob = value != .zero
            let fill: Color
            switch style {
            case .original: fill = activeKnob ? lerpWhite(tint) : Color(argb: tint)
            case .contrast: fill = contrastFill
            case .outline: fill = Color.black.opacity(0.25)
            }
            ctx.fill(circle(knob, r * 0.48), with: .color(fill))
            if style == .outline { ctx.stroke(circle(knob, r * 0.48), with: .color(.white), lineWidth: 2) }
            if activeKnob { ctx.stroke(circle(knob, r * 0.48), with: .color(accent), lineWidth: 2) }
        }
    }
}

/// Stylised controller body behind the buttons (Bodies.kt).
func drawBody(_ ctx: inout GraphicsContext, _ size: CGSize, _ body: Body) {
    let w = size.width, h = size.height
    func rect(_ l: CGFloat, _ t: CGFloat, _ r: CGFloat, _ b: CGFloat, _ radius: CGFloat) -> Path {
        Path(roundedRect: CGRect(x: l * w, y: t * h, width: (r - l) * w, height: (b - t) * h), cornerRadius: radius * h)
    }
    func disc(_ x: CGFloat, _ y: CGFloat, _ r: CGFloat) -> Path { circle(CGPoint(x: x * w, y: y * h), r * h) }
    func union(_ parts: [Path]) -> Path {
        var p = Path()
        for part in parts { p.addPath(part) }
        return p
    }
    let main = Color(argb: body.main), second = Color(argb: body.second), accent = Color(argb: body.accent)
    func shape(_ p: Path, _ color: Color, rim: Bool = true) {
        ctx.fill(p.offsetBy(dx: 0, dy: 0.015 * h), with: .color(.black.opacity(0.35)))
        ctx.fill(p, with: .color(color))
        if rim { ctx.stroke(p, with: .color(.white.opacity(0.1)), lineWidth: 1.5) }
    }

    switch body.shape {
    case .none: break
    case .dogbone:
        shape(union([disc(0.2, 0.6, 0.38), disc(0.8, 0.6, 0.38), rect(0.2, 0.3, 0.8, 0.94, 0.2)]), main, rim: false)
        ctx.fill(rect(0.36, 0.4, 0.64, 0.8, 0.12), with: .color(second))
        ctx.fill(disc(0.84, 0.56, 0.3), with: .color(accent))
        ctx.fill(disc(0.17, 0.58, 0.24), with: .color(second.opacity(0.6)))
    case .brick:
        shape(rect(0.03, 0.2, 0.97, 0.98, 0.08), main)
        ctx.fill(rect(0.06, 0.3, 0.94, 0.93, 0.05), with: .color(second))
        for i in 0..<3 {
            ctx.fill(Path(CGRect(x: 0.42 * w, y: (0.42 + CGFloat(i) * 0.08) * h, width: 0.16 * w, height: 0.025 * h)), with: .color(accent))
        }
    case .trident:
        shape(union([rect(0.03, 0.18, 0.97, 0.6, 0.2), rect(0.03, 0.4, 0.16, 0.98, 0.07),
                     rect(0.18, 0.4, 0.33, 0.98, 0.07), rect(0.64, 0.4, 0.97, 0.98, 0.12)]), main, rim: false)
        ctx.fill(rect(0.4, 0.26, 0.6, 0.5, 0.1), with: .color(second))
    case .handles:
        shape(union([rect(0.05, 0.18, 0.95, 0.66, 0.24), disc(0.19, 0.7, 0.29), disc(0.81, 0.7, 0.29),
                     rect(0.28, 0.45, 0.72, 0.92, 0.18)]), main, rim: false)
        ctx.fill(rect(0.4, 0.26, 0.6, 0.56, 0.1), with: .color(second))
    case .joycons:
        ctx.fill(rect(0.38, 0.22, 0.62, 0.94, 0.04), with: .color(Color(argb: 0xFF15161A)))
        ctx.fill(rect(0.4, 0.27, 0.6, 0.89, 0.02), with: .color(Color(argb: 0xFF26282E)))
        shape(rect(0.02, 0.17, 0.4, 0.98, 0.3), main)
        shape(rect(0.6, 0.17, 0.98, 0.98, 0.3), second)
        ctx.fill(Path(CGRect(x: 0.385 * w, y: 0.25 * h, width: 0.012 * w, height: 0.66 * h)), with: .color(accent))
        ctx.fill(Path(CGRect(x: 0.603 * w, y: 0.25 * h, width: 0.012 * w, height: 0.66 * h)), with: .color(accent))
    case .handheld:
        shape(rect(0.02, 0.17, 0.98, 0.98, 0.3), main)
        ctx.fill(rect(0.37, 0.24, 0.63, 0.8, 0.06), with: .color(Color(argb: 0xFF15161A)))
        ctx.fill(rect(0.39, 0.29, 0.61, 0.75, 0.03), with: .color(second))
    case .panel:
        shape(rect(0.02, 0.2, 0.98, 0.98, 0.04), main)
        ctx.fill(Path(CGRect(x: 0.02 * w, y: 0.24 * h, width: 0.96 * w, height: 0.03 * h)), with: .color(accent))
        ctx.fill(rect(0.04, 0.32, 0.96, 0.94, 0.03), with: .color(second))
    }
}

// MARK: - Screen

struct PadScreen: View {
    @ObservedObject var model: AppModel
    @StateObject private var touches = PadTouches()
    @AppStorage("backdrop") private var backdrop = Backdrop.controller
    @AppStorage("buttonStyle") private var style = ButtonStyle.original
    @State private var editing = false
    @State private var tweaks: [Int: Tweak] = [:]
    @State private var selected: Int?
    @State private var dragStart: Tweak?

    private var base: PadLayout { layoutFor(model.link.game?.platform, screen: model.link.screen != nil) }
    private var layout: PadLayout { base.tweaked(tweaks) }

    var body: some View {
        GeometryReader { geo in
            ZStack {
                Color(argb: layout.background).ignoresSafeArea()
                backdropView(geo.size)
                Canvas { ctx, size in
                    if let area = layout.screen, let img = model.link.screen {
                        let r = screenRect(area, img, size)
                        ctx.fill(Path(roundedRect: r.insetBy(dx: -4, dy: -4), cornerRadius: 7), with: .color(.black))
                        if let frame = model.link.frame {
                            let c = ctx
                            c.draw(Image(uiImage: frame).interpolation(.none), in: r)
                        }
                    }
                    drawControls(&ctx, size, layout, touches, style, tweaks.compactMapValues { $0.style })
                    if editing, let i = selected, i < layout.controls.count {
                        let c = layout.controls[i]
                        ctx.stroke(circle(CGPoint(x: c.center.x * size.width, y: c.center.y * size.height), c.radius * size.height * 1.2),
                                   with: .color(Color(argb: layout.accent)), style: StrokeStyle(lineWidth: 2, dash: [6, 4]))
                    }
                }
                if editing {
                    editLayer(geo.size)
                } else {
                    TouchLayer(layout: layout, screenImage: model.link.screen, touches: touches,
                               onState: { model.link.sendPad($0) },
                               onTouch: { x, y, down in model.link.sendTouch(x: x, y: y, down: down) })
                }
                topBar
            }
        }
        .ignoresSafeArea()
        .statusBarHidden()
        .onAppear { tweaks = TweakStore.load(base) }
        .onChange(of: base.title) { _ in
            tweaks = TweakStore.load(base)
            selected = nil
        }
    }

    @ViewBuilder private func backdropView(_ size: CGSize) -> some View {
        if backdrop == .cover, let cover = model.link.game?.cover {
            Image(uiImage: cover).resizable().scaledToFill().frame(width: size.width, height: size.height).clipped()
                .overlay(Color.black.opacity(0.55))
        } else {
            Canvas { ctx, size in drawBody(&ctx, size, layout.body) }
        }
    }

    private var topBar: some View {
        VStack {
            HStack(spacing: 10) {
                Text(model.link.player > 0 ? "J\(model.link.player)" : "—")
                    .font(.system(size: 13, weight: .heavy)).padding(.horizontal, 10).padding(.vertical, 5)
                    .background(Capsule().fill(Color(argb: layout.accent)))
                Text(model.link.game?.name ?? layout.title)
                    .font(.system(size: 13, weight: .semibold)).lineLimit(1)
                if !model.link.wifiInput {
                    Text("Wi-Fi : menus seulement").font(.system(size: 11)).foregroundColor(.orange)
                }
                Spacer()
                if editing, let i = selected, i < layout.controls.count {
                    // This control only: its own style, then its own colour.
                    let t = tweaks[i] ?? Tweak()
                    Button("Style : \(t.style?.label ?? "comme les autres")") {
                        let order: [ButtonStyle?] = [nil] + ButtonStyle.allCases
                        var n = t
                        n.style = order[((order.firstIndex(of: t.style) ?? 0) + 1) % order.count]
                        tweaks[i] = n
                        TweakStore.save(base, tweaks)
                    }
                    if !layout.controls[i].isDpad {
                        Button {
                            var n = t
                            let next = buttonColors[((buttonColors.firstIndex(of: t.color) ?? 0) + 1) % buttonColors.count]
                            n.color = next
                            // A colour shows with the Origine style.
                            if next != nil { n.style = .original }
                            tweaks[i] = n
                            TweakStore.save(base, tweaks)
                        } label: {
                            HStack(spacing: 5) {
                                Text("Couleur")
                                Circle().fill(t.color.map { Color(argb: $0) } ?? .clear)
                                    .overlay(Circle().stroke(.white, lineWidth: 1.5)).frame(width: 14, height: 14)
                            }
                        }
                    }
                }
                if editing {
                    if selected == nil {
                        Picker("Style", selection: $style) {
                            ForEach(ButtonStyle.allCases, id: \.self) { Text($0.label).tag($0) }
                        }.pickerStyle(.menu)
                    }
                    Button("Réinitialiser") {
                        tweaks = [:]
                        TweakStore.save(base, tweaks)
                    }
                    Button("OK") {
                        editing = false
                        selected = nil
                    }.font(.system(size: 14, weight: .bold))
                } else {
                    Button { backdrop = backdrop == .controller ? .cover : .controller } label: {
                        Image(systemName: backdrop == .controller ? "photo" : "gamecontroller")
                    }
                    Button { editing = true } label: { Image(systemName: "slider.horizontal.3") }
                    if model.link.game != nil {
                        Button { model.link.quitGame() } label: { Image(systemName: "xmark.circle") }
                    }
                    Button { model.link.disconnect() } label: { Image(systemName: "wifi.slash") }
                }
            }
            .foregroundColor(.white)
            .padding(.horizontal, 54)
            .padding(.top, 8)
            Spacer()
        }
    }

    /// Personnaliser: drag a control to move it, pinch to resize it.
    private func editLayer(_ size: CGSize) -> some View {
        Color.white.opacity(0.001)
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { g in
                        if dragStart == nil {
                            selected = controlAt(layout, g.startLocation, size)
                            dragStart = selected.map { tweaks[$0] ?? Tweak() }
                        }
                        guard let i = selected, let start = dragStart else { return }
                        var t = start
                        t.dx = start.dx + g.translation.width / size.width
                        t.dy = start.dy + g.translation.height / size.height
                        tweaks[i] = t
                    }
                    .onEnded { _ in
                        dragStart = nil
                        TweakStore.save(base, tweaks)
                    }
            )
            .simultaneousGesture(
                MagnificationGesture()
                    .onChanged { s in
                        guard let i = selected else { return }
                        var t = tweaks[i] ?? Tweak()
                        if dragStart == nil { dragStart = t }
                        t.scale = min(2.2, max(0.5, (dragStart?.scale ?? 1) * s))
                        tweaks[i] = t
                    }
                    .onEnded { _ in
                        dragStart = nil
                        TweakStore.save(base, tweaks)
                    }
            )
    }
}
