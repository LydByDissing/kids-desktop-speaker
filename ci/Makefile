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

.PHONY: all build test bundle clean cross-build vm-image vm smoke ci-test

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

IMAGE_NAME    := kids-desktop-speaker
IMAGE_TAG     := latest
PLATFORM      := linux/arm/v7
DOCKERFILE    := Dockerfile.pi-image
OUTPUT_DIR    ?= $(CURDIR)/output

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

## bundle — Build RAUC bundle artifact (.raucb) using throwaway dev certs
bundle:
	mkdir -p $(OUTPUT_DIR)
	docker run --rm --platform $(PLATFORM) \
		-v $(OUTPUT_DIR):/output \
		-v $(CURDIR)/create-bundle.sh:/usr/local/bin/create-bundle.sh:ro \
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
