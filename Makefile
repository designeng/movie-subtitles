.PHONY: dev release install

# make dev                   — run the app in development mode (tauri dev)
# make install               — install the latest GitHub release into /Applications (macOS)
# make install VERSION=0.1.7 — install a specific release
# make release               — bump patch (0.1.0 → 0.1.1), commit, tag and push
# make release VERSION=0.2.0 — same, with an explicit version
CURRENT := $(shell node -p "require('./src-tauri/tauri.conf.json').version")
VERSION ?= $(shell node -p "'$(CURRENT)'.split('.').map((n, i) => i == 2 ? +n + 1 : n).join('.')")
TAG := v$(VERSION)

dev:
	pnpm tauri dev

release:
	@test -z "$$(git status --porcelain)" || { echo "Working tree is not clean"; exit 1; }
	@! git rev-parse -q --verify "refs/tags/$(TAG)" >/dev/null || { echo "Tag $(TAG) already exists"; exit 1; }
	sed -i '' 's/"version": "$(CURRENT)"/"version": "$(VERSION)"/' package.json src-tauri/tauri.conf.json
	sed -i '' '1,/^version = /s/^version = ".*"/version = "$(VERSION)"/' src-tauri/Cargo.toml
	cargo update -p movie-subtitles --offline --manifest-path src-tauri/Cargo.toml
	git commit -am "Release $(TAG)"
	git tag $(TAG)
	git push origin HEAD $(TAG)

REPO := designeng/movie-subtitles
APP := /Applications/Movie Subtitles.app
ASSET := Movie.Subtitles_universal.app.tar.gz
ifeq ($(origin VERSION),command line)
DOWNLOAD_URL := https://github.com/$(REPO)/releases/download/v$(VERSION)/$(ASSET)
else
DOWNLOAD_URL := https://github.com/$(REPO)/releases/latest/download/$(ASSET)
endif

install:
	@test "$$(uname)" = Darwin || { echo "make install supports macOS only"; exit 1; }
	@tmp=$$(mktemp -d) && trap 'rm -rf "$$tmp"' EXIT && \
		echo "Downloading $(DOWNLOAD_URL)" && \
		curl -fL "$(DOWNLOAD_URL)" | tar -xz -C "$$tmp" && \
		rm -rf "$(APP)" && \
		mv "$$tmp/Movie Subtitles.app" /Applications/ && \
		xattr -dr com.apple.quarantine "$(APP)" && \
		echo "Installed $(APP)"
