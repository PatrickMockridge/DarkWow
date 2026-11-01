.POSIX:

# Compile target for docs
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

DWOWD_JSONRPC = src/clients/dwowd_jsonrpc.md

JSONRPC = \
	$(DWOWD_JSONRPC)

all: $(JSONRPC)
	mdbook build
	cp -f theme/logo* book/
	$(MAKE) -C ../ rustdoc
	cp -r ../target/$(RUST_TARGET)/doc/* book/dev/
	./remove_chapter_nav_js.py

# The book only: no cargo/rustdoc, and no generated seminar page. The seminar
# generator (generate_seminar_ics.py) was retired in July 2023 and wrote an
# unlisted src/dev/seminars.md; the dead darkfid JSON-RPC rule built from
# ../bin/darkfid, which no longer exists. Use `book` for documentation work.
book:
	mdbook build
	cp -f theme/logo* book/

clean:
	rm -rf book

github: all
	./ghp_push.py

.PHONY: $(JSONRPC) all book clean github
