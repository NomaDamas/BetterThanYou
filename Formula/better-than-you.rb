class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.8.7.tar.gz"
  version "0.8.7"
  sha256 "39672a6dae54866be984c08103a2529d52b88fe964035407a03e0461472bb89c"
  license "MIT"
  head "https://github.com/NomaDamas/BetterThanYou.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    output = shell_output("#{bin}/better-than-you --help")
    assert_match "BetterThanYou", output
  end
end
