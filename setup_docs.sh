#!/bin/bash
# Sphinx Documentation Skill Setup Script

echo "🚀 Setting up Sphinx Documentation Skill..."

# Check Python
if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 is required but not installed"
    exit 1
fi

# Create virtual environment
if [ ! -d ".venv" ]; then
    echo "🐍 Creating virtual environment..."
    python3 -m venv .venv
fi

# Activate virtual environment
source .venv/bin/activate

# Install dependencies
if [ -f "docs/source/requirements.txt" ]; then
    echo "📦 Installing dependencies..."
    pip install -r docs/source/requirements.txt
else
    echo "⚠️  requirements.txt not found in docs/source/"
fi

# Check PlantUML
if [ ! -f "docs/_plantuml/plantuml-1.2026.1.jar" ]; then
    echo "🎨 Downloading PlantUML..."
    mkdir -p docs/_plantuml
    wget https://github.com/plantuml/plantuml/releases/download/v1.2026.1/plantuml-1.2026.1.jar \
         -O docs/_plantuml/plantuml-1.2026.1.jar
fi

# Verify Java for PlantUML
if ! command -v java &> /dev/null; then
    echo "⚠️  Java is recommended for PlantUML diagrams but not required"
fi

echo "✅ Setup complete!"
echo ""
echo "Next steps:"
echo "1. cd docs"
echo "2. make html  # Build documentation"
echo "3. ./autobuild.sh  # Live reloading"
