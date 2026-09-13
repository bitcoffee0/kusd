"""Command-line entry point for a complete KUSD deployment."""
import os
import runpy

os.environ.setdefault("KUSD_TAG", "kusd")

runpy.run_module("deployment_core", run_name="__main__")
