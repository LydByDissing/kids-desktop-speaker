# CI Pipeline — Kids Desktop Speaker
# ===================================
# Builds, tests, and bundles Raspberry Pi OS images with RAUC OTA support.
# Uses Docker + QEMU user-mode emulation (plain Docker, no Dagger).
#
# Usage:
#   make build    — Build the Pi OS Docker image
#   make test     — Run smoke tests against the built image
#   make bundle   — Produce a RAUC bundle (.raucb) from the image
#   make all      — build → test → bundle (full pipeline)
#
# Requirements:
#   - Docker with buildx
#   - QEMU user-mode emulation (binfmt_misc for arm)
#   - On GitHub Actions: docker/setup-qemu-action handles this automatically

.PHONY: all build test bundle clean

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

IMAGE_NAME    := kids-desktop-speaker
IMAGE_TAG     := latest
PLATFORM      := linux/arm/v7
DOCKERFILE    := Dockerfile.pi-image
OUTPUT_DIR    := output
BUNDLE_NAME   := image.raucb

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
test: build
	@echo "==> Running smoke tests..."
	@mkdir -p $(OUTPUT_DIR)
	docker run --rm --platform $(PLATFORM) \
		--privileged \
		-v /sys/fs/cgroup:/sys/fs/cgroup:rw \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		/bin/bash /usr/local/bin/smoke-test.sh
	@echo "==> Smoke tests passed"

## bundle — Produce a RAUC bundle (.raucb)
bundle: test
	@echo "==> Building RAUC bundle..."
	@mkdir -p $(OUTPUT_DIR)
	docker run --rm --platform $(PLATFORM) \
		-v $(OUTPUT_DIR):/output \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		/bin/bash -c "rauc bundle /output/rootfs /output/$(BUNDLE_NAME)"
	@echo "==> RAUC bundle: $(OUTPUT_DIR)/$(BUNDLE_NAME)"

## clean — Remove build artifacts
clean:
	@echo "==> Cleaning build artifacts..."
	rm -rf $(OUTPUT_DIR)
	docker rmi $(IMAGE_NAME):$(IMAGE_TAG) 2>/dev/null || true
	@echo "==> Clean complete"
