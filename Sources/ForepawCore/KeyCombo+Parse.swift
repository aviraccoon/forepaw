extension KeyCombo {
    /// Parse a combo string like "cmd+shift+s" or "return".
    public static func parse(_ string: String) -> KeyCombo {
        let parts = string.lowercased().split(separator: "+").map(String.init)
        var modifiers: [Modifier] = []
        var key = ""

        for part in parts {
            if let modifier = Modifier.fromString(part) {
                modifiers.append(modifier)
            } else {
                key = part
            }
        }

        // If no non-modifier key was found, the whole string is the key
        if key.isEmpty {
            key = string.lowercased()
            modifiers = []
        }

        return KeyCombo(key: key, modifiers: modifiers)
    }
}

extension KeyCombo.Modifier {
    static func fromString(_ s: String) -> KeyCombo.Modifier? {
        switch s {
        case "cmd", "command": .command
        case "shift": .shift
        case "opt", "option", "alt": .option
        case "ctrl", "control": .control
        default: nil
        }
    }
}
