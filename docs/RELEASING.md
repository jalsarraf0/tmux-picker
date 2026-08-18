# Releasing (maintainer notes)

1. Bump `version` in `Cargo.toml`.
2. Update `packaging/PKGBUILD`'s `pkgver` to match.
3. `cargo publish --dry-run` and resolve any warnings.
4. Tag and push: `git tag -s -m "Release v$VERSION" v$VERSION && git push origin v$VERSION`.
5. Build the per-target release tarball and attach it to the GitHub release
   so `cargo binstall` and the AUR `PKGBUILD` can find it. It must contain
   `tmux-picker`, a **flat** `tmux-autoattach.sh` (not nested under
   `shell/`), `LICENSE`, and `README.md`, all directly inside a directory
   named `tmux-picker-$VERSION-$TARGET/`.
6. Build the native packages: `bash packaging/build-native-packages.sh`
   (needs [`fpm`](https://fpm.readthedocs.io) — produces both `.deb` and
   `.rpm` into `dist/`). Sanity-check them before attaching — the
   post-install/pre-remove scripts wire and unwire a system-wide bashrc
   block, so at minimum verify in a throwaway container:
   ```bash
   docker run --rm -v "$PWD/dist:/pkgs:ro" fedora:latest \
     bash -c 'dnf install -y /pkgs/tmux-picker-*.rpm && grep -A2 "BEGIN tmux-picker" /etc/bashrc'
   docker run --rm -v "$PWD/dist:/pkgs:ro" debian:latest \
     bash -c 'apt-get update -qq && apt-get install -y /pkgs/tmux-picker_*.deb && grep -A2 "BEGIN tmux-picker" /etc/bash.bashrc'
   ```
   Also check the *reinstall/upgrade* case doesn't strip the block (the
   `--before-remove` script only strips on a genuine removal — rpm passes
   `$1=0` for that, deb passes `remove`/`purge` — never on upgrade).
7. `gh release create v$VERSION dist/*.deb dist/*.rpm tmux-picker-$VERSION-$TARGET.tar.gz ...`
   (or `gh release upload` onto an already-created release).
8. `cargo publish` to crates.io.
9. Submit / refresh the AUR package — clone
   `ssh://aur@aur.archlinux.org/tmux-picker-bin.git`, copy in `PKGBUILD` +
   `tmux-picker.install`, regenerate `.SRCINFO`
   (`makepkg --printsrcinfo > .SRCINFO`), commit, push. Requires an AUR
   account with a registered SSH key.
