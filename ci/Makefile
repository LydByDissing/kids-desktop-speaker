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

.PHONY: all build test test-armv6 hw-test hw-ab-test hw-ota-test sd-image bundle export-rootfs clean cross-build vm-image vm smoke ci-test

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

IMAGE_NAME    := kids-desktop-speaker
IMAGE_TAG     := latest
PLATFORM      := linux/arm/v6
QEMU_CPU      := arm1176
BASE_TAR      := base/raspbian-bookworm-armhf.tar
# Extra docker buildx flags, such as layer caching in CI. Empty for local builds.
BUILDX_CACHE  ?=
DOCKERFILE    := Dockerfile.pi-image
OUTPUT_DIR    ?= $(CURDIR)/output

# Source dir for cross-compiled service binaries, relative to the build
# context (repo root). Default = local/VM debug build (DEC-022). The release
# pipeline overrides this to ci/binaries (--release artifacts, DEC-007).
BIN_DIR       ?= target/arm-unknown-linux-gnueabihf/debug

# Bundle version, written into the RAUC manifest. The release pipeline passes
# the git tag (github.ref_name); locally it derives from git describe.
VERSION       ?= $(shell git -C .. describe --tags --always --dirty 2>/dev/null || echo 0.1.0-dev)

# ---------------------------------------------------------------------------
# Targets
# ---------------------------------------------------------------------------

all: build test bundle

## base — Build the ARMv6 Raspbian root filesystem if it is missing (DEC-024)
$(BASE_TAR):
	@bash base/build-base.sh $(BASE_TAR)

## build — Build the Pi OS Docker image for ARM
build: $(BASE_TAR)
	@echo "==> Building Pi OS image ($(PLATFORM))..."
	docker buildx build \
		--platform $(PLATFORM) \
		-f $(DOCKERFILE) \
		--build-arg BIN_DIR=$(BIN_DIR) \
		--build-arg VERSION=$(VERSION) \
		$(BUILDX_CACHE) \
		-t $(IMAGE_NAME):$(IMAGE_TAG) \
		--load \
		..
	@echo "==> Build complete: $(IMAGE_NAME):$(IMAGE_TAG)"

## test — Run smoke tests against the built image
test:
	@echo "==> Running smoke tests..."
	docker run --rm --platform $(PLATFORM) \
		-e QEMU_CPU=$(QEMU_CPU) \
		--entrypoint /bin/bash \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		/usr/local/bin/smoke-test.sh
	@echo "==> Smoke tests passed"

## test-armv6 — Run the image's programs on an emulated Pi Zero W CPU
test-armv6:
	@bash scripts/test-armv6.sh

## hw-test — Run the image's programs on a real Pi Zero W: make hw-test PI_HOST=user@host
hw-test:
	@PI_HOST=$(PI_HOST) bash scripts/hw-test.sh

## hw-ab-test — Prove A/B updates and rollback on a real Pi: make hw-ab-test PI_HOST=root@host
hw-ab-test:
	@PI_HOST=$(PI_HOST) bash scripts/hw-ab-test.sh

## hw-ota-test — Prove the OTA updater end to end on a real Pi: make hw-ota-test PI_HOST=root@host
hw-ota-test:
	@PI_HOST=$(PI_HOST) bash scripts/hw-ota-test.sh

## export-rootfs — Export the built image's root filesystem to a tar for bundling
export-rootfs:
	@echo "==> Exporting rootfs from $(IMAGE_NAME):$(IMAGE_TAG)..."
	mkdir -p $(OUTPUT_DIR)
	cid=$$(docker create --platform $(PLATFORM) $(IMAGE_NAME):$(IMAGE_TAG)); \
	docker export "$$cid" > $(OUTPUT_DIR)/rootfs-raw.tar; \
	docker rm "$$cid" >/dev/null 2>&1 || { sleep 3; docker rm -f "$$cid" >/dev/null 2>&1 || true; }
	python3 scripts/clean-rootfs.py $(OUTPUT_DIR)/rootfs-raw.tar $(OUTPUT_DIR)/rootfs.tar
	rm -f $(OUTPUT_DIR)/rootfs-raw.tar
	@echo "==> rootfs exported: $(OUTPUT_DIR)/rootfs.tar ($$(du -sh $(OUTPUT_DIR)/rootfs.tar | cut -f1))"

## bundle — Build RAUC bundle (.raucb) of the real rootfs, signed with throwaway dev certs
bundle: export-rootfs
	mkdir -p $(OUTPUT_DIR)
	docker run --rm --platform $(PLATFORM) \
		-e QEMU_CPU=$(QEMU_CPU) \
		--tmpfs /verify:size=1g \
		-v $(OUTPUT_DIR):/output \
		-v $(CURDIR)/create-bundle.sh:/usr/local/bin/create-bundle.sh:ro \
		-e VERSION=$(VERSION) \
		-e RAUC_SIGNING_KEY \
		-e RAUC_SIGNING_CERT \
		-e RAUC_REQUIRE_STABLE_KEY \
		--entrypoint /bin/bash \
		$(IMAGE_NAME):$(IMAGE_TAG) \
		/usr/local/bin/create-bundle.sh

## sd-image — Build a flashable SD card image from the built image (DEC-025)
sd-image: export-rootfs
	@bash scripts/build-sd-image.sh $(OUTPUT_DIR)/rootfs.tar $(OUTPUT_DIR)/kds-sdcard.img.xz
	@bash scripts/test-sd-image.sh $(OUTPUT_DIR)/kds-sdcard.img.xz

## clean — Remove build artifacts
clean:
	@echo "==> Cleaning build artifacts..."
	docker rmi $(IMAGE_NAME):$(IMAGE_TAG) 2>/dev/null || true
	rm -rf $(OUTPUT_DIR)
	@echo "==> Clean complete"

## cross-build — Cross-compile all workspace binaries for ARMv6
cross-build:
	@echo "==> Cross-compiling for arm-unknown-linux-gnueabihf..."
	cd .. && cross build --target arm-unknown-linux-gnueabihf
	@echo "==> Cross-build complete"

## vm-image — Build QEMU ARM rootfs image (depends on cross-build)
vm-image: cross-build $(BASE_TAR)
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
