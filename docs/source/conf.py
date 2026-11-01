# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
project = 'Kids Desktop Speaker'
copyright = '2026, Lyd by Dissing'
author = 'Lyd by Dissing'
release = '0.1'

# -- General configuration ---------------------------------------------------
extensions = [
    'sphinxcontrib.plantuml',
    'sphinx_needs',
    'sphinx_data_viewer',
    'myst_parser',
    'sphinx.ext.autodoc',
    'sphinx.ext.viewcode',
    'sphinx.ext.todo',
    'sphinx.ext.intersphinx',
]

# Source file formats
source_suffix = {
    '.rst': 'restructuredtext',
    '.md': 'markdown',
}

# Sphinx-needs configuration
needs_build_json = True
needs_include_needs = True
needs_id_regex = r'^[A-Z]{2,4}_[A-Z]{2,4}_[0-9]{3}$'
needs_types = [
    {
        'directive': 'req',
        'title': 'Requirement',
        'prefix': 'REQ_',
        'color': '#BF8CFF',
        'style': 'node',
    },
    {
        'directive': 'spec',
        'title': 'Specification',
        'prefix': 'SPEC_',
        'color': '#8CFFBF',
        'style': 'node',
    },
    {
        'directive': 'impl',
        'title': 'Implementation',
        'prefix': 'IMPL_',
        'color': '#438DD5',
        'style': 'node',
    },
    {
        'directive': 'test',
        'title': 'Test Case',
        'prefix': 'T_',
        'color': '#FF8C8C',
        'style': 'node',
    },
]

needs_links = {
    'fulfills': {
        'incoming': 'is fulfilled by',
        'outgoing': 'fulfills',
    },
    'depends': {
        'incoming': 'is depended on by',
        'outgoing': 'depends on',
    },
    'tests': {
        'incoming': 'is tested by',
        'outgoing': 'tests',
    },
}

# PlantUML configuration
import os
_here = os.path.dirname(os.path.abspath(__file__))
plantuml = 'java -jar ' + os.path.join(_here, '..', '_plantuml', 'plantuml-1.2026.1.jar')
plantuml_output_format = 'png'

templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store']

# -- Options for HTML output -------------------------------------------------
html_theme = 'sphinx_rtd_theme'
html_static_path = ['_static']
html_theme_options = {
    'navigation_depth': 4,
    'collapse_navigation': False,
    'sticky_navigation': True,
}

# -- MyST Parser configuration ------------------------------------------------
myst_enable_extensions = [
    'colon_fence',
    'deflist',
    'tasklist',
]
