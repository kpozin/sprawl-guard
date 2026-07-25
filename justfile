# Prepare the frozen Cargo publication candidate without modifying crates.io.
cargo-release-prepare release_commit release_tag: _release-tools-install
    node npm/scripts/publish-cargo-crates.ts \
        prepare \
        {{quote(release_commit)}} \
        {{quote(release_tag)}}

# Publish the frozen Cargo candidate to crates.io.
cargo-release-publish release_commit release_tag:
    node npm/scripts/publish-cargo-crates.ts \
        publish \
        {{quote(release_commit)}} \
        {{quote(release_tag)}}

_release-tools-install:
    pnpm --dir npm install --frozen-lockfile --config.confirmModulesPurge=false
