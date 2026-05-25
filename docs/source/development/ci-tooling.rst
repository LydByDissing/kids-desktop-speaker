CI Tooling Evaluation
=====================

This document evaluates tooling options for the local CI pipeline
(:ref:`DEC-002 <dec-002>`). The pipeline must build ARM (armhf) Raspberry Pi
OS images on x86_64 hardware, run smoke tests, and produce RAUC bundles.

.. contents::
   :local:
   :depth: 2

Requirements
------------

The CI tooling must support:

1. **Cross-architecture builds**: Build ARM containers/images on x86_64 via QEMU
   user-mode emulation (``binfmt_misc``)
2. **Docker image construction**: Build a Pi OS base image with our services
3. **Smoke testing**: Boot the container with systemd, verify services start
4. **RAUC bundle creation**: Run ``rauc bundle`` to produce a ``.raucb`` artifact
5. **Local + CI parity**: Same tooling works on a developer's Linux laptop and
   in GitHub Actions
6. **Caching**: Layer caching for Docker builds to keep iteration times low

Options Evaluated
-----------------

Dagger
~~~~~~

`Dagger`_ (v0.20.8) wraps Docker/BuildKit with a programmatic SDK (Python,
Go, TypeScript). It runs pipelines as DAGs of container operations, with
built-in caching, secret management, and observability.

**What Dagger provides:**

.. code-block:: python

   import dagger

   async def build_pi_image():
       async with dagger.Connection() as client:
           # Build ARM container from Dockerfile
           ctr = client.container().platform("linux/arm/v7").build(
               client.host().directory("."),
               dockerfile="Dockerfile.pi-image",
           )
           # Run smoke tests
           result = ctr.with_exec(["systemctl", "is-active", "api-server"])
           await result.sync()
           # Export RAUC bundle
           ctr.with_exec(["rauc", "bundle", "/rootfs", "/output/image.raucb"])
           await ctr.file("/output/image.raucb").export("image.raucb")

**Pros:**

- ✅ Programmatic pipeline definition — full Python control flow, not just
  YAML
- ✅ Built-in content-addressable caching — skips unchanged steps
- ✅ Secret management built in (no env var leakage in logs)
- ✅ Works with existing Dockerfiles — no new image format to learn
- ✅ Local and CI parity — same Python code runs on laptop and GitHub Actions
- ✅ Good observability — step-by-step trace, logs per step
- ✅ Active development, strong community (30k+ GitHub stars)

**Cons:**

- ⚠️ **Requires a Dagger engine** — runs as a containerized BuildKit instance.
  Adds a dependency that plain Docker doesn't need. On GitHub Actions, this
  means installing the Dagger CLI first.
- ⚠️ **Python SDK is the only option for us** — Go SDK requires writing Go,
  TypeScript SDK requires Node.js. Python is fine for build scripts but adds
  a dependency to the project.
- ⚠️ **Overkill for simple pipelines** — Our pipeline is: build → test → bundle.
  That's 3 Docker operations. Dagger's caching and DAG scheduling shine for
  complex multi-stage pipelines with many interdependencies. For 3 linear
  steps, the value is marginal.
- ⚠️ **Version churn** — Dagger has had significant API changes between major
  versions (v0.9 → v0.11 → v0.18). The Python SDK is stable now but the
  project has a history of breaking changes.

Plain Docker + Shell Scripts
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Standard Docker with a shell script or Makefile driving the pipeline. Uses
``docker buildx`` for cross-platform builds and ``docker run`` for testing.

**What plain Docker provides:**

.. code-block:: makefile

   # Makefile
   .PHONY: build test bundle

   build:
   	docker buildx build \
   		--platform linux/arm/v7 \
   		-f Dockerfile.pi-image \
   		-t kds:latest \
   		.

   test: build
   	docker run --rm --platform linux/arm/v7 \
   		--privileged \
   		-v /sys/fs/cgroup:/sys/fs/cgroup:rw \
   		kds:latest \
   		/bin/sh -c "systemctl is-active api-server && rauc status"

   bundle: build
   	docker run --rm --platform linux/arm/v7 \
   		-v $(PWD)/output:/output \
   		kds:latest \
   		/bin/sh -c "rauc bundle / /output/image.raucb"

**Pros:**

- ✅ **Zero new dependencies** — Docker is already required. No Dagger engine,
  no Python packages, no new binaries.
- ✅ **Maximum compatibility** — Works on any Linux with Docker. GitHub Actions
   has Docker pre-installed. No setup steps.
- ✅ **Transparent** — Every step is a visible Docker command. Easy to debug
   by running commands manually.
- ✅ **Simple** — 3 steps, 3 ``docker`` commands. No abstraction layer.
- ✅ **Proven** — This is how most embedded Linux projects build images in CI.

**Cons:**

- ❌ **No built-in caching** — Docker layer caching works but is less
   sophisticated than Dagger's content-addressable cache. Rebuilds re-execute
   unchanged layers if the cache is cold.
- ❌ **No secret management** — Must handle secrets via Docker build args or
   mount files manually.
- ❌ **Less structured** — Shell scripts can become messy as the pipeline grows.
   No typed pipeline definition.

GitHub Actions (CI Platform)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Regardless of whether we use Dagger or plain Docker, GitHub Actions is the CI
platform. The question is what runs *inside* the GitHub Actions runner.

**Relevant GitHub Actions features:**

- ``docker/setup-qemu-action`` — registers QEMU binfmt for cross-arch builds
- ``docker/setup-buildx-action`` — enables ``docker buildx`` for multi-platform
- ``actions/upload-artifact`` — publishes the ``.raucb`` bundle
- ``ubuntu-latest`` runners have Docker pre-installed

**ARM runner availability:** GitHub does not provide free ARM runners for
private repos. We must use QEMU emulation on x86_64 runners.

Recommendation
--------------

**Use plain Docker + shell scripts for v1.** Add Dagger later if the pipeline
grows complex enough to justify it.

**Rationale:**

1. **Our pipeline is simple** — build → test → bundle. Three linear steps.
   Dagger's DAG scheduling and caching shine for complex pipelines with many
   parallel branches. For 3 steps, plain Docker is sufficient.

2. **Minimize dependencies** — The project already depends on Docker. Adding
   Dagger means: installing the Dagger CLI in CI, maintaining a Python
   runtime for build scripts, and managing Dagger engine lifecycle. For a
   3-step pipeline, this is not worth it.

3. **Debuggability** — When a build fails in CI, the first step is to
   reproduce locally. With plain Docker, every CI step is a ``docker``
   command you can run directly. With Dagger, you need the Dagger engine
   running locally too.

4. **GitHub Actions compatibility** — Plain Docker works out of the box on
   ``ubuntu-latest`` runners. No setup steps beyond ``setup-qemu-action`` and
   ``setup-buildx-action``.

5. **Dagger can be added later** — If the pipeline grows (e.g., parallel test
   suites, multi-architecture builds, complex caching needs), Dagger can be
   layered on top without changing the Dockerfile or the fundamental approach.

**When to reconsider Dagger:**

- Pipeline grows beyond ~10 interdependent steps
- Need for sophisticated caching (e.g., Rust build cache across PRs)
- Multiple developers need to run the full pipeline locally with reliable
  caching
- Secret management becomes a pain point with plain Docker

Implementation Sketch
---------------------

The v1 CI pipeline uses a ``Makefile`` + GitHub Actions:

.. code-block:: makefile

   # ci/Makefile
   PLATFORM    ?= linux/arm/v7
   IMAGE_TAG   ?= kds:latest
   OUTPUT_DIR  ?= $(PWD)/output

   .PHONY: build test bundle all

   all: build test bundle

   build:
   	@echo "=== Building Pi OS image ==="
   	docker buildx build \
   		--platform $(PLATFORM) \
   		-f Dockerfile.pi-image \
   		-t $(IMAGE_TAG) \
   		--cache-from type=gha \
   		--cache-to type=gha,mode=max \
   		.

   test: build
   	@echo "=== Running smoke tests ==="
   	docker run --rm --platform $(PLATFORM) \
   		--privileged \
   		-v /sys/fs/cgroup:/sys/fs/cgroup:rw \
   		$(IMAGE_TAG) \
   		/bin/sh -c "\
   			rauc status && \
   			echo 'RAUC slot status OK'"

   bundle: build
   	@echo "=== Building RAUC bundle ==="
   	mkdir -p $(OUTPUT_DIR)
   	docker run --rm --platform $(PLATFORM) \
   		-v $(OUTPUT_DIR):/output \
   		$(IMAGE_TAG) \
   		/bin/sh -c "rauc bundle / /output/image.raucb"
   	@echo "RAUC bundle: $(OUTPUT_DIR)/image.raucb"

   clean:
   	rm -rf $(OUTPUT_DIR)
   	docker rmi $(IMAGE_TAG) 2>/dev/null || true

.. code-block:: yaml

   # .github/workflows/build-image.yml
   name: Build Pi Image

   on:
     push:
       branches: [main]
     pull_request:

   jobs:
     build:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4

         - name: Set up QEMU
           uses: docker/setup-qemu-action@v3
           with:
             platforms: arm

         - name: Set up Docker Buildx
           uses: docker/setup-buildx-action@v3

         - name: Build image
           run: make -C ci build

         - name: Run smoke tests
           run: make -C ci test

         - name: Build RAUC bundle
           if: github.event_name == 'push' && github.ref == 'refs/heads/main'
           run: make -C ci bundle

         - name: Upload RAUC bundle
           if: github.event_name == 'push' && github.ref == 'refs/heads/main'
           uses: actions/upload-artifact@v4
           with:
             name: rauc-bundle
             path: output/image.raucb
             retention-days: 30
