TESTDATA_REPO  := o3co/xx.hocon
TESTDATA_REF   := main
EXPECTED_DIR   := tests/testdata/expected

.PHONY: testdata test

testdata:
	@if [ -f .xx-hocon-version ] && [ -d "$(EXPECTED_DIR)" ]; then \
	  remote_sha=$$(curl -s "https://api.github.com/repos/$(TESTDATA_REPO)/commits/$(TESTDATA_REF)" | grep '"sha"' | head -1 | cut -d'"' -f4); \
	  local_sha=$$(cat .xx-hocon-version); \
	  if [ "$$remote_sha" = "$$local_sha" ]; then \
	    echo "Expected JSON up to date ($$local_sha)"; exit 0; \
	  fi; \
	fi; \
	tmpdir="$$(mktemp -d)"; \
	trap 'rm -rf "$$tmpdir"' EXIT INT TERM; \
	mkdir -p "$(EXPECTED_DIR)"; \
	curl -sL "https://github.com/$(TESTDATA_REPO)/archive/$(TESTDATA_REF).tar.gz" -o "$$tmpdir/archive.tar.gz"; \
	tar xzf "$$tmpdir/archive.tar.gz" -C "$$tmpdir" --strip-components=1; \
	cp -R "$$tmpdir/expected/hocon/." "$(EXPECTED_DIR)/"; \
	curl -s "https://api.github.com/repos/$(TESTDATA_REPO)/commits/$(TESTDATA_REF)" | grep '"sha"' | head -1 | cut -d'"' -f4 > .xx-hocon-version; \
	echo "Done. Fetched $$(cat .xx-hocon-version)"

test:
	cargo test
