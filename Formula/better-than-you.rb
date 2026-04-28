class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.8.0.tar.gz"
  version "0.8.0"
  sha256 "446f7b59a6abc51ca47f7086dfa9f93c612e883f4b52e73099d650f53d8865ca"
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
