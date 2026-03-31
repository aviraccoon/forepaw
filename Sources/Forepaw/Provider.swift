import ForepawCore
import ForepawDarwin

/// The single point where the CLI creates a platform provider.
/// All command files use this instead of importing ForepawDarwin directly.
/// Typed as `any DesktopProvider` so the compiler enforces that commands
/// only call protocol methods -- if a method isn't on the protocol, it
/// won't compile.
let provider: any DesktopProvider = DarwinProvider()
