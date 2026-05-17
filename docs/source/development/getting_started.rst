Getting Started
===============

This guide walks through setting up the development environment and running the
Kids Desktop Speaker project locally.

Prerequisites
-------------

- Python 3.10+
- Java 8+ (for PlantUML diagram rendering)
- Git
- A Raspberry Pi Zero W (for deployment)

Clone the Repository
--------------------

.. code-block:: bash

   git clone https://github.com/your-repo/kids-desktop-speaker.git
   cd kids-desktop-speaker

Set Up the Python Environment
-----------------------------

.. code-block:: bash

   python3 -m venv .venv
   source .venv/bin/activate
   pip install -r requirements.txt

Install System Dependencies
---------------------------

On Raspberry Pi OS (Debian/Ubuntu):

.. code-block:: bash

   sudo apt-get update
   sudo apt-get install -y \
       python3-dev \
       libasound2-dev \
       libatlas-base-dev \
       libbluetooth-dev \
       bluez \
       shairport-sync \
       librespot

Run the Application
-------------------

.. code-block:: bash

   python -m kids_speaker

The web interface will be available at ``http://localhost:8000``.

Running Tests
-------------

.. code-block:: bash

   # Run all tests
   pytest

   # Run with coverage
   pytest --cov=kids_speaker --cov-report=html

   # Run specific test module
   pytest tests/test_player.py

Build Documentation
-------------------

.. code-block:: bash

   cd docs
   make html

   # For live reloading during development
   make autobuild

Development Workflow
--------------------

1. Create a feature branch: ``git checkout -b feature/my-feature``
2. Make changes and run tests
3. Build documentation if needed
4. Commit and push
5. Open a pull request

Code Style
----------

- Follow `PEP 8`_ for Python code
- Use type hints for all public functions
- Write docstrings in Google format
- Keep functions small and focused

.. _PEP 8: https://peps.python.org/pep-0008/
