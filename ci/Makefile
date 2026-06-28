# CI Pipeline — Kids Desktop Speaker
# ===================================
# Builds and tests Raspberry Pi OS images with RAUC OTA support.
# Uses Docker + QEMU user-mode emulation (plain Docker, no Dagger).
#
# Usage:
#   make build    — Build the Pi OS Docker image
#   make test     — Run smoke tests against the built image
#   make all      — build → test (full pipeline)
#   make clean    — Remove build artifacts
#
# Requirements:
#   - Docker with buildx
#   - QEMU user-mode emulation (binfmt_misc for arm)
#   - On GitHub Actions: docker/setup-qemu-action handles this automatically

.PHONY: all build test bundle export-rootfs clean cross-build vm-image vm smoke ci-test

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

IMAGE_NAME    := kids-desktop-speaker
IMAGE_TAG     := latest
PLATFORM      := linux/arm/v7
DOCKERFILE    := Dockerfile.pi-image
OUTPUT_DIR    ?= $(CURDIR)/output

# Source dir for cross-compiled service binaries, relative to the build
# context (repo root). Default = local/VM debug build (DEC-022). The release
# pipeline overrides this to ci/binaries (armv6 --release artifacts, DEC-007).
BIN_DIR       ?= target/armv7-unknown-linux-gnueabihf/debug

# Bundle version, written into the RAUC manifest. The release pipeline passes
# the git tag (github.ref_name); locally it derives from git describe.
VERSION       ?= $(shell git -C .. describe --tags --always --dirty 2>/dev/null || echo 0.1.0-dev)

# ---------------------------------------------------------------------------
# Targets
# ---------------------------------------------------------------------------

all: build test bundle

## build — Build the Pi OS Docker image for ARM
build:
	@echo "==> Building Pi OS image ($(PLATFORM))..."
	docker buildx build \
		--platform $(PLATFORM) \
		-f $(DOCKERFILE) \
		--build-arg BIN_DIR=$(BIN_DIR) \
		-t $(IMAGE_NAME):$(IMAGE_TAG) \
		--load \
		..
	@echo "==> Build complete: $(IMAGE_NAME):$(IMAGE_TAG)"

## test — Run smoke tests against the built image
test:
	@echo "==> Running smoke tests..."
	docker run --rm --platform $(PLATFORM) \
		--entrypoint /bin/bash \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		/usr/local/bin/smoke-test.sh
	@echo "==> Smoke tests passed"

## export-rootfs — Export the built image's root filesystem to a tar for bundling
export-rootfs:
	@echo "==> Exporting rootfs from $(IMAGE_NAME):$(IMAGE_TAG)..."
	mkdir -p $(OUTPUT_DIR)
	cid=$$(docker create --platform $(PLATFORM) $(IMAGE_NAME):$(IMAGE_TAG)); \
	docker export "$$cid" > $(OUTPUT_DIR)/rootfs.tar; \
	docker rm "$$cid" >/dev/null
	@echo "==> rootfs exported: $(OUTPUT_DIR)/rootfs.tar ($$(du -sh $(OUTPUT_DIR)/rootfs.tar | cut -f1))"

## bundle — Build RAUC bundle (.raucb) of the real rootfs, signed with throwaway dev certs
bundle: export-rootfs
	mkdir -p $(OUTPUT_DIR)
	docker run --rm --platform $(PLATFORM) \
		-v $(OUTPUT_DIR):/output \
		-v $(CURDIR)/create-bundle.sh:/usr/local/bin/create-bundle.sh:ro \
		-e VERSION=$(VERSION) \
		--entrypoint /bin/bash \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		/usr/local/bin/create-bundle.sh

## clean — Remove build artifacts
clean:
	@echo "==> Cleaning build artifacts..."
	docker rmi $(IMAGE_NAME):$(IMAGE_TAG) 2>/dev/null || true
	rm -rf $(OUTPUT_DIR)
	@echo "==> Clean complete"

## cross-build — Cross-compile all workspace binaries for ARMv7
cross-build:
	@echo "==> Cross-compiling for armv7-unknown-linux-gnueabihf..."
	cd .. && cross build --target armv7-unknown-linux-gnueabihf
	@echo "==> Cross-build complete"

## vm-image — Build QEMU ARM rootfs image (depends on cross-build)
vm-image: cross-build
	@bash vm/build-image.sh

## vm — Boot QEMU ARM VM with port-forwarded API on :8080
vm: vm-image
	@bash vm/run-vm.sh

## smoke — Run API smoke tests against VM on localhost:8080
smoke:
	@bash vm/dev-smoke.sh

## ci-test — Boot QEMU VM headlessly, run smoke tests, and exit with test exit code
ci-test:
	@bash vm/ci-run.sh
