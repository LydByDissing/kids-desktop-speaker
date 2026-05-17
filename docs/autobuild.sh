#!/usr/bin/env bash
# Sphinx Autobuild Script
# This script watches for changes and automatically rebuilds the documentation

# Activate virtual environment
if [ -f "../.venv/bin/activate" ]; then
    source "../.venv/bin/activate"
fi

# Run sphinx-autobuild
sphinx-autobuild source _build/html --host 0.0.0.0 --port 8000
