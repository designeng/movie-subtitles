.PHONY: dev release

# make dev                   — run the app in development mode (tauri dev)
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
