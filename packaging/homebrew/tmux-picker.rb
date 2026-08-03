# Homebrew formula for tmux-picker.
#
# No tap is set up yet, so install directly from this file:
#   brew install --formula https://raw.githubusercontent.com/jalsarraf0/tmux-picker/main/packaging/homebrew/tmux-picker.rb
# or, from a local checkout:
#   brew install --formula ./packaging/homebrew/tmux-picker.rb
#
# Builds from source via cargo (no bottle/CI infra yet, so every install
# compiles locally — a couple of minutes on typical hardware).
#
# NOTE: authored and sha256-verified against the GitHub source tarball for
# v1.2.1, but not build-tested on real macOS (no macOS available in the
# environment this was written in). The crate itself only depends on
# ratatui/crossterm/clap/toml/nucleo-matcher/signal-hook/libc, all of which
# support macOS, so it should build cleanly — but treat this formula as
# unverified on macOS until someone confirms a real `brew install` run.
class TmuxPicker < Formula
  desc "TUI session picker for tmux on SSH login and local terminals"
  homepage "https://github.com/jalsarraf0/tmux-picker"
  url "https://github.com/jalsarraf0/tmux-picker/archive/refs/tags/v1.2.1.tar.gz"
  sha256 "04c9be6c51b87820df59adbbdc2cb1f745b2cd1680406fe16b9a670a8d210a56"
  license "MIT"
  head "https://github.com/jalsarraf0/tmux-picker.git", branch: "main"

  depends_on "rust" => :build
  depends_on "tmux"

  def install
    system "cargo", "install", *std_cargo_args
    (share/"tmux-picker").install "shell/tmux-autoattach.sh"
  end

  def caveats
    <<~EOS
      tmux-picker is installed, but the auto-attach hook isn't wired into
      your shell yet (Homebrew formulae don't edit your dotfiles). Add this
      to ~/.bashrc (or ~/.bash_profile for a login shell) to run it on every
      new shell:

        [ -r "#{opt_share}/tmux-picker/tmux-autoattach.sh" ] && . "#{opt_share}/tmux-picker/tmux-autoattach.sh"

      Default behavior fires on every new interactive shell (trigger_mode =
      "always"). To restrict it to SSH logins only, run:

        tmux-picker --init
        # then edit ~/.config/tmux-picker/config.toml:
        #   trigger_mode = "ssh_only"
    EOS
  end

  test do
    assert_match "tmux-picker #{version}", shell_output("#{bin}/tmux-picker --version")
    assert_match "always", shell_output("#{bin}/tmux-picker --print-trigger-mode").strip
  end
end
