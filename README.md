# Kids Desktop Speaker

A small-form-factor streaming speaker for children, built on Raspberry Pi Zero W. Streams audio from Spotify, AirPlay, podcasts, and internet radio. Controlled via a web interface, native mobile apps, and Bluetooth.

## Hardware

| Component | Description |
|-----------|-------------|
| **Base** | Raspberry Pi Zero W (v1) |
| **Amp** | IQaudIO Pi-Codec Zero HAT |
| **DSP** | CamillaDSP (software) |

## Features

- **Spotify Connect** — Stream directly from Spotify
- **AirPlay** — Stream from Apple devices
- **Podcasts** — Subscribe and play podcast episodes
- **Internet Radio** — Stream from online radio stations
- **Web Interface** — Control playback from any browser
- **Bluetooth Control** — Volume and playback via Bluetooth
- **TTS Announcements** — Custom text messages read aloud
- **Multi-room Audio** — Sync multiple speakers on the same network
- **Home Assistant** — Smart home integration

## Quick Start

```bash
# Clone the repository
git clone https://github.com/your-repo/kids-desktop-speaker.git
cd kids-desktop-speaker

# Set up the Python environment
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt

# Run the speaker service
python -m kids_speaker
```

## Documentation

Full project documentation is in the `docs/` directory, built with Sphinx.

```bash
# Build documentation
cd docs
make html

# Live-reload during development
make autobuild

# View the result
open _build/html/index.html
```

## Project Structure

```
kids-desktop-speaker/
├── docs/                   # Sphinx documentation
│   ├── source/            # Documentation source files
│   │   ├── architecture/  # C4 architecture diagrams
│   │   ├── development/   # Developer guides
│   │   ├── requirements/  # Project requirements
│   │   └── ...
│   ├── _build/            # Generated HTML output
│   └── Makefile           # Build commands
├── src/                   # Application source code
│   └── kids_speaker/      # Main package
├── tests/                 # Test suite
├── requirements.txt       # Python dependencies
└── README.md              # This file
```

## License

This project is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.
