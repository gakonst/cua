// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Cua AI, Inc.

import AppKit

/// A disposable editor oracle. State is read from NSTextView, independently
/// of Driver's AX readback. The extra modes model applications that acknowledge
/// a selection write without applying it, or edit during that write.
final class SelectionOracleTextView: NSTextView {
    var mode = "normal"

    override func setAccessibilitySelectedTextRange(_ range: NSRange) {
        if mode == "ignore" { return }
        super.setAccessibilitySelectedTextRange(range)
        if mode == "mutate" {
            textStorage?.append(NSAttributedString(string: "!"))
        }
    }

    override func accessibilitySelectedText() -> String? {
        if mode == "misreport" { return "inconsistent selection" }
        return super.accessibilitySelectedText()
    }
}

final class TextSelectionFixture {
    static let seed = "Fixture text 😀 world. The cat sat. The cat ran. café e\u{301} 👍🏽 end"
    private let directory: URL
    private let primary = NSWindow(
        contentRect: NSRect(x: 120, y: 180, width: 620, height: 260),
        styleMask: [.titled, .closable], backing: .buffered, defer: false)
    private let sibling = NSWindow(
        contentRect: NSRect(x: 180, y: 460, width: 620, height: 180),
        styleMask: [.titled, .closable], backing: .buffered, defer: false)
    private let editor = SelectionOracleTextView(frame: NSRect(x: 16, y: 60, width: 580, height: 180))
    private let siblingEditor = NSTextView(frame: NSRect(x: 16, y: 16, width: 580, height: 140))
    private var timer: Timer?
    private var sequence = 0
    private var lastState: Data?

    init(directory: URL) {
        self.directory = directory
        primary.title = "CuaTestHarness Selection Primary"
        sibling.title = "CuaTestHarness Selection Sibling"
        primary.isReleasedWhenClosed = false
        sibling.isReleasedWhenClosed = false
        editor.string = Self.seed
        editor.font = .systemFont(ofSize: 16)
        editor.setAccessibilityIdentifier("selection-editor")
        primary.contentView?.addSubview(editor)
        let button = NSButton(title: "Not a text control", target: nil, action: nil)
        button.frame = NSRect(x: 16, y: 12, width: 240, height: 32)
        button.setAccessibilityIdentifier("selection-button")
        primary.contentView?.addSubview(button)
        siblingEditor.string = "Sibling must remain unchanged"
        siblingEditor.setAccessibilityIdentifier("selection-sibling")
        sibling.contentView?.addSubview(siblingEditor)
    }

    func show() {
        primary.makeKeyAndOrderFront(nil)
        sibling.makeKeyAndOrderFront(nil)
        sibling.makeFirstResponder(siblingEditor)
        editor.setSelectedRange(NSRange(location: 0, length: 0))
        siblingEditor.setSelectedRange(NSRange(location: 0, length: 0))
        publish()
        timer = Timer.scheduledTimer(withTimeInterval: 0.02, repeats: true) { [weak self] _ in
            self?.readCommand()
            self?.publish()
        }
    }

    private func readCommand() {
        guard let data = try? Data(contentsOf: directory.appendingPathComponent("command.json")),
              let command = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let next = command["sequence"] as? Int, next > sequence else { return }
        sequence = next
        if let mode = command["mode"] as? String { editor.mode = mode }
        if command["reset"] as? Bool == true {
            editor.string = Self.seed
            editor.setSelectedRange(NSRange(location: 0, length: 0))
        }
        if command["close_primary"] as? Bool == true { primary.close() }
        publish()
    }

    private func editorState(_ view: NSTextView) -> [String: Any] {
        let range = view.selectedRange()
        let value = view.string as NSString
        return [
            "value": view.string,
            "range": ["location": range.location, "length": range.length],
            "selected_text": range.location <= value.length && range.length <= value.length - range.location
                ? value.substring(with: range) : "INVALID_RANGE",
        ]
    }

    private func publish() {
        do {
            let data = try JSONSerialization.data(withJSONObject: [
                "pid": ProcessInfo.processInfo.processIdentifier,
                "primary_window": primary.windowNumber,
                "sibling_window": sibling.windowNumber,
                "sequence": sequence,
                "mode": editor.mode,
                "primary": editorState(editor),
                "sibling": editorState(siblingEditor),
            ], options: [.sortedKeys])
            if data != lastState {
                try data.write(to: directory.appendingPathComponent("state.json"), options: .atomic)
                lastState = data
            }
        } catch { fputs("selection oracle write failed: \(error)\n", stderr) }
    }
}
