// Synthetic input and window/clipboard queries for mac.sh. Posting events needs the
// Accessibility permission of whatever app runs the shell.
import AppKit
import Carbon

// Named keys sit at the same place on every layout.
let namedKeys: [String: CGKeyCode] = [
    "enter": 36, "tab": 48, "space": 49, "backspace": 51, "escape": 53, "delete": 117,
    "left": 123, "right": 124, "down": 125, "up": 126,
]

/// Which key, with or without Shift, types each character on the current layout. QWERTZ
/// swaps Y and Z, so fixed key codes would type the wrong letters.
let layoutKeys: [Character: (CGKeyCode, Bool)] = {
    let source = TISCopyCurrentKeyboardLayoutInputSource().takeRetainedValue()
    let data = Unmanaged<CFData>.fromOpaque(TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)).takeUnretainedValue() as Data
    var map: [Character: (CGKeyCode, Bool)] = [:]
    data.withUnsafeBytes { raw in
        let layout = raw.bindMemory(to: UCKeyboardLayout.self).baseAddress!
        for shift in [false, true] {
            for code in 0..<128 {
                var dead: UInt32 = 0
                var length = 0
                var chars = [UniChar](repeating: 0, count: 4)
                UCKeyTranslate(layout, UInt16(code), UInt16(kUCKeyActionDown), shift ? UInt32(shiftKey >> 8) : 0, UInt32(LMGetKbdType()),
                               OptionBits(kUCKeyTranslateNoDeadKeysBit), &dead, 4, &length, &chars)
                if length == 1, let c = String(utf16CodeUnits: chars, count: 1).first, map[c] == nil {
                    map[c] = (CGKeyCode(code), shift)
                }
            }
        }
    }
    return map
}()

let modifierFlags: [String: CGEventFlags] = [
    "cmd": .maskCommand, "shift": .maskShift, "alt": .maskAlternate, "ctrl": .maskControl,
]

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

func key(_ code: CGKeyCode, down: Bool, flags: CGEventFlags = []) {
    let event = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)!
    event.flags = flags
    event.post(tap: .cghidEventTap)
    usleep(30_000)
}

func code(_ name: String) -> CGKeyCode {
    if let code = namedKeys[name] { return code }
    if name.count == 1, case let (code, false)? = layoutKeys[Character(name)] { return code }
    fail("unknown key '\(name)'")
}

func mouse(_ type: CGEventType, _ x: Double, _ y: Double) {
    let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: CGPoint(x: x, y: y), mouseButton: .left)!
    event.setIntegerValueField(.mouseEventClickState, value: 1)
    // A nil source copies the modifier state of earlier posted keys, and screencapture
    // treats a held Control as "copy instead of save".
    event.flags = []
    event.post(tap: .cghidEventTap)
}

func drag(_ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double) {
    mouse(.mouseMoved, x1, y1)
    usleep(200_000)
    mouse(.leftMouseDown, x1, y1)
    for i in 1...20 {
        let t = Double(i) / 20
        mouse(.leftMouseDragged, x1 + (x2 - x1) * t, y1 + (y2 - y1) * t)
        usleep(25_000)
    }
    mouse(.leftMouseUp, x2, y2)
    usleep(300_000)
}

let args = Array(CommandLine.arguments.dropFirst())
let num = { (i: Int) -> Double in
    guard i < args.count, let n = Double(args[i]) else { fail("expected a number at argument \(i)") }
    return n
}
switch args.first ?? "" {
case "front":
    print(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0)
case "windows":
    // Normal-level on-screen windows of a pid, as "x y w h" in points.
    let pid = Int(num(1))
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    for w in list where w[kCGWindowOwnerPID as String] as? Int == pid && w[kCGWindowLayer as String] as? Int == 0 {
        let b = w[kCGWindowBounds as String] as! [String: Double]
        print(Int(b["X"]!), Int(b["Y"]!), Int(b["Width"]!), Int(b["Height"]!))
    }
case "key":
    // "cmd+shift+4": modifiers ride on the key's flags.
    let parts = args[1].lowercased().split(separator: "+").map(String.init)
    let flags = parts.dropLast().reduce(CGEventFlags()) { acc, m in
        guard let f = modifierFlags[m] else { fail("unknown modifier '\(m)'") }
        return acc.union(f)
    }
    let k = code(parts.last!)
    key(k, down: true, flags: flags)
    key(k, down: false, flags: flags)
case "type":
    for ch in args.dropFirst().joined(separator: " ") {
        guard let (k, shift) = layoutKeys[ch] else { fail("no key types '\(ch)' on this layout") }
        let flags: CGEventFlags = shift ? .maskShift : []
        key(k, down: true, flags: flags)
        key(k, down: false, flags: flags)
    }
case "click":
    drag(num(1), num(2), num(1), num(2))
case "drag":
    // An optional fifth argument is a key held for the whole drag, like the A or R tool.
    let hold = args.count > 5 ? code(args[5]) : nil
    if let hold { key(hold, down: true); usleep(150_000) }
    drag(num(1), num(2), num(3), num(4))
    if let hold { key(hold, down: false) }
case "clipboard":
    // Types on the general pasteboard; PNG and TIFF payloads land in the given directory.
    let pb = NSPasteboard.general
    print("types", (pb.types ?? []).map(\.rawValue).joined(separator: ","))
    for (type, ext) in [(NSPasteboard.PasteboardType.png, "png"), (.tiff, "tiff")] {
        if let data = pb.data(forType: type) {
            let path = "\(args[1])/clipboard.\(ext)"
            try! data.write(to: URL(fileURLWithPath: path))
            let rep = NSBitmapImageRep(data: data)
            print(ext, "\(rep?.pixelsWide ?? 0)x\(rep?.pixelsHigh ?? 0)px", "\(Int(rep?.size.width ?? 0))x\(Int(rep?.size.height ?? 0))pt", path)
        } else {
            print(ext, "none")
        }
    }
default:
    fail("usage: macdrive front|windows PID|key COMBO|type TEXT|click X Y|drag X1 Y1 X2 Y2 [HOLD]|clipboard DIR")
}
