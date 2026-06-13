TESTDATA_REPO  := o3co/xx.hocon
TESTDATA_REF   := main
TESTDATA_DIR   := tests/testdata/hocon
EXPECTED_DIR   := tests/testdata/expected
# Fetched-only (gitignored) subdir used as the "fixtures already present"
# sentinel. tests/testdata/hocon/ also holds git-tracked vendored fixtures, so
# the dir's mere existence is NOT evidence the fetched-only fixtures (which the
# conformance tests need) were downloaded — only a fetched-only subdir is.
FETCH_SENTINEL := $(TESTDATA_DIR)/concat-errors

.PHONY: testdata test

# Fetch conformance fixtures from xx.hocon. No CI cache is involved: the SHA is
# resolved once via `git ls-remote` (not the REST API, so no unauthenticated
# rate-limit), the archive is downloaded for that EXACT SHA (pin always matches
# content), and the same SHA is written to .xx-hocon-version. `set -e` makes any
# failed step abort rather than silently leaving partial/empty state. The
# early-exit skips the download only when the fetched-only sentinel is present
# AND the pin already matches the resolved SHA (local-dev convenience; in CI the
# checkout is always fresh so the sentinel is absent and the fetch always runs).
testdata:
	@set -e; \
	sha="$$(git ls-remote "https://github.com/$(TESTDATA_REPO).git" "$(TESTDATA_REF)" | head -1 | cut -f1)"; \
	if [ -z "$$sha" ]; then \
	  echo "error: could not resolve $(TESTDATA_REPO)@$(TESTDATA_REF) SHA via git ls-remote" >&2; \
	  exit 1; \
	fi; \
	if [ -f .xx-hocon-version ] && [ -d "$(EXPECTED_DIR)" ] && [ -d "$(FETCH_SENTINEL)" ] && \
	   [ "$$sha" = "$$(cat .xx-hocon-version)" ]; then \
	  echo "Fixtures up to date ($$sha)"; \
	  exit 0; \
	fi; \
	tmpdir="$$(mktemp -d)"; \
	trap 'rm -rf "$$tmpdir"' EXIT INT TERM; \
	mkdir -p "$(TESTDATA_DIR)" "$(EXPECTED_DIR)"; \
	curl -sfL "https://github.com/$(TESTDATA_REPO)/archive/$$sha.tar.gz" -o "$$tmpdir/archive.tar.gz"; \
	tar xzf "$$tmpdir/archive.tar.gz" -C "$$tmpdir" --strip-components=1; \
	cp -R "$$tmpdir/testdata/hocon/." "$(TESTDATA_DIR)/"; \
	cp -R "$$tmpdir/expected/hocon/." "$(EXPECTED_DIR)/"; \
	printf '%s\n' "$$sha" > .xx-hocon-version; \
	echo "Done. Fetched $$sha"

test:
	cargo test
