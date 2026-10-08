# Build pipeline. Outputs land in web/, which the Worker serves as static assets.

# Local builds are wasm-dev: a change rebuilds in seconds (Cargo.toml says why). What players
# download is wasm-release, small but minutes to build: deploy and preview build it whatever
# PROFILE says, and so does CI. `make dev PROFILE=wasm-release` tries it locally.
PROFILE ?= wasm-dev
# Worked out when used (=, not :=), so deploy's and preview's own PROFILE counts.
PROFILE_DIR = $(if $(filter dev,$(PROFILE)),debug,$(PROFILE))

# wasm-bindgen-cli must match the wasm-bindgen crate in Cargo.lock, so install it locally, one
# folder per version: a version bump installs the new one, other Cargo.lock changes don't.
WASM_BINDGEN_VERSION := $(shell awk '/^name = "wasm-bindgen"$$/{getline; gsub(/"/, "", $$3); print $$3; exit}' Cargo.lock)
WASM_BINDGEN := .tools/wasm-bindgen-$(WASM_BINDGEN_VERSION)/bin/wasm-bindgen
# wasm-opt comes with the pinned emsdk.
WASM_OPT := emulator/.cache/emsdk/upstream/bin/wasm-opt

.PHONY: client netplay emulator emulator-remote upload-emulator supermodel supermodel-remote upload-supermodel \
	mame mame-remote upload-mame daytona daytona-remote upload-daytona upload-daytona-states upload-rom dev deploy preview \
	editor editor-web editor-dev editor-deploy editor-preview pull-map e2e

$(WASM_BINDGEN):
	cargo install wasm-bindgen-cli --version $(WASM_BINDGEN_VERSION) --root .tools/wasm-bindgen-$(WASM_BINDGEN_VERSION) --locked

$(WASM_OPT):
	./emulator/emsdk.sh

# Bevy client -> web/pkg/ (https://github.com/bevyengine/bevy/tree/latest/examples#wasm),
# plus its art -> web/assets/. wasm-release is shrunk with wasm-opt. Any other build is too big
# to serve (Workers static assets take files up to 25 MiB), so it's gzipped instead, and the
# page unpacks it as it loads: web/pkg/build.js tells it which. Any build but wasm-release also
# has the hooks browser tests use (client/src/testing.rs, tools/e2e).
client: $(WASM_BINDGEN) $(WASM_OPT)
	rm -rf web/assets && cp -R assets web/assets
	cargo build -p client --profile $(PROFILE) --target wasm32-unknown-unknown \
		$(if $(filter wasm-release,$(PROFILE)),,--features test-hooks)
	rm -rf web/pkg
	$(WASM_BINDGEN) --out-dir web/pkg --target web \
		target/wasm32-unknown-unknown/$(PROFILE_DIR)/client.wasm
	$(if $(filter wasm-release,$(PROFILE)),$(WASM_OPT) -Oz --output web/pkg/client_bg.opt.wasm web/pkg/client_bg.wasm && mv web/pkg/client_bg.opt.wasm web/pkg/client_bg.wasm,gzip -1 web/pkg/client_bg.wasm)
	echo "export const gzipped = $(if $(filter wasm-release,$(PROFILE)),false,true);" > web/pkg/build.js

# GGRS rollback for the emulator worker (netplay/src/lib.rs) -> web/netplay/.
netplay: $(WASM_BINDGEN) $(WASM_OPT)
	cargo build -p netplay --profile wasm-release --target wasm32-unknown-unknown
	$(WASM_BINDGEN) --out-dir web/netplay --target web \
		target/wasm32-unknown-unknown/wasm-release/netplay.wasm
	$(WASM_OPT) -Oz --output web/netplay/netplay_bg.opt.wasm web/netplay/netplay_bg.wasm
	mv web/netplay/netplay_bg.opt.wasm web/netplay/netplay_bg.wasm

# FBNeo cores -> emulator/dist/<core>/, then into local R2 (served at /fbneo/<core>/* by
# `make dev BUCKET=local`). Only for working on the emulator: `make dev` plays the site's cores.
emulator:
	./emulator/build.sh
	$(MAKE) upload-emulator R2_TARGET=--local

# Production R2 (create the bucket once: cd server && npx wrangler r2 bucket create vab).
emulator-remote:
	$(MAKE) upload-emulator R2_TARGET=--remote

# The Supermodel (Sega Model 3) core -> supermodel/dist/, then into local R2 (served at
# /supermodel/*), like the FBNeo cores above. supermodel-remote uploads to production.
supermodel:
	./supermodel/build.sh web
	$(MAKE) upload-supermodel R2_TARGET=--local

supermodel-remote:
	$(MAKE) upload-supermodel R2_TARGET=--remote

# The MAME core (Namco System 12 and System 23: Tekken 3, Time Crisis II) -> mame/dist/, then
# into local R2 (served at /mame/*), like Supermodel above. mame-remote uploads to production.
mame:
	./mame/build.sh
	$(MAKE) upload-mame R2_TARGET=--local

mame-remote:
	$(MAKE) upload-mame R2_TARGET=--remote
# The Daytona USA (Sega Model 2) core -> daytona/dist/, then into local R2 (served at
# /daytona/*), the same way. daytona-remote uploads to production. Its ROM set is MAME's
# `daytona`: make upload-rom ROM=$HOME/Downloads/daytona.zip.
daytona:
	./daytona/build.sh web
	$(MAKE) upload-daytona R2_TARGET=--local

daytona-remote:
	$(MAKE) upload-daytona R2_TARGET=--remote

# A ROM set (or its start-up .state) into R2, served at /roms/<file>:
# make upload-rom ROM=$HOME/Downloads/mk2.zip (local R2, for `make dev BUCKET=local`; add
# R2_TARGET=--remote for production, and R2_BUCKET=vab-preview for the preview Worker's bucket).
R2_TARGET ?= --local
R2_BUCKET ?= vab
upload-rom:
	cd server && npx wrangler r2 object put $(R2_BUCKET)/roms/$(notdir $(ROM)) $(R2_TARGET) \
		--file $(abspath $(ROM)) \
		--content-type $(if $(filter %.zip,$(ROM)),application/zip,application/octet-stream)

upload-supermodel:
	cd server && npx wrangler r2 object put $(R2_BUCKET)/supermodel/supermodel.mjs $(R2_TARGET) \
		--file ../supermodel/dist/supermodel.mjs --content-type text/javascript && \
	npx wrangler r2 object put $(R2_BUCKET)/supermodel/supermodel.wasm $(R2_TARGET) \
		--file ../supermodel/dist/supermodel.wasm --content-type application/wasm

upload-mame:
	cd server && npx wrangler r2 object put $(R2_BUCKET)/mame/mame.mjs $(R2_TARGET) \
		--file ../mame/dist/mame.mjs --content-type text/javascript && \
	npx wrangler r2 object put $(R2_BUCKET)/mame/mame.wasm $(R2_TARGET) \
		--file ../mame/dist/mame.wasm --content-type application/wasm
upload-daytona:
	cd server && npx wrangler r2 object put $(R2_BUCKET)/daytona/daytona.mjs $(R2_TARGET) \
		--file ../daytona/dist/daytona.mjs --content-type text/javascript && \
	npx wrangler r2 object put $(R2_BUCKET)/daytona/daytona.wasm $(R2_TARGET) \
		--file ../daytona/dist/daytona.wasm --content-type application/wasm

# Daytona USA's arcade-mode seat states (node daytona/make-states.mjs; ROM-derived, like the ROM
# set) into R2 next to it, served at /roms/daytona.seat<k>.state: one per seat 0-7.
# make upload-daytona-states [R2_TARGET=--remote] [R2_BUCKET=vab-preview]
upload-daytona-states:
	cd server && for k in 0 1 2 3 4 5 6 7; do \
		npx wrangler r2 object put $(R2_BUCKET)/roms/daytona.seat$$k.state $(R2_TARGET) \
			--file ../daytona/dist/states/daytona.seat$$k.state --content-type application/octet-stream || exit 1; \
	done

upload-emulator:
	cd server && for dir in ../emulator/dist/*/; do core=$$(basename $$dir); \
		npx wrangler r2 object put $(R2_BUCKET)/fbneo/$$core/fbneo.mjs $(R2_TARGET) \
			--file $$dir/fbneo.mjs --content-type text/javascript && \
		npx wrangler r2 object put $(R2_BUCKET)/fbneo/$$core/fbneo.wasm $(R2_TARGET) \
			--file $$dir/fbneo.wasm --content-type application/wasm || exit 1; \
	done

# Worker + Durable Object, serving web/ (http://localhost:8787). Cores, ROMs and start-up states
# come from the site's own R2 bucket (`remote` in wrangler.toml; `npx wrangler login` once), so
# games play with nothing built or uploaded, and the map is assets/maps/bar.ron, not the one
# saved on the site. BUCKET=local uses the local bucket instead, map included: for a core from
# `make emulator`, or a map saved from `make editor-dev`.
BUCKET ?= remote
dev: client netplay
	cd server && npx wrangler dev $(if $(filter local,$(BUCKET)),--local,--var MAP_FROM_R2:false)

# Browser tests (tools/e2e/README.md), against `make dev` running in another terminal.
e2e:
	cd tools/e2e && npm install --silent && \
		for test in $$(ls *.mjs | grep -v '^lib.mjs$$'); do node $$test || exit 1; done

deploy: PROFILE = wasm-release
deploy: client netplay
	cd server && npx wrangler deploy

# The preview Worker (server/wrangler.toml). Its bucket gets cores and ROMs like the main one:
# make emulator-remote R2_BUCKET=vab-preview, make upload-rom R2_TARGET=--remote R2_BUCKET=vab-preview ...
preview: PROFILE = wasm-release
preview: client netplay
	cd server && npx wrangler deploy --env preview

# Bar layout editor on the desktop. Saves assets/maps/bar.ron; draw mode writes art/.
editor:
	cargo run -p editor

# The editor for the web -> tools/editor/web/pkg/, with the tiles it paints with and the map it
# starts from (once a map has been saved there, the Worker serves that one instead). Built and
# gzipped like the client: editor-deploy and editor-preview build wasm-release.
EDITOR_WEB := tools/editor/web
editor-web: $(WASM_BINDGEN) $(WASM_OPT)
	rm -rf $(EDITOR_WEB)/assets && mkdir -p $(EDITOR_WEB)/assets
	cp -R assets/tiles assets/maps $(EDITOR_WEB)/assets/
	cargo build -p editor --profile $(PROFILE) --target wasm32-unknown-unknown
	rm -rf $(EDITOR_WEB)/pkg
	$(WASM_BINDGEN) --out-dir $(EDITOR_WEB)/pkg --target web \
		target/wasm32-unknown-unknown/$(PROFILE_DIR)/editor.wasm
	$(if $(filter wasm-release,$(PROFILE)),$(WASM_OPT) -Oz --output $(EDITOR_WEB)/pkg/editor_bg.opt.wasm $(EDITOR_WEB)/pkg/editor_bg.wasm && mv $(EDITOR_WEB)/pkg/editor_bg.opt.wasm $(EDITOR_WEB)/pkg/editor_bg.wasm,gzip -1 $(EDITOR_WEB)/pkg/editor_bg.wasm)
	echo "export const gzipped = $(if $(filter wasm-release,$(PROFILE)),false,true);" > $(EDITOR_WEB)/pkg/build.js

# The editor Worker serving tools/editor/web/ at http://localhost:8788, next to `make dev`.
# It saves into the local R2 bucket, never the site's, so a map saved here shows in the local
# bar when that runs with BUCKET=local.
editor-dev: editor-web
	cd server && npx wrangler dev --env editor --port 8788

# https://vab-editor.<account>.workers.dev (merging to main does this too).
editor-deploy: PROFILE = wasm-release
editor-deploy: editor-web
	cd server && npx wrangler deploy --env editor

# https://vab-editor-preview.<account>.workers.dev, saving into the preview site's bucket.
editor-preview: PROFILE = wasm-release
editor-preview: editor-web
	cd server && npx wrangler deploy --env editor-preview

# The map last saved from the web editor, into the repo to commit it.
pull-map:
	cd server && npx wrangler r2 object get vab/maps/bar.ron --remote --file ../assets/maps/bar.ron
