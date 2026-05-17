# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information

project = 'Kids Desktop Speaker'
copyright = '2026, Kids Desktop Speaker Team'
author = 'Kids Desktop Speaker Team'
release = '0.1'

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = [
    'sphinx_needs',
    'sphinx.ext.autodoc',
    'sphinx.ext.viewcode',
    'sphinx.ext.todo',
]

# Sphinx-needs configuration
needs_build_json = True
needs_include_needs = True
needs_id_regex = r'^[A-Z]{2,4}_[A-Z]{2,4}_[0-9]{3}$'
needs_types = [
    {
        'directive': 'need',
        'title': 'Requirement',
        'prefix': 'R_',
        'color': '#BF8CFF',
        'style': 'node',
    },
    {
        'directive': 'specification',
        'title': 'Specification',
        'prefix': 'SPEC_',
        'color': '#8CFFBF',
        'style': 'node',
    },
    {
        'directive': 'test',
        'title': 'Test',
        'prefix': 'T_',
        'color': '#FF8C8C',
        'style': 'node',
    },
]

templates_path = ['_templates']
exclude_patterns = []

# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = 'sphinx_rtd_theme'
html_static_path = ['_static']

# -- Extension configuration -------------------------------------------------
