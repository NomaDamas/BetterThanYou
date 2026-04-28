class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.8.8.tar.gz"
  version "0.8.8"
  sha256 "1631ebf67dfca9f34cd28fc9fd17fcd2d173495637964f3f0f7363f5b8cbd0d4"
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
