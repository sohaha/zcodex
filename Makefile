OUT ?= /Users/chenwenjie/bin/codex
OUT_SERVE ?= /Users/chenwenjie/bin/codex-serve

.PHONY: write-serve-web-assets
write-serve-web-assets:
	cd web && npm ci && npm run build
	rm -rf codex-rs/serve/assets/web
	cp -R web/dist codex-rs/serve/assets/web

.PHONY: release-codex-web
release-codex-web: write-serve-web-assets release-codex release-codex-serve

.PHONY: release-codex
release-codex:
	env -u CARGO_PROFILE_RELEASE_LTO \
		-u CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
		-u CARGO_PROFILE_RELEASE_DEBUG \
		-u CARGO_PROFILE_RELEASE_STRIP \
		sh -c 'cd codex-rs && cargo build -p codex-cli --bin codex --release'
	mkdir -p "$(dir $(OUT))"
	install -m 755 codex-rs/target/release/codex "$(OUT)"

.PHONY: release-codex-serve
release-codex-serve:
	env -u CARGO_PROFILE_RELEASE_LTO \
		-u CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
		-u CARGO_PROFILE_RELEASE_DEBUG \
		-u CARGO_PROFILE_RELEASE_STRIP \
		sh -c 'cd codex-rs && cargo build -p codex-serve --bin codex-serve --release'
	mkdir -p "$(dir $(OUT_SERVE))"
	install -m 755 codex-rs/target/release/codex-serve "$(OUT_SERVE)"
