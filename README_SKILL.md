# Sphinx Documentation Skill

A professional, production-ready documentation system using Sphinx, PlantUML, and C4 model.

## Features

✅ **Professional Structure** - Organized by chapters with proper hierarchy
✅ **Read the Docs Theme** - Beautiful, responsive documentation theme
✅ **C4 Architecture** - System context, container, component diagrams
✅ **Requirements Management** - sphinx-needs for traceability
✅ **Diagram Support** - PlantUML integration for all diagram types
✅ **Markdown Support** - MyST parser for .md files
✅ **Live Reloading** - Automatic rebuilds during development
✅ **Template System** - ADR, technology assessment, requirements templates

## Quick Start

### Installation

```bash
# Clone or copy this repository
git clone https://github.com/your-repo/sphinx-docs-skill.git
cd sphinx-docs-skill

# Set up virtual environment
python -m venv .venv
source .venv/bin/activate

# Install dependencies
pip install -r docs/source/requirements.txt

# Download PlantUML (if needed)
mkdir -p docs/_plantuml
wget https://github.com/plantuml/plantuml/releases/download/v1.2026.1/plantuml-1.2026.1.jar \
     -O docs/_plantuml/plantuml-1.2026.1.jar
```

### Usage

```bash
# Build documentation
cd docs
make html

# Development with live reloading
./autobuild.sh

# Clean build
rm -rf _build && make html
```

### Customization

Edit `docs/source/conf.py` to customize:

```python
# Project information
project = 'Your Project Name'
copyright = '2024, Your Company'
author = 'Your Name'

# Theme options
html_theme = 'sphinx_rtd_theme'
html_theme_options = {
    'navigation_depth': 4,
    'collapse_navigation': False,
}
```

## Structure

```
docs/
├── source/                    # All documentation source
│   ├── index.rst               # Main entry point
│   ├── conf.py                 # Configuration
│   ├── introduction/           # Getting started
│   ├── architecture/          # System architecture
│   ├── development/            # Dev guides
│   ├── templates/             # Documentation templates
│   ├── guides/                 # User/admin guides
│   ├── reference/              # Glossary, FAQ, etc.
│   └── appendices/            # Best practices
├── _build/                    # Generated output
├── _plantuml/                  # PlantUML JAR
│   └── plantuml-1.2026.1.jar
├── Makefile                   # Build commands
└── autobuild.sh               # Live reloading
```

## Documentation Chapters

1. **Introduction** - Getting started guide
2. **Architecture** - C4 diagrams and system design
3. **Development** - API reference and configuration
4. **Templates** - ADR, tech assessment, requirements
5. **Guides** - User and admin guides
6. **Reference** - Glossary, FAQ, changelog
7. **Appendices** - Best practices and resources

## Requirements

- Python 3.7+
- Java 8+ (for PlantUML)
- pip (for package installation)

## Output Formats

- **HTML** - Default web output
- **PDF** - Via LaTeX (requires texlive)
- **ePub** - For e-readers

## Best Practices

1. **Heading Hierarchy**: Use =, -, ~, ^ consistently
2. **Cross-Referencing**: Use :doc: and :ref: roles
3. **Diagrams**: Keep PlantUML diagrams simple
4. **Reviews**: Peer-review all major documentation
5. **Updates**: Keep docs in sync with code changes

## License

MIT License - See LICENSE file for details.
