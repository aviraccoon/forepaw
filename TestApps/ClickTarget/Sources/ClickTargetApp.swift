import SwiftUI

@main
struct ClickTargetApp: App {
    init() {
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.activate(ignoringOtherApps: true)
        ProcessInfo.processInfo.disableAutomaticTermination("test app")
    }

    var body: some Scene {
        WindowGroup {
            ClickTargetView()
        }
        .defaultSize(width: 700, height: 500)
    }
}

// MARK: - Models

struct ClickDot: Identifiable {
    let id = UUID()
    let x: CGFloat
    let y: CGFloat
    let label: String
    let color: Color
    let size: CGFloat
}

// MARK: - Main View

struct ClickTargetView: View {
    @State private var lastEvent: String = "No clicks yet"
    @State private var clickLog: [String] = []
    @State private var clickDots: [ClickDot] = []
    @State private var textFieldValue: String = ""

    var body: some View {
        ZStack {
            VStack(spacing: 0) {
                Text("Click Target Test App")
                    .font(.title)
                    .padding(.top, 16)

                // Word grid
                Grid(horizontalSpacing: 40, verticalSpacing: 20) {
                    GridRow {
                        TargetWord("alpha")
                        TargetWord("beta")
                        TargetWord("gamma")
                    }
                    GridRow {
                        TargetWord("delta")
                        TargetWord("epsilon")
                        TargetWord("zeta")
                    }
                    GridRow {
                        TargetWord("eta")
                        TargetWord("theta")
                        TargetWord("iota")
                    }
                }
                .padding(.vertical, 20)
                .padding(.horizontal, 30)

                // Text field for type/keyboard-type testing
                HStack {
                    Text("Input:")
                        .font(.system(.body, design: .monospaced))
                    TextField("Type here...", text: $textFieldValue)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(.body, design: .monospaced))
                        .accessibilityIdentifier("test-input")
                }
                .padding(.horizontal, 30)
                .padding(.bottom, 8)

                // Clear button
                HStack {
                    Button("Clear dots") {
                        clickDots.removeAll()
                    }
                    Button("Clear log") {
                        clickLog.removeAll()
                        lastEvent = "No clicks yet"
                    }
                    Button("Clear input") {
                        textFieldValue = ""
                    }
                }
                .padding(.bottom, 8)

                Divider()

                // Status
                Text(lastEvent)
                    .font(.system(.body, design: .monospaced))
                    .padding(6)

                // Click log
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(clickLog.reversed(), id: \.self) { entry in
                            Text(entry)
                                .font(.system(.caption, design: .monospaced))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 8)
                }
            }

            // Click dot overlays
            ForEach(clickDots) { dot in
                Circle()
                    .fill(dot.color)
                    .frame(width: dot.size, height: dot.size)
                    .position(x: dot.x, y: dot.y)
                Text(dot.label)
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundColor(dot.color)
                    .position(x: dot.x + 30, y: dot.y - 10)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onContinuousHover { phase in
            if case .active(let location) = phase {
                lastEvent = "mouse: \(Int(location.x)),\(Int(location.y))"
            }
        }
        .onAppear {
            setupClickMonitors()
        }
    }

    private func setupClickMonitors() {
        // Local monitor: clicks that reach this app
        NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { event in
            guard let window = event.window else { return event }
            let windowPoint = event.locationInWindow
            let titleBarHeight = window.frame.height - window.contentLayoutRect.height
            let flipped = CGPoint(
                x: windowPoint.x,
                y: window.frame.height - windowPoint.y - titleBarHeight
            )

            let isRight = event.type == .rightMouseDown
            let clickCount = event.clickCount
            let buttonName = isRight ? "right" : "left"
            let entry = "LOCAL \(Int(flipped.x)),\(Int(flipped.y)) \(buttonName) x\(clickCount)"
            print(entry)
            fflush(stdout)
            lastEvent = entry
            clickLog.append(entry)

            // Color: red=left, blue=right. Size: 10=single, 16=double, 22=triple
            let color: Color = isRight ? .blue : .red
            let size: CGFloat = CGFloat(6 + clickCount * 5)
            let label = "\(isRight ? "R" : "L")x\(clickCount)"
            clickDots.append(ClickDot(
                x: flipped.x, y: flipped.y,
                label: label, color: color, size: size
            ))

            return event
        }

        // Global monitor: clicks when this app is not frontmost
        NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { event in
            let screenPoint = NSEvent.mouseLocation
            let screenHeight = NSScreen.main?.frame.height ?? 0
            let flipped = CGPoint(x: screenPoint.x, y: screenHeight - screenPoint.y)
            let buttonName = event.type == .rightMouseDown ? "right" : "left"
            let entry = "GLOBAL \(Int(flipped.x)),\(Int(flipped.y)) \(buttonName) x\(event.clickCount)"
            print(entry)
            fflush(stdout)
            lastEvent = entry
            clickLog.append(entry)
        }
    }
}

// MARK: - Components

struct TargetWord: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.system(size: 18, weight: .medium))
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(Color.blue.opacity(0.15))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color.blue.opacity(0.3), lineWidth: 1)
                    )
            )
    }
}
