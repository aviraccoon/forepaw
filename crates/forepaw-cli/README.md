# forepaw-cli

A raccoon's paws on your desktop, from the terminal.

CLI for [forepaw](https://crates.io/crates/forepaw) desktop automation.
Control any application through accessibility trees, OCR, and input simulation.

```bash
forepaw snapshot --app Finder -i
forepaw click @e3 --app Finder
forepaw ocr --app Notes
forepaw list-apps
```

Full documentation at [github.com/aviraccoon/forepaw](https://github.com/aviraccoon/forepaw).

## Install

```bash
cargo install forepaw-cli
```

Or download binaries from [releases](https://github.com/aviraccoon/forepaw/releases).

## License

[Unlicense](https://unlicense.org/). Raccoons don't believe in fences.
