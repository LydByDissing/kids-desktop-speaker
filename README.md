# Sphinx Documentation Skill

This project demonstrates a comprehensive Sphinx documentation setup with advanced features for professional project documentation.

## Features

- ✅ **Sphinx Documentation**: Complete Sphinx setup with RTD theme
- ✅ **PlantUML Support**: Local rendering with PlantUML 1.2026.1
- ✅ **C4 Architecture**: C4-PlantUML for system architecture diagrams
- ✅ **Requirements Management**: sphinx-needs for requirements tracking
- ✅ **Markdown Support**: MyST parser for .md files
- ✅ **Dual Format**: Supports both .rst and .md files
- ✅ **Live Reloading**: sphinx-autobuild for development
- ✅ **Virtual Environment**: Isolated Python environment
- ✅ **Documentation Templates**: ADR, Technology Assessment, Requirements
- ✅ **Comprehensive Guide**: Complete documentation writing guide

## Quick Start

```bash
# Activate the virtual environment
source .venv/bin/activate

# Navigate to docs directory
cd docs

# Build the documentation
make html

# View the documentation
open _build/html/index.html

# For live reloading during development
make autobuild
```

## Project Structure

```
sphinx-docs-skill/
├── .venv/                  # Virtual environment
├── _plantuml/              # PlantUML JAR file
│   └── plantuml-1.2026.1.jar
├── docs/                   # Documentation root
│   ├── _build/            # Generated HTML output
│   ├── source/            # Documentation source files
│   │   ├── _static/       # Static files
│   │   ├── _templates/    # Templates
│   │   ├── conf.py         # Sphinx configuration
│   │   ├── index.rst       # Main documentation
│   │   ├── getting_started.rst # Getting started guide
│   │   ├── api_reference.rst # API reference
│   │   ├── configuration.rst # Configuration docs
│   │   ├── plantuml_examples.rst # PlantUML examples
│   │   ├── c4_architecture.rst # C4 architecture diagrams
│   │   ├── documentation_guide.rst # Documentation guide
│   │   ├── templates/      # Documentation templates
│   │   │   ├── adr_template.rst # Architecture Decision Records
│   │   │   ├── technology_assessment.rst # Tech evaluations
│   │   │   └── requirements_template.rst # Requirements docs
│   │   ├── Makefile        # Build commands
│   │   ├── autobuild.sh    # Live reloading script
│   │   └── requirements.txt # Python dependencies
│   └── README.md          # This file
└── README.md              # This file
```

## Key Documentation Features

### PlantUML Examples
The documentation includes working PlantUML examples:

- **Class Diagram**: Shows Car and Person classes with relationships
- **Sequence Diagram**: Demonstrates User → Web Server → Database interaction
- **C4 Architecture**: System context, container, component, and deployment diagrams

### Requirements Management
- **sphinx-needs** integration for requirements tracking
- Traceability matrices
- Status and priority tracking
- Links between requirements, implementations, and tests

### Architecture Documentation
- **C4 Model** support for system architecture
- Context, container, component, and deployment diagrams
- Dynamic sequence diagrams for workflows

### Documentation Templates
- **Architecture Decision Records (ADR)**: Structured decision documentation
- **Technology Assessment**: Framework for evaluating new technologies
- **Requirements Template**: Comprehensive requirements documentation

## Requirements

- Python 3.7+
- Java 8+ (for local PlantUML rendering)
- pip

## Customization

Edit `docs/conf.py` to customize:

- Project name, author, version
- Extensions (add/remove Sphinx extensions)
- Theme (change from sphinx_rtd_theme)
- PlantUML configuration
- Markdown extensions

## Troubleshooting

**PlantUML diagrams not rendering?**
- Check Java installation: `java -version`
- Verify JAR file exists: `_plantuml/plantuml-1.2026.1.jar`
- Test manually: `java -jar _plantuml/plantuml-1.2026.1.jar -testdot`

**Build errors?**
- Activate virtual environment: `source .venv/bin/activate`
- Install dependencies: `pip install -r docs/requirements.txt`
- Clean build: `rm -rf docs/_build` and try again

## License

MIT License - See the LICENSE file for details.